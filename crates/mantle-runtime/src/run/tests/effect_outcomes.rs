use super::support::*;
use crate::{ProcessStatus, RuntimeProcessId};
use mantle_artifact::{ArtifactAction, ArtifactSendTarget, EffectOutcomeId};

mod boundary;

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const ENTRY_PROCESS: ProcessId = ProcessId::new(2);
const PING_MESSAGE: MessageId = MessageId::new(0);
const UNIT: TypeId = TypeId::new(10);
const SEND_ERROR: TypeId = TypeId::new(11);
const SEND_RESULT: TypeId = TypeId::new(12);
const SPAWN_ERROR: TypeId = TypeId::new(11);
const SPAWN_RESULT: TypeId = TypeId::new(12);

#[test]
fn runtime_send_outcome_commits_ok_after_acceptance() {
    let artifact = send_outcome_artifact();
    let report = run_artifact_with_host(
        &artifact,
        &mut InMemoryRuntimeHost::default(),
        RunLimits::default(),
    )
    .expect("accepted send outcome should run");

    assert_eq!(process_state(&report, "Main"), "Ok(Unit)");
    assert_eq!(report.delivered_messages.len(), 2);
}

#[test]
fn runtime_send_outcome_returns_full_before_acceptance_and_preserves_message() {
    assert_direct_send_outcome_full("Err(Full(Ping))");
}

#[test]
fn runtime_send_outcome_returns_full_and_preserves_process_ref_message_payload() {
    let artifact = send_outcome_process_ref_payload_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let worker_pid = run
        .spawn_process(WORKER_PROCESS, Some(main_pid))
        .expect("worker should spawn");
    let worker_index = run
        .process_index_for_pid(worker_pid)
        .expect("worker pid should resolve");
    run.processes[worker_index]
        .mailbox
        .push_back(RuntimeMessageEnvelope::new(PING_MESSAGE, None));

    let mut process_refs = LocalProcessRefs::new(1);
    process_refs
        .bind(ProcessRefId::new(0), worker_pid)
        .expect("worker process ref should bind");
    let step = main_step(main_pid);
    let action = LoadedAction::SendOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SEND_RESULT,
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: PING_MESSAGE,
        payload: Some(LoadedValueTemplate::ProcessRef {
            ty: PROCESS_REF_WORKER,
            target_process: WORKER_PROCESS,
            process_ref: ProcessRefId::new(0),
        }),
    };
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("full send outcome should preserve process reference message payload");

    assert!(handled);
    assert_eq!(
        effect_outcomes[0].payload.label(),
        "Err(Full(Forward(type8#2)))"
    );
    assert_eq!(
        effect_outcomes[0]
            .payload
            .process_ref()
            .map(|item| (item.target_process, item.pid)),
        Some((WORKER_PROCESS, worker_pid.as_u64()))
    );
    assert_eq!(run.processes[worker_index].mailbox.len(), 1);
    assert!(run.delivered_messages.is_empty());
}

#[test]
fn runtime_send_outcome_returns_stopped_before_acceptance_and_preserves_message() {
    assert_direct_send_outcome_failure(ProcessStatus::Stopped, "Err(Stopped(Ping))");
}

#[test]
fn runtime_send_outcome_returns_crashed_before_acceptance_and_preserves_message() {
    assert_direct_send_outcome_failure(ProcessStatus::Failed, "Err(Crashed(Ping))");
}

#[test]
fn runtime_spawn_outcome_commits_ok_after_acceptance() {
    let artifact = spawn_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let step = main_step(main_pid);
    let action = LoadedAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SPAWN_RESULT,
        target: WORKER_PROCESS,
        spawn_site: SPAWN_SITE,
    };
    let mut process_refs = LocalProcessRefs::empty();
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("accepted spawn outcome should bind typed process reference result");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), "Ok(type8#2)");
    assert_eq!(
        effect_outcomes[0]
            .payload
            .process_ref()
            .map(|item| item.pid),
        Some(2)
    );
    assert_eq!(run.processes.len(), 2);
}

#[test]
fn runtime_spawn_outcome_returns_exhausted_before_acceptance() {
    let artifact = spawn_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let limits = RunLimits {
        max_runtime_processes: 1,
        ..RunLimits::default()
    };
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, limits);
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let step = main_step(main_pid);
    let action = LoadedAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SPAWN_RESULT,
        target: WORKER_PROCESS,
        spawn_site: SPAWN_SITE,
    };
    let mut process_refs = LocalProcessRefs::empty();
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("exhausted spawn outcome should bind a typed failure");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), "Err(Exhausted(Unit))");
    assert_eq!(run.processes.len(), 1);
}

#[test]
fn runtime_spawn_outcome_branches_by_error_variant_without_process_ref_equality() {
    let mut artifact = spawn_outcome_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.outputs = vec!["spawn accepted".to_string()];
    artifact.processes[0].transitions[0].effects =
        vec![ArtifactEffect::Spawn, ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::SpawnOutcome {
            outcome: EffectOutcomeId::new(0),
            outcome_ty: SPAWN_RESULT,
            target: WORKER_PROCESS,
            spawn_site: SPAWN_SITE,
        },
        ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: SPAWN_RESULT,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: SPAWN_RESULT,
                    value: artifact_value("Err(Exhausted(Unit))"),
                }),
            },
            then_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: Vec::new(),
        },
    ];
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("spawn outcome variant branch should run");

    assert_eq!(report.emitted_outputs, ["spawn accepted"]);
}

#[test]
fn loaded_admission_rejects_spawn_outcome_targeting_entry_process() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let LoadedAction::SpawnOutcome { target, .. } =
        &mut program.processes[0].transitions[0].actions[0]
    else {
        panic!("test program action should be spawn outcome");
    };
    *target = MAIN_PROCESS;

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "spawn outcome targets entry process id 0",
    );
}

#[test]
fn loaded_admission_rejects_spawn_outcome_targeting_self() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut entry_process = program.processes[1].clone();
    entry_process.debug_name = "Entry".to_string();
    entry_process.authorities.clear();
    entry_process.spawn_sites.clear();
    entry_process.process_refs.clear();
    program.processes.push(entry_process);
    program.entry_process = ENTRY_PROCESS;
    program.processes[0].process_refs.clear();
    let LoadedAction::SpawnOutcome { target, .. } =
        &mut program.processes[0].transitions[0].actions[0]
    else {
        panic!("test program action should be spawn outcome");
    };
    *target = MAIN_PROCESS;

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "spawn outcome targets itself, which is not supported",
    );
}

#[test]
fn loaded_admission_rejects_send_outcome_type_without_mailbox_closed_variant() {
    let artifact = send_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[SEND_ERROR.index()] =
        send_error_type_with_labels(WORKER_MSG, &["Full", "Stopped", "Crashed"]);
    program.processes[0].state_values = loaded_state_values(
        SEND_RESULT,
        &[
            "Ok(Unit)",
            "Err(Full(Ping))",
            "Err(Stopped(Ping))",
            "Err(Crashed(Ping))",
        ],
    );
    program.processes[0].transitions[0].next_state = LoadedNextState::Current;

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "send outcome error type type id 11 has 3 variants, expected 4",
    );
}

#[test]
fn loaded_admission_rejects_process_ref_spawn_outcome_as_state_template() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_type = SPAWN_RESULT;
    program.processes[0].state_values =
        loaded_state_values(SPAWN_RESULT, &["Err(Exhausted(Unit))"]);
    program.processes[0].init_state = StateId::new(0);
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::EffectOutcome {
            ty: SPAWN_RESULT,
            outcome: EffectOutcomeId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process reference outcome must remain step-local",
    );
}

#[test]
fn loaded_admission_rejects_process_ref_spawn_outcome_structural_equality() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let bool_type = TypeId::from_index(program.types.len()).expect("test type id should fit");
    program.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::IfElse {
            condition: LoadedValueTemplate::Equality {
                ty: bool_type,
                operand_ty: SPAWN_RESULT,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(LoadedValueTemplate::EffectOutcome {
                    ty: SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(LoadedValueTemplate::EffectOutcome {
                    ty: SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "built-in payload enum requires one operand to be a safe built-in variant pattern",
    );
}

#[test]
fn loaded_admission_rejects_send_outcome_structural_equality() {
    let artifact = send_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let bool_type = push_loaded_type(
        &mut program,
        ArtifactType::enum_value("Bool", vec!["False".to_string(), "True".to_string()]),
    );
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::IfElse {
            condition: LoadedValueTemplate::Equality {
                ty: bool_type,
                operand_ty: SEND_RESULT,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(LoadedValueTemplate::EffectOutcome {
                    ty: SEND_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(LoadedValueTemplate::EffectOutcome {
                    ty: SEND_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "built-in payload enum requires one operand to be a safe built-in variant pattern",
    );
}

#[test]
fn loaded_admission_rejects_nested_process_ref_spawn_outcome_in_variant_pattern() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let boxed_outcome = push_loaded_type(
        &mut program,
        ArtifactType::record(
            "OutcomeBox",
            vec![ArtifactTypeField {
                name: "outcome".to_string(),
                ty: SPAWN_RESULT,
            }],
        ),
    );
    let maybe_boxed_outcome = push_loaded_type(
        &mut program,
        ArtifactType::enum_value_with_payloads(
            "Option",
            vec![
                ArtifactEnumVariant {
                    label: "None".to_string(),
                    payload_type: None,
                },
                ArtifactEnumVariant {
                    label: "Some".to_string(),
                    payload_type: Some(boxed_outcome),
                },
            ],
        ),
    );
    let bool_type = push_loaded_type(
        &mut program,
        ArtifactType::enum_value("Bool", vec!["False".to_string(), "True".to_string()]),
    );
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::IfElse {
            condition: loaded_template(ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: maybe_boxed_outcome,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(ArtifactValueTemplate::EnumVariant {
                    ty: maybe_boxed_outcome,
                    variant: EnumVariantId::new(1),
                    payload: Box::new(ArtifactValueTemplate::Record {
                        ty: boxed_outcome,
                        fields: vec![ArtifactValueTemplateField {
                            field: RecordFieldId::new(0),
                            value: ArtifactValueTemplate::EffectOutcome {
                                ty: SPAWN_RESULT,
                                outcome: EffectOutcomeId::new(0),
                            },
                        }],
                    }),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: maybe_boxed_outcome,
                    value: artifact_value("None"),
                }),
            }),
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "equality payload.operand_type_id must be Bool",
    );
}

#[test]
fn loaded_admission_rejects_spawn_outcome_type_without_process_ref_success() {
    let artifact = spawn_outcome_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[SPAWN_RESULT.index()] = result_type(UNIT, SPAWN_ERROR);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "spawn outcome success type type id 10 must be a process reference type",
    );
}

fn assert_direct_send_outcome_failure(status: ProcessStatus, expected: &str) {
    let artifact = send_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let worker_pid = run
        .spawn_process(WORKER_PROCESS, Some(main_pid))
        .expect("worker should spawn");
    let worker_index = run
        .process_index_for_pid(worker_pid)
        .expect("worker pid should resolve");
    run.processes[worker_index].status = status;

    let mut process_refs = LocalProcessRefs::new(1);
    process_refs
        .bind(ProcessRefId::new(0), worker_pid)
        .expect("worker process ref should bind");
    let step = main_step(main_pid);
    let action = LoadedAction::SendOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SEND_RESULT,
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: PING_MESSAGE,
        payload: None,
    };
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("send outcome failure should bind a typed result");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), expected);
    assert_eq!(run.processes[worker_index].mailbox.len(), 0);
    assert!(run.delivered_messages.is_empty());
}

fn assert_direct_send_outcome_full(expected: &str) {
    let artifact = send_outcome_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let worker_pid = run
        .spawn_process(WORKER_PROCESS, Some(main_pid))
        .expect("worker should spawn");
    let worker_index = run
        .process_index_for_pid(worker_pid)
        .expect("worker pid should resolve");
    run.processes[worker_index]
        .mailbox
        .push_back(RuntimeMessageEnvelope::new(PING_MESSAGE, None));

    let mut process_refs = LocalProcessRefs::new(1);
    process_refs
        .bind(ProcessRefId::new(0), worker_pid)
        .expect("worker process ref should bind");
    let step = main_step(main_pid);
    let action = LoadedAction::SendOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SEND_RESULT,
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: PING_MESSAGE,
        payload: None,
    };
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("full send outcome should bind a typed result");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), expected);
    assert_eq!(run.processes[worker_index].mailbox.len(), 1);
    assert!(run.delivered_messages.is_empty());
}

fn send_outcome_artifact() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    artifact.types[WORKER_MSG.index()] =
        ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]);
    push_outcome_types(&mut artifact);
    artifact.processes[0].state_type = SEND_RESULT;
    artifact.processes[0].state_values = state_values(
        SEND_RESULT,
        &[
            "Ok(Unit)",
            "Err(Full(Ping))",
            "Err(Stopped(Ping))",
            "Err(Crashed(Ping))",
            "Err(MailboxClosed(Ping))",
        ],
    );
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].effects =
        vec![ArtifactEffect::Spawn, ArtifactEffect::Send];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EffectOutcome {
            ty: SEND_RESULT,
            outcome: EffectOutcomeId::new(0),
        });

    let actions = vec![
        ArtifactAction::Spawn {
            target: WORKER_PROCESS,
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_SITE,
        },
        ArtifactAction::SendOutcome {
            outcome: EffectOutcomeId::new(0),
            outcome_ty: SEND_RESULT,
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            port: None,
            message: PING_MESSAGE,
            payload: None,
        },
    ];
    artifact.processes[0].transitions[0].actions = actions;
    artifact
}

fn send_outcome_process_ref_payload_artifact() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    replace_process_message_variants(
        &mut artifact,
        WORKER_PROCESS.index(),
        vec![ArtifactMessageVariant::payload(
            "Forward",
            PROCESS_REF_WORKER,
        )],
    );
    push_outcome_types(&mut artifact);
    artifact.processes[0].state_type = SEND_RESULT;
    artifact.processes[0].state_values = state_values(SEND_RESULT, &["Ok(Unit)"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].effects =
        vec![ArtifactEffect::Spawn, ArtifactEffect::Send];
    artifact.processes[0].transitions[0].next_state = NextState::Current;
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: WORKER_PROCESS,
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_SITE,
        },
        ArtifactAction::SendOutcome {
            outcome: EffectOutcomeId::new(0),
            outcome_ty: SEND_RESULT,
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            port: None,
            message: PING_MESSAGE,
            payload: Some(ArtifactValueTemplate::ProcessRef {
                ty: PROCESS_REF_WORKER,
                target_process: WORKER_PROCESS,
                process_ref: ProcessRefId::new(0),
            }),
        },
    ];
    artifact
}

fn spawn_outcome_artifact() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    grant_main_spawn_authority(&mut artifact);
    push_spawn_outcome_types(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].next_state = NextState::Current;
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SPAWN_RESULT,
        target: WORKER_PROCESS,
        spawn_site: SPAWN_SITE,
    }];
    artifact
}

fn push_outcome_types(artifact: &mut MantleArtifact) {
    assert_eq!(push_type(artifact, ArtifactType::value("Unit")), UNIT);
    assert_eq!(push_type(artifact, send_error_type(WORKER_MSG)), SEND_ERROR);
    assert_eq!(
        push_type(artifact, result_type(UNIT, SEND_ERROR)),
        SEND_RESULT
    );
}

fn push_spawn_outcome_types(artifact: &mut MantleArtifact) {
    assert_eq!(push_type(artifact, ArtifactType::value("Unit")), UNIT);
    assert_eq!(push_type(artifact, spawn_error_type()), SPAWN_ERROR);
    assert_eq!(
        push_type(artifact, result_type(PROCESS_REF_WORKER, SPAWN_ERROR)),
        SPAWN_RESULT
    );
}

fn push_type(artifact: &mut MantleArtifact, ty: ArtifactType) -> TypeId {
    let id = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ty);
    id
}

fn append_bool_type(artifact: &mut MantleArtifact) -> TypeId {
    push_type(
        artifact,
        ArtifactType::enum_value("Bool", vec!["False".to_string(), "True".to_string()]),
    )
}

fn push_loaded_type(program: &mut LoadedProgram, ty: ArtifactType) -> TypeId {
    let id = TypeId::from_index(program.types.len()).expect("test type id should fit");
    program.types.push(ty);
    id
}

fn result_type(ok: TypeId, err: TypeId) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "Result",
        vec![
            ArtifactEnumVariant {
                label: "Ok".to_string(),
                payload_type: Some(ok),
            },
            ArtifactEnumVariant {
                label: "Err".to_string(),
                payload_type: Some(err),
            },
        ],
    )
}

fn send_error_type(message_ty: TypeId) -> ArtifactType {
    send_error_type_with_labels(message_ty, &["Full", "Stopped", "Crashed", "MailboxClosed"])
}

fn send_error_type_with_labels(message_ty: TypeId, labels: &[&str]) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SendError",
        labels
            .iter()
            .map(|label| ArtifactEnumVariant {
                label: (*label).to_string(),
                payload_type: Some(message_ty),
            })
            .collect(),
    )
}

fn spawn_error_type() -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SpawnError",
        ["Denied", "Exhausted", "BackendUnavailable"]
            .into_iter()
            .map(|label| ArtifactEnumVariant {
                label: label.to_string(),
                payload_type: Some(UNIT),
            })
            .collect(),
    )
}

fn main_step(main_pid: RuntimeProcessId) -> ActiveStep {
    ActiveStep {
        pid: main_pid,
        process_id: MAIN_PROCESS,
        process_name: "Main".to_string(),
        current_state: StateId::new(0),
        message: MessageId::new(0),
        message_label: "Start".to_string(),
        payload: None,
    }
}

fn process_state(report: &RuntimeReport, process: &str) -> String {
    report
        .processes
        .iter()
        .find(|item| item.process == process)
        .unwrap_or_else(|| panic!("process {process} should be reported"))
        .state
        .clone()
}
