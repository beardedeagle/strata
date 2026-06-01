use super::support::*;
use crate::executable::{
    ExecutableActionPlan, ExecutableNextState, ExecutableProgram, ExecutableSendTarget,
    ExecutableTemplateProgram, ExecutableValueTemplate, ExecutableValueTemplateRef,
};
use crate::program::{
    LoadedAction, LoadedLoopElement, LoadedNextState, LoadedSendTarget, LoadedValueTemplate,
};
use crate::run::run_loaded_program_with_host;
use mantle_artifact::{
    ArtifactValueBooleanOperator, ArtifactValueEqualityOperator, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, EffectOutcomeId, RecordFieldId,
};

#[test]
fn executable_plan_constructs_typed_action_tables_after_admission() {
    let artifact = valid_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let executable =
        ExecutableProgram::from_admitted(&program).expect("executable plan should construct");

    assert_eq!(executable.process_count(), 2);
    assert_eq!(executable.entry().process_id, ProcessId::new(0));
    assert_eq!(executable.entry().message_id, MessageId::new(0));

    let transition = executable
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("entry transition should dispatch by typed ids");
    let actions = transition
        .actions()
        .all_actions(executable.actions())
        .map(|(_, action)| action)
        .collect::<Vec<_>>();

    match actions[0] {
        ExecutableActionPlan::Spawn {
            target,
            process_ref,
            spawn,
        } => {
            assert_eq!(*target, ProcessId::new(1));
            assert_eq!(process_ref.id, ProcessRefId::new(0));
            assert_eq!(process_ref.target_process, ProcessId::new(1));
            assert_eq!(spawn.id, SPAWN_SITE);
            assert_eq!(spawn.authority, SPAWN_AUTHORITY);
        }
        action => panic!("expected planned spawn action, got {action:?}"),
    }
    match actions[1] {
        ExecutableActionPlan::Send {
            target: ExecutableSendTarget::ProcessRef(process_ref),
            message,
            ..
        } => {
            assert_eq!(*message, MessageId::new(0));
            assert_eq!(process_ref.id, ProcessRefId::new(0));
            assert_eq!(process_ref.target_process, ProcessId::new(1));
        }
        action => panic!("expected planned send action, got {action:?}"),
    }
}

#[test]
fn executable_plan_order_is_deterministic_when_loaded_transition_order_changes() {
    let artifact = sequence_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let baseline = ExecutableProgram::from_admitted(&program)
        .expect("baseline executable plan should construct")
        .transition_signature();

    let mut reordered_artifact = sequence_artifact();
    reordered_artifact.processes[1].transitions.reverse();
    let reordered_program =
        LoadedProgram::from_artifact(&reordered_artifact).expect("reordered artifact should load");
    let reordered = ExecutableProgram::from_admitted(&reordered_program)
        .expect("reordered executable plan should construct")
        .transition_signature();

    assert_eq!(baseline, reordered);
}

#[test]
fn executable_plan_ignores_stale_loaded_transition_lookup() {
    let artifact = sequence_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions.swap(0, 1);
    let mut host = InMemoryRuntimeHost::default();

    let report = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("executable plan should rebuild dispatch from current loaded transitions");

    assert_eq!(
        report.emitted_outputs,
        vec![
            "worker handled First".to_string(),
            "worker handled Second".to_string()
        ]
    );
}

#[test]
fn executable_plan_dispatch_uses_ids_not_debug_labels() {
    let artifact = valid_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].debug_name = "WorkerLabel".to_string();
    program.processes[1].debug_name = "MainLabel".to_string();
    let mut host = InMemoryRuntimeHost::default();

    let report = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("debug labels must remain trace metadata only");

    assert_eq!(report.entry_process, "WorkerLabel");
    assert_eq!(
        report.emitted_outputs,
        vec!["worker handled Ping".to_string()]
    );
    assert!(
        report
            .delivered_messages
            .iter()
            .any(|delivery| delivery.process == "MainLabel" && delivery.message == "Ping")
    );
}

#[test]
fn executable_plan_rejects_invalid_loaded_references_before_artifact_loaded() {
    let artifact = valid_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(99),
        });
    let mut host = InMemoryRuntimeHost::default();

    let err = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect_err("executable plan construction must fail closed");

    assert!(err.to_string().contains("output id 99 is not loaded"));
    assert!(host.events().is_empty());
    assert!(host.stdout().is_empty());
}

#[test]
fn executable_plan_compiles_value_templates_into_template_program() {
    let artifact = payload_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let executable =
        ExecutableProgram::from_admitted(&program).expect("executable plan should construct");

    assert_eq!(executable.templates().len(), 3);

    let entry_transition = executable
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("entry transition should dispatch by typed ids");
    let send_payload = entry_transition
        .actions()
        .all_actions(executable.actions())
        .find_map(|(_, action)| match action {
            ExecutableActionPlan::Send { payload, .. } => *payload,
            _ => None,
        })
        .expect("entry send should have an executable payload template ref");
    match executable
        .templates()
        .get(send_payload)
        .expect("send payload template should resolve")
    {
        ExecutableValueTemplate::Literal { ty, .. } => assert_eq!(*ty, JOB),
        template => panic!("expected literal send payload template, got {template:?}"),
    }

    let worker_transition = executable
        .transition_for_dispatch(ProcessId::new(1), MessageId::new(0), StateId::new(0), None)
        .expect("worker transition should dispatch by typed ids");
    let ExecutableNextState::Template(next_state_template) = worker_transition.next_state() else {
        panic!("worker transition should store an executable next-state template ref")
    };
    match executable
        .templates()
        .get(*next_state_template)
        .expect("next-state template should resolve")
    {
        ExecutableValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            assert_eq!(*ty, WORKER_STATE);
            assert_eq!(*variant, EnumVariantId::new(2));
            match executable
                .templates()
                .get(*payload)
                .expect("nested payload template should resolve")
            {
                ExecutableValueTemplate::ReceivedPayload { ty } => assert_eq!(*ty, JOB),
                template => panic!("expected received-payload template, got {template:?}"),
            }
        }
        template => panic!("expected enum-variant next-state template, got {template:?}"),
    }
}

#[test]
fn executable_template_counts_and_resolves_recursive_nested_refs() {
    const BOOL: TypeId = TypeId::new(8);
    const STATE_MAP: TypeId = TypeId::new(9);

    let mut artifact = valid_artifact();
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact
        .types
        .push(ArtifactType::map("StateMap", WORKER_STATE, WORKER_STATE, 2));
    replace_process_message_variants(
        &mut artifact,
        1,
        vec![ArtifactMessageVariant::payload("AssignMap", STATE_MAP)],
    );
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Map {
            ty: STATE_MAP,
            entries: vec![ArtifactValueTemplateMapEntry {
                key: ArtifactValueTemplate::Literal {
                    ty: WORKER_STATE,
                    value: artifact_value("Ready"),
                },
                value: ArtifactValueTemplate::IfElse {
                    ty: WORKER_STATE,
                    condition: Box::new(ArtifactValueTemplate::BooleanBinary {
                        ty: BOOL,
                        operator: ArtifactValueBooleanOperator::And,
                        left: Box::new(ArtifactValueTemplate::Equality {
                            ty: BOOL,
                            operand_ty: WORKER_STATE,
                            operator: ArtifactValueEqualityOperator::Equal,
                            left: Box::new(ArtifactValueTemplate::Literal {
                                ty: WORKER_STATE,
                                value: artifact_value("Ready"),
                            }),
                            right: Box::new(ArtifactValueTemplate::Literal {
                                ty: WORKER_STATE,
                                value: artifact_value("Ready"),
                            }),
                        }),
                        right: Box::new(ArtifactValueTemplate::Literal {
                            ty: BOOL,
                            value: artifact_value("True"),
                        }),
                    }),
                    then_value: Box::new(ArtifactValueTemplate::RecordField {
                        ty: WORKER_STATE,
                        record: Box::new(ArtifactValueTemplate::ListElement {
                            ty: JOB,
                            list: Box::new(ArtifactValueTemplate::List {
                                ty: JOB_LIST,
                                items: vec![ArtifactValueTemplate::Record {
                                    ty: JOB,
                                    fields: vec![ArtifactValueTemplateField {
                                        field: RecordFieldId::new(0),
                                        value: ArtifactValueTemplate::Literal {
                                            ty: WORKER_STATE,
                                            value: artifact_value("Ready"),
                                        },
                                    }],
                                }],
                            }),
                            index: 0,
                            len: 1,
                        }),
                        field: RecordFieldId::new(0),
                    }),
                    else_value: Box::new(ArtifactValueTemplate::Literal {
                        ty: WORKER_STATE,
                        value: artifact_value("Other"),
                    }),
                },
            }],
        }),
    };
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let executable =
        ExecutableProgram::from_admitted(&program).expect("executable plan should construct");

    assert_eq!(executable.templates().len(), 14);
    let transition = executable
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("entry transition should dispatch by typed ids");
    let send_payload = transition
        .actions()
        .all_actions(executable.actions())
        .find_map(|(_, action)| match action {
            ExecutableActionPlan::Send { payload, .. } => *payload,
            _ => None,
        })
        .expect("entry send should have an executable payload template ref");

    let mut visited = Vec::new();
    assert_template_tree_resolves(executable.templates(), send_payload, &mut visited);
    assert_eq!(visited.len(), executable.templates().len());
}

#[test]
fn executable_plan_compiles_for_each_collection_and_loop_payload_refs() {
    let artifact = for_each_artifact("List[Job{phase:Ready}]", 1);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let executable =
        ExecutableProgram::from_admitted(&program).expect("executable plan should construct");

    assert_eq!(executable.templates().len(), 2);

    let transition = executable
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("entry transition should dispatch by typed ids");
    let for_each = transition
        .actions()
        .all_actions(executable.actions())
        .find_map(|(_, action)| match action {
            ExecutableActionPlan::ForEach {
                collection, body, ..
            } => Some((*collection, *body)),
            _ => None,
        })
        .expect("entry transition should include an executable for-each plan");
    match executable
        .templates()
        .get(for_each.0)
        .expect("collection template should resolve")
    {
        ExecutableValueTemplate::Literal { ty, .. } => assert_eq!(*ty, JOB_LIST),
        template => panic!("expected literal collection template, got {template:?}"),
    }

    let body_payload = for_each
        .1
        .all_actions(executable.actions())
        .find_map(|(_, action)| match action {
            ExecutableActionPlan::Send { payload, .. } => *payload,
            _ => None,
        })
        .expect("for-each body send should have an executable loop-element ref");
    match executable
        .templates()
        .get(body_payload)
        .expect("loop payload template should resolve")
    {
        ExecutableValueTemplate::LoopElement { ty, element } => {
            assert_eq!(*ty, JOB);
            assert_eq!(*element, LoopElementId::new(0));
        }
        template => panic!("expected loop-element payload template, got {template:?}"),
    }
}

#[test]
fn executable_template_rejects_inactive_loop_element_ref_before_artifact_loaded() {
    let artifact = valid_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let process = program
        .process(ProcessId::new(0))
        .expect("main process should load");
    let action = LoadedAction::ForEach {
        element: LoadedLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: loaded_template(ArtifactValueTemplate::Literal {
            ty: JOB_LIST,
            value: artifact_value("List[Job{phase:Ready}]"),
        }),
        max_items: 1,
        body: vec![LoadedAction::IfElse {
            condition: LoadedValueTemplate::LoopElement {
                ty: JOB,
                element: LoopElementId::new(1),
            },
            then_actions: vec![LoadedAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: Vec::new(),
        }],
    };

    let err = ExecutableActionPlan::from_loaded_for_test(&program, process, &action)
        .expect_err("inactive executable loop element refs must fail closed");

    assert!(
        err.to_string().contains("inactive loop element id 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn executable_template_rejects_unbound_effect_outcome_ref_before_artifact_loaded() {
    let artifact = valid_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let process = program
        .process(ProcessId::new(0))
        .expect("main process should load");
    let action = LoadedAction::IfElse {
        condition: LoadedValueTemplate::EffectOutcome {
            ty: WORKER_STATE,
            outcome: EffectOutcomeId::new(0),
        },
        then_actions: vec![LoadedAction::Emit {
            output: OutputId::new(0),
        }],
        else_actions: Vec::new(),
    };

    let err = ExecutableActionPlan::from_loaded_for_test(&program, process, &action)
        .expect_err("unbound executable effect outcome refs must fail closed");

    assert!(
        err.to_string().contains("unbound effect outcome id 0"),
        "unexpected error: {err}"
    );
}

#[test]
fn executable_template_rejects_nested_process_ref_payload_scope() {
    const PROCESS_REF_WORKER: TypeId = TypeId::new(8);
    const REF_ENVELOPE: TypeId = TypeId::new(9);

    let mut artifact = valid_artifact();
    artifact.types.push(ArtifactType::process_ref(
        "ProcessRefWorker",
        ProcessId::new(1),
    ));
    artifact.types.push(ArtifactType::enum_value_with_payloads(
        "RefEnvelope",
        vec![ArtifactEnumVariant {
            label: "ReplyTo".to_string(),
            payload_type: Some(PROCESS_REF_WORKER),
        }],
    ));
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let process = program
        .process(ProcessId::new(0))
        .expect("main process should load");
    let action = LoadedAction::Send {
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: MessageId::new(0),
        payload: Some(LoadedValueTemplate::EnumVariant {
            ty: REF_ENVELOPE,
            variant: EnumVariantId::new(0),
            payload: Box::new(LoadedValueTemplate::ProcessRef {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }),
        }),
    };

    let err = ExecutableActionPlan::from_loaded_for_test_with_spawned_refs(
        &program,
        process,
        &action,
        &[true],
    )
    .expect_err("nested executable process refs must fail closed");

    assert!(
        err.to_string().contains("must be a direct message payload"),
        "unexpected error: {err}"
    );
}

#[test]
fn executable_template_rejects_projected_process_ref_payload_scope() {
    const PROCESS_REF_WORKER: TypeId = TypeId::new(8);
    const REF_ENVELOPE: TypeId = TypeId::new(9);

    let mut artifact = valid_artifact();
    artifact.types.push(ArtifactType::process_ref(
        "ProcessRefWorker",
        ProcessId::new(1),
    ));
    artifact.types.push(ArtifactType::enum_value_with_payloads(
        "RefEnvelope",
        vec![ArtifactEnumVariant {
            label: "ReplyTo".to_string(),
            payload_type: Some(PROCESS_REF_WORKER),
        }],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].message_variants[0].payload_type = Some(REF_ENVELOPE);
    let process = program
        .process(ProcessId::new(0))
        .expect("main process should load");
    let action = LoadedAction::Send {
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: MessageId::new(0),
        payload: Some(LoadedValueTemplate::EnumPayload {
            ty: PROCESS_REF_WORKER,
            value: Box::new(LoadedValueTemplate::ReceivedPayload { ty: REF_ENVELOPE }),
            variant: EnumVariantId::new(0),
        }),
    };

    let err = ExecutableActionPlan::from_loaded_for_test(&program, process, &action)
        .expect_err("projected executable process refs must fail closed");

    assert!(
        err.to_string()
            .contains("process reference template must be a direct message payload"),
        "unexpected error: {err}"
    );
}

#[test]
fn executable_template_rejects_invalid_variant_ref_before_artifact_loaded() {
    let artifact = payload_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let LoadedNextState::Template(LoadedValueTemplate::EnumVariant { variant, .. }) =
        &mut program.processes[1].transitions[0].next_state
    else {
        panic!("payload artifact should use an enum-variant next-state template")
    };
    *variant = EnumVariantId::new(99);
    let mut host = InMemoryRuntimeHost::default();

    let err = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect_err("invalid executable template refs must fail before artifact load");

    assert!(err.to_string().contains("variant id 99"));
    assert!(host.events().is_empty());
    assert!(host.stdout().is_empty());
}

fn loaded_template(template: ArtifactValueTemplate) -> LoadedValueTemplate {
    LoadedValueTemplate::from_artifact(&template).expect("test template should load")
}

fn assert_template_tree_resolves(
    templates: &ExecutableTemplateProgram<'_>,
    root: ExecutableValueTemplateRef,
    visited: &mut Vec<u32>,
) {
    let template = templates
        .get(root)
        .expect("executable template ref should resolve");
    visited.push(root.as_u32());
    match template {
        ExecutableValueTemplate::EnumPayload { value, .. }
        | ExecutableValueTemplate::RecordField { record: value, .. }
        | ExecutableValueTemplate::ListElement { list: value, .. }
        | ExecutableValueTemplate::ListPrefixElement { list: value, .. }
        | ExecutableValueTemplate::ListRest { list: value, .. }
        | ExecutableValueTemplate::MapValue { map: value, .. }
        | ExecutableValueTemplate::MapRest { map: value, .. }
        | ExecutableValueTemplate::EnumVariant { payload: value, .. }
        | ExecutableValueTemplate::BooleanNot { operand: value, .. } => {
            assert_template_tree_resolves(templates, *value, visited);
        }
        ExecutableValueTemplate::Record { fields, .. } => {
            for field in fields {
                assert_template_tree_resolves(templates, field.value, visited);
            }
        }
        ExecutableValueTemplate::List { items, .. } => {
            for item in items {
                assert_template_tree_resolves(templates, *item, visited);
            }
        }
        ExecutableValueTemplate::Map { entries, .. } => {
            for entry in entries {
                assert_template_tree_resolves(templates, entry.key, visited);
                assert_template_tree_resolves(templates, entry.value, visited);
            }
        }
        ExecutableValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            assert_template_tree_resolves(templates, *condition, visited);
            assert_template_tree_resolves(templates, *then_value, visited);
            assert_template_tree_resolves(templates, *else_value, visited);
        }
        ExecutableValueTemplate::Equality { left, right, .. }
        | ExecutableValueTemplate::ScalarArithmetic { left, right, .. }
        | ExecutableValueTemplate::ScalarOrdering { left, right, .. }
        | ExecutableValueTemplate::BooleanBinary { left, right, .. } => {
            assert_template_tree_resolves(templates, *left, visited);
            assert_template_tree_resolves(templates, *right, visited);
        }
        ExecutableValueTemplate::Literal { .. }
        | ExecutableValueTemplate::ReceivedPayload { .. }
        | ExecutableValueTemplate::CurrentStatePayload { .. }
        | ExecutableValueTemplate::ProcessRef { .. }
        | ExecutableValueTemplate::LoopElement { .. }
        | ExecutableValueTemplate::EffectOutcome { .. } => {}
    }
}
