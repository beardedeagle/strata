use super::support::*;

#[test]
fn artifact_round_trips_and_validates_magic() {
    let artifact = valid_artifact();
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains(&format!("schema_version={ARTIFACT_SCHEMA_VERSION}")));
    assert!(encoded.contains("entry_process=0"));
    assert!(encoded.contains("type.2.label=WorkerState"));
    assert!(encoded.contains("process.1.state_value.1.type_id=2"));
    assert!(encoded.contains("process.1.state_value.1.value=Handled"));
    assert!(encoded.contains("process.1.state_value.1.label=Handled"));
    assert!(encoded.contains("process.0.transition.0.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_value=1"));
    assert!(encoded.contains("process.0.process_ref.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.effect_count=2"));
    assert!(encoded.contains("process.0.transition.0.effect.0=spawn"));
    assert!(encoded.contains("process.0.transition.0.effect.1=send"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.action.0.process_ref=0"));
    assert!(encoded.contains("process.0.transition.0.action.1.target_process_ref=0"));

    let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
    assert!(err.to_string().contains("invalid Mantle artifact magic"));
}

#[test]
fn artifact_round_trips_enum_variant_metadata_above_template_field_limit() {
    let mut artifact = valid_artifact();
    let variants = (0..=MAX_VALUE_TEMPLATE_FIELDS)
        .map(|index| format!("V{index}"))
        .collect::<Vec<_>>();
    artifact.types[MAIN_MSG.index()] = ArtifactType::enum_value("MainMsg", variants);

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded)
        .expect("enum variant metadata above template-field limit should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains(&format!(
        "type.{}.enum_variant_count={}",
        MAIN_MSG.index(),
        MAX_VALUE_TEMPLATE_FIELDS + 1
    )));
}

#[test]
fn artifact_round_trips_panic_step_result() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].step_result = StepResult::Panic;

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("panic artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.step_result=Panic"));
}

#[test]
fn artifact_round_trips_map_rest_template_with_excluded_keys() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = vec![state_value(JOB, "Map[Done=>Ready]")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::MapRest {
            ty: JOB,
            map: Box::new(ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("Map[Done=>Ready,Ready=>Done]"),
            }),
            excluded_keys: vec![artifact_value("Ready")],
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("map rest artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.next_state_template.kind=map_rest"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.excluded_key.0=Ready"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.expected_key.0"));
}

#[test]
fn artifact_round_trips_list_rest_template_with_prefix_len() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = vec![state_value(JOB, "List[Done]")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ListRest {
            ty: JOB,
            list: Box::new(ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("List[Ready,Done]"),
            }),
            prefix_len: 1,
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("list rest artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.next_state_template.kind=list_rest"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.prefix_len=1"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.expected_key.0"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.excluded_key.0"));
}

#[test]
fn artifact_round_trips_list_prefix_template_with_prefix_len() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = vec![state_value(JOB, "Ready")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ListPrefixElement {
            ty: JOB,
            list: Box::new(ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("List[Ready,Done]"),
            }),
            index: 0,
            prefix_len: 1,
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("list prefix artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(
        encoded.contains("process.0.transition.0.next_state_template.kind=list_prefix_element")
    );
    assert!(encoded.contains("process.0.transition.0.next_state_template.index=0"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.prefix_len=1"));
}

#[test]
fn artifact_round_trips_if_else_control_flow() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let condition = ArtifactValueTemplate::Literal {
        ty: bool_type,
        value: artifact_value("True"),
    };
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: condition.clone(),
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition,
        then_actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
        else_actions: Vec::new(),
    }];

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("if_else artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.next_state=if_else"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.kind=literal"));
    assert!(encoded.contains("process.1.transition.0.next_state_then.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_else.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.action.0.kind=if_else"));
    assert!(encoded.contains("process.1.transition.0.action.0.then_action_count=1"));
    assert!(encoded.contains("process.1.transition.0.action.0.else_action_count=0"));
}

#[test]
fn artifact_round_trips_for_each_control_flow() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: JOB,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("List[Ready,Done]"),
            },
            max_items: 2,
            body: vec![ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::LoopElement {
                    ty: JOB,
                    element: LoopElementId::new(0),
                }),
            }],
        },
    ];

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("for_each artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.action.1.kind=for_each"));
    assert!(encoded.contains("process.0.transition.0.action.1.loop_element=0"));
    assert!(encoded.contains("process.0.transition.0.action.1.collection.kind=literal"));
    assert!(encoded.contains("process.0.transition.0.action.1.body_action.0.kind=send"));
    assert!(encoded.contains(
        "process.0.transition.0.action.1.body_action.0.payload_template.kind=loop_element"
    ));
}

#[test]
fn decode_rejects_missing_for_each_body_action() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::LoopElement {
                ty: JOB,
                element: LoopElementId::new(0),
            }),
        }],
    }];
    let encoded = artifact.encode().replace(
        "process.0.transition.0.action.0.body_action.0.kind=send\n",
        "",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("missing loop body action should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field process.0.transition.0.action.0.body_action.0.kind")
    );
}

#[test]
fn admission_rejects_inactive_for_each_loop_element_payload() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: JOB,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("List[Ready]"),
            },
            max_items: 1,
            body: vec![ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::LoopElement {
                    ty: JOB,
                    element: LoopElementId::new(1),
                }),
            }],
        },
    ];

    let err = artifact
        .validate()
        .expect_err("inactive loop element should fail admission");

    assert!(
        err.to_string()
            .contains("references inactive loop element id 1"),
        "{err}"
    );
}

#[test]
fn admission_rejects_static_for_each_non_list_collection() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Ready"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("static non-list for_each collection should fail admission");

    assert!(
        err.to_string()
            .contains("process Main transition 0 for collection must evaluate to a list value"),
        "{err}"
    );
}

#[test]
fn admission_rejects_for_each_process_ref_element_type() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: PROCESS_REF_WORKER,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("process-ref for_each element should fail admission");

    assert!(
        err.to_string()
            .contains("artifact field for loop element type type id 9 must be a value type"),
        "{err}"
    );
}

#[test]
fn admission_rejects_for_each_loop_element_id_above_codec_bound() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(MAX_VALUE_TEMPLATE_FIELDS as u32),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("loop element ids above the codec bound should fail admission");

    assert!(
        err.to_string()
            .contains("for loop element id must be no greater than"),
        "{err}"
    );
}

#[test]
fn decode_rejects_missing_if_else_next_state_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };
    let encoded = artifact.encode().replace(
        "process.1.transition.0.next_state_else.next_state=current\n",
        "",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("missing else branch should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field process.1.transition.0.next_state_else.next_state")
    );
}

#[test]
fn decode_rejects_if_else_action_nesting_above_limit() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].actions = vec![nested_if_else_action(
        MAX_VALUE_TEMPLATE_DEPTH + 1,
        bool_type,
    )];
    let encoded = artifact.encode();

    let err = MantleArtifact::decode(&encoded).expect_err("overly nested action should fail");

    assert!(err.to_string().contains(&format!(
        "exceeds maximum action nesting depth of {MAX_VALUE_TEMPLATE_DEPTH}"
    )));
}

#[test]
fn decode_rejects_unknown_step_result() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.step_result=Stop",
        "process.1.transition.0.step_result=Crash",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown step result should fail");

    assert!(
        err.to_string()
            .contains("invalid step_result value \"Crash\"")
    );
}

#[test]
fn decode_rejects_unsupported_schema_before_body_fields() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version=0\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unsupported schema should fail first");

    assert!(err.to_string().contains(&format!(
        "unsupported artifact schema version 0; expected {ARTIFACT_SCHEMA_VERSION}"
    )));
}

#[test]
fn decode_reports_duplicate_fields() {
    let encoded = valid_artifact().encode().replace(
        "process.0.debug_name=Main",
        "process.0.debug_name=Main\nprocess.0.debug_name=Other",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

    assert!(
        err.to_string()
            .contains("duplicate artifact field \"process.0.debug_name\"")
    );
}

#[test]
fn decode_reports_unknown_fields() {
    let mut encoded = valid_artifact().encode();
    encoded.push_str("process.0.transition.0.action.0.extra=value\n");

    let err = MantleArtifact::decode(&encoded).expect_err("unknown field should fail");

    assert!(
        err.to_string()
            .contains("unknown artifact field \"process.0.transition.0.action.0.extra\"")
    );
}

#[test]
fn decode_reports_artifact_value_field_context() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value.0.value=MainState",
        "process.0.state_value.0.value=Main\u{7}State",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("invalid state value should fail");
    let message = err.to_string();

    assert!(
        message.contains(
            "process.0.state_value.0.value must be non-empty and contain no control characters"
        ),
        "unexpected error: {err}"
    );
    assert!(
        !message.contains("payload value"),
        "state value decode should not report payload context: {err}"
    );
}

#[test]
fn decode_rejects_unbounded_process_count_before_allocation() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version={ARTIFACT_SCHEMA_VERSION}\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("process count should be bounded");

    assert!(
        err.to_string()
            .contains("process_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_nested_counts_before_allocation() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value_count=1",
        &format!(
            "process.0.state_value_count={}",
            MAX_STATE_VALUES_PER_PROCESS + 1
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("state value count should be bounded");

    assert!(
        err.to_string()
            .contains("process.0.state_value_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_transition_current_state_before_validation() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.message=0",
        &format!(
            "process.1.transition.0.current_state={}\nprocess.1.transition.0.message=0",
            MAX_STATE_VALUES_PER_PROCESS
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("current_state id should be bounded");

    assert!(
        err.to_string()
            .contains("process.1.transition.0.current_state must be no greater than")
    );
}

#[test]
fn decode_rejects_unknown_transition_effect() {
    let encoded = valid_artifact().encode().replace(
        "process.0.transition.0.effect.1=send",
        "process.0.transition.0.effect.1=write",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown effect should fail");

    assert!(
        err.to_string()
            .contains("process.0.transition.0.effect.1: invalid effect value \"write\"")
    );
}
