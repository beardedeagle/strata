use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactEffect,
    ArtifactMessageVariant, ArtifactPayload, ArtifactProcess, ArtifactProcessRef,
    ArtifactSendTarget, ArtifactStateValue, ArtifactTransition, ArtifactValueTemplate,
    ArtifactValueTemplateField, MantleArtifact, MessageId, NextState, OutputId, ProcessId,
    ProcessRefId, STRATA_SOURCE_LANGUAGE, StateId, StepResult, write_artifact,
};

use super::program::LoadedProgram;
use super::*;

#[test]
fn runtime_rejects_invalid_artifact_identity() {
    let mut artifact = valid_artifact();
    artifact.format = "other".to_string();

    let err = run_artifact(Path::new("target/test/bad.mta"), &artifact)
        .expect_err("invalid artifact must fail closed");
    assert!(err.to_string().contains("unsupported artifact format"));
}

#[test]
fn runtime_rejects_action_without_declared_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("artifact admission should reject undeclared effect");

    assert!(
        err.to_string()
            .contains("process Main transition 0 uses effect send but does not declare it")
    );
    assert!(
        host.events().is_empty(),
        "rejected artifacts must not reach runtime execution"
    );
}

#[test]
fn runtime_rejects_blocked_trace_sink_before_returning_run_report() {
    let dir = unique_test_dir("blocked-trace-sink");
    fs::create_dir_all(&dir).expect("test dir should be created");
    let blocked_parent = dir.join("blocked");
    fs::write(&blocked_parent, "not a directory").expect("blocking file should be written");

    let artifact_path = blocked_parent.join("hello.mta");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact(&artifact_path, &artifact)
        .expect_err("blocked trace sink should fail before a run report is returned");

    assert!(!err.to_string().is_empty());
    assert!(!trace_path.exists(), "trace path must not be created");

    let _ = fs::remove_file(blocked_parent);
    let _ = fs::remove_dir(dir);
}

#[test]
fn run_artifact_path_writes_trace_for_current_directory_artifact() {
    let artifact_path = unique_current_dir_artifact_path("runtime-current-dir");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    write_artifact(&artifact_path, &artifact).expect("artifact write should succeed");

    let report =
        run_artifact_path(&artifact_path).expect("current-directory artifact run should work");

    assert_eq!(report.trace_path, trace_path);
    assert!(trace_path.exists(), "runtime trace should be written");
    let trace = fs::read_to_string(&trace_path).expect("runtime trace should be readable");
    assert!(trace.contains(r#""event":"artifact_loaded""#));
    assert!(trace.contains(&format!(r#""schema_version":"{ARTIFACT_SCHEMA_VERSION}""#)));
    assert!(trace.contains(r#""event":"process_stopped""#));

    fs::remove_file(artifact_path).expect("test artifact should be removed");
    fs::remove_file(trace_path).expect("test trace should be removed");
}

#[test]
fn runtime_rejects_dispatch_budget_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-budget");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = looping_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_dispatches: 3,
            ..RunLimits::default()
        },
    )
    .expect_err("looping artifact should hit the dispatch budget");

    assert!(
        err.to_string()
            .contains("runtime dispatch budget exceeded after 3 process step(s)")
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn runtime_limits_reject_zero_runtime_processes() {
    let err = RunLimits {
        max_runtime_processes: 0,
        ..RunLimits::default()
    }
    .validate()
    .expect_err("zero runtime process limit should fail closed");

    assert!(
        err.to_string()
            .contains("max_runtime_processes must be greater than zero")
    );
}

#[test]
fn runtime_rejects_runtime_process_limit_before_spawning_child() {
    let artifact = valid_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(
        &artifact,
        &mut host,
        RunLimits {
            max_runtime_processes: 1,
            ..RunLimits::default()
        },
    )
    .expect_err("runtime process limit should fail before spawning Worker");

    assert!(
        err.to_string()
            .contains("runtime process instance limit exceeded at 1 process instance(s)")
    );
    assert!(!host.events().iter().any(|event| {
        matches!(
            event,
            RuntimeEvent::ProcessSpawned { process, .. } if process == "Worker"
        )
    }));
}

#[test]
fn runtime_rejects_trace_limit_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-trace-limit");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_trace_bytes: 8,
            ..RunLimits::default()
        },
    )
    .expect_err("small trace limit should fail closed");

    assert!(
        err.to_string()
            .contains("runtime trace exceeded maximum size of 8 bytes")
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn runtime_rejects_emitted_output_limit_exhaustion() {
    let artifact_path = unique_current_dir_artifact_path("runtime-output-limit");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    let err = run_artifact_with_limits(
        &artifact_path,
        &artifact,
        RunLimits {
            max_emitted_output_bytes: "worker handled Ping".len(),
            ..RunLimits::default()
        },
    )
    .expect_err("small emitted output limit should fail closed");

    assert!(
        err.to_string()
            .contains("emitted output exceeded maximum size")
    );

    let _ = fs::remove_file(trace_path);
}

#[test]
fn actor_artifact_spawns_sends_updates_state_and_stops() {
    let artifact_path = unique_current_dir_artifact_path("runtime-actor");
    let trace_path = artifact_path.with_extension("observability.jsonl");
    let artifact = valid_artifact();

    write_artifact(&artifact_path, &artifact).expect("artifact write should succeed");

    let report = run_artifact_path(&artifact_path).expect("actor artifact should run");

    assert_eq!(report.spawned_processes.len(), 2);
    assert_eq!(report.delivered_messages.len(), 2);
    assert_eq!(report.emitted_outputs, ["worker handled Ping"]);
    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "Handled"
                && process.status == ProcessStatus::Stopped)
    );

    let trace = fs::read_to_string(&trace_path).expect("runtime trace should be readable");
    assert!(trace.contains(r#""event":"process_spawned""#));
    assert!(trace.contains(r#""process":"Worker""#));
    assert!(trace.contains(r#""event":"message_accepted""#));
    assert!(trace.contains(r#""message":"Ping""#));
    assert!(trace.contains(r#""event":"message_dequeued""#));
    assert!(trace.contains(r#""event":"state_updated""#));
    assert!(trace.contains(r#""from_state_id":0,"from":"Idle","to_state_id":1,"to":"Handled""#));
    assert!(trace.contains(r#""event":"process_stopped""#));

    fs::remove_file(artifact_path).expect("test artifact should be removed");
    fs::remove_file(trace_path).expect("test trace should be removed");
}

#[test]
fn in_memory_host_fails_closed_for_panic_without_replay() {
    let artifact = panic_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("panic artifact should fail closed");

    assert!(err.to_string().contains(
        "process Worker panicked after consuming message Ping; message will not be replayed"
    ));
    assert_eq!(
        host.events()
            .iter()
            .filter(|event| matches!(
                event,
                RuntimeEvent::MessageAccepted {
                    process,
                    message,
                    ..
                } if process == "Worker" && message == "Ping"
            ))
            .count(),
        2
    );
    assert_eq!(
        host.events()
            .iter()
            .filter(|event| matches!(
                event,
                RuntimeEvent::MessageDequeued {
                    process,
                    message,
                    ..
                } if process == "Worker" && message == "Ping"
            ))
            .count(),
        1
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process,
            result: RuntimeStepResult::Panic,
            state,
            ..
        } if process == "Worker" && state == "Handled"
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessFailed {
            process,
            state,
            reason: RuntimeFailureReason::Panic,
            ..
        } if process == "Worker" && state == "Handled"
    )));
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::ProcessStopped {
                process,
                ..
            } if process == "Worker"
        )),
        "panic must not be reported as a normal stop"
    );
}

#[test]
fn in_memory_host_runs_actor_without_filesystem_trace_sink() {
    let artifact = valid_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("actor artifact should run through in-memory host");

    assert_eq!(report.spawned_processes.len(), 2);
    assert_eq!(report.delivered_messages.len(), 2);
    assert_eq!(report.emitted_outputs, ["worker handled Ping"]);
    assert_eq!(host.stdout(), ["worker handled Ping"]);
    assert!(
        host.events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::ArtifactLoaded { .. }))
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessSpawned {
            process,
            spawned_by_pid: Some(parent_pid),
            ..
        } if process == "Worker" && *parent_pid == RuntimeProcessId::FIRST
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageAccepted {
            process,
            message,
            sender_pid: Some(sender_pid),
            ..
        } if process == "Worker" && message == "Ping" && *sender_pid == RuntimeProcessId::FIRST
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::StateUpdated {
            process,
            from,
            to,
            ..
        } if process == "Worker" && from == "Idle" && to == "Handled"
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStopped {
            process,
            reason: RuntimeStopReason::Normal,
            ..
        } if process == "Worker"
    )));
}

#[test]
fn in_memory_host_delivers_payload_envelopes_and_template_state() {
    let artifact = payload_artifact();
    let expected_payload = ArtifactPayload {
        ty: "Job".to_string(),
        value: "Job{phase:Ready}".to_string(),
        process_ref: None,
    };
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("payload artifact should run through in-memory host");

    assert_eq!(report.emitted_outputs, ["worker assigned job"]);
    assert!(
        report
            .delivered_messages
            .iter()
            .any(|delivery| delivery.process == "Worker"
                && delivery.message == "Assign(Job{phase:Ready})")
    );
    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "WorkerState{job:Job{phase:Ready}}"
                && process.status == ProcessStatus::Stopped)
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageAccepted {
            process,
            message,
            payload: Some(payload),
            ..
        } if process == "Worker" && message == "Assign" && payload == &expected_payload
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageDequeued {
            process,
            message,
            payload: Some(payload),
            ..
        } if process == "Worker" && message == "Assign" && payload == &expected_payload
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process,
            message,
            payload: Some(payload),
            state,
            ..
        } if process == "Worker"
            && message == "Assign"
            && payload == &expected_payload
            && state == "WorkerState{job:Job{phase:Ready}}"
    )));
}

#[test]
fn runtime_preflights_template_state_before_process_outputs() {
    let mut artifact = payload_artifact();
    artifact.processes[1].state_values =
        state_values("WorkerState", &["WorkerState{job:Job{phase:Done}}"]);
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("unadmitted dynamic template state should fail");

    assert!(err.to_string().contains(
        "process Worker next_state template produced value WorkerState{job:Job{phase:Ready}} not admitted by state table"
    ));
    assert!(
        host.stdout().is_empty(),
        "worker output must not be emitted after invalid template state"
    );
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::ProgramOutput { process, .. } if process == "Worker"
        )),
        "worker program output event must not be recorded after invalid template state"
    );
}

#[test]
fn runtime_rejects_template_state_when_label_matches_but_identity_does_not() {
    let mut artifact = payload_artifact();
    artifact.processes[1].state_values[1] = ArtifactStateValue::with_label(
        "WorkerState",
        "WorkerState{job:Job{phase:Spoofed}}",
        "WorkerState{job:Job{phase:Ready}}",
    );
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("typed state identity mismatch should fail");

    assert!(err.to_string().contains(
        "process Worker next_state template produced value WorkerState{job:Job{phase:Ready}} not admitted by state table"
    ));
    assert!(
        host.stdout().is_empty(),
        "worker output must not be emitted after invalid typed state identity"
    );
}

#[test]
fn in_memory_host_selects_transitions_by_message_id() {
    let artifact = sequence_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("sequence artifact should run through in-memory host");

    assert_eq!(
        report.emitted_outputs,
        ["worker handled First", "worker handled Second"]
    );
    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "Done"
                && process.status == ProcessStatus::Stopped)
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process_id,
            process,
            message_id,
            message,
            result: RuntimeStepResult::Continue,
            state_id,
            state,
            ..
        } if *process_id == ProcessId::new(1)
            && process == "Worker"
            && *message_id == MessageId::new(0)
            && message == "First"
            && *state_id == StateId::new(1)
            && state == "SawFirst"
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process_id,
            process,
            message_id,
            message,
            result: RuntimeStepResult::Stop,
            state_id,
            state,
            ..
        } if *process_id == ProcessId::new(1)
            && process == "Worker"
            && *message_id == MessageId::new(1)
            && message == "Second"
            && *state_id == StateId::new(2)
            && state == "Done"
    )));
}

#[test]
fn loaded_program_indexes_transitions_by_message_id() {
    let mut artifact = sequence_artifact();
    artifact.processes[1].transitions.swap(0, 1);

    let program = LoadedProgram::from_artifact(&artifact)
        .expect("artifact transitions should load by message id");
    let worker = program
        .process(ProcessId::new(1))
        .expect("worker process should be loaded");

    assert_eq!(worker.transitions[0].step_result, StepResult::Continue);
    assert_eq!(worker.transitions[1].step_result, StepResult::Stop);
    assert_eq!(
        worker
            .transition_for_message(MessageId::new(0))
            .expect("First transition should be loaded")
            .step_result,
        StepResult::Continue
    );
    assert_eq!(
        worker
            .transition_for_message(MessageId::new(1))
            .expect("Second transition should be loaded")
            .step_result,
        StepResult::Stop
    );
}

#[test]
fn in_memory_host_preserves_current_next_state() {
    let mut artifact = valid_artifact();
    artifact.entry_process = ProcessId::new(0);
    artifact.entry_message = MessageId::new(0);
    artifact.processes = vec![ArtifactProcess {
        debug_name: "Worker".to_string(),
        state_type: "WorkerState".to_string(),
        state_values: state_values("WorkerState", &["Idle", "Handled"]),
        message_type: "WorkerMsg".to_string(),
        message_variants: vec![ArtifactMessageVariant::unit("Ping")],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(1),
        transitions: vec![ArtifactTransition {
            message: MessageId::new(0),
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: vec![ArtifactEffect::Emit],
            actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("current next state should preserve runtime state");

    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker" && process.state == "Handled")
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::StateUpdated { .. })),
        "preserving current state must not emit a state update"
    );
}

#[test]
fn in_memory_host_rejects_trace_limit_exhaustion() {
    let artifact = valid_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(
        &artifact,
        &mut host,
        RunLimits {
            max_trace_bytes: 8,
            ..RunLimits::default()
        },
    )
    .expect_err("small trace limit should fail for in-memory hosts");

    assert!(
        err.to_string()
            .contains("runtime trace exceeded maximum size of 8 bytes")
    );
    assert!(
        host.events().is_empty(),
        "host should not receive an event that exceeds the trace budget"
    );
}

#[test]
fn runtime_process_id_rejects_zero() {
    let err = RuntimeProcessId::from_u64(0).expect_err("zero runtime pid should be invalid");

    assert!(
        err.to_string()
            .contains("runtime process id must be greater than zero")
    );
}

fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: "actor_ping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        outputs: vec!["worker handled Ping".to_string()],
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: "MainState".to_string(),
                state_values: state_values("MainState", &["MainState"]),
                message_type: "MainMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: "WorkerState".to_string(),
                state_values: state_values("WorkerState", &["Idle", "Handled"]),
                message_type: "WorkerMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                process_refs: Vec::new(),
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Value(StateId::new(1)),
                    effects: vec![ArtifactEffect::Emit],
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn panic_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.module = "actor_panic_no_replay".to_string();
    artifact.outputs = Vec::new();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: None,
        });
    artifact.processes[1].mailbox_bound = 2;
    artifact.processes[1].transitions[0] = ArtifactTransition {
        message: MessageId::new(0),
        step_result: StepResult::Panic,
        next_state: NextState::Value(StateId::new(1)),
        effects: Vec::new(),
        actions: Vec::new(),
    };
    artifact
}

fn payload_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.module = "actor_payloads".to_string();
    artifact.outputs = vec!["worker assigned job".to_string()];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: "Job".to_string(),
            value: "Job{phase:Ready}".to_string(),
        }),
    };
    artifact.processes[1].state_type = "WorkerState".to_string();
    artifact.processes[1].state_values = state_values(
        "WorkerState",
        &[
            "WorkerState{job:Job{phase:Done}}",
            "WorkerState{job:Job{phase:Ready}}",
        ],
    );
    artifact.processes[1].message_type = "WorkerMsg".to_string();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", "Job")];
    artifact.processes[1].transitions[0] = ArtifactTransition {
        message: MessageId::new(0),
        step_result: StepResult::Stop,
        next_state: NextState::Template(ArtifactValueTemplate::Record {
            ty: "WorkerState".to_string(),
            fields: vec![ArtifactValueTemplateField {
                name: "job".to_string(),
                value: ArtifactValueTemplate::ReceivedPayload {
                    ty: "Job".to_string(),
                },
            }],
        }),
        effects: vec![ArtifactEffect::Emit],
        actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
    };
    artifact
}

fn looping_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: "looping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        outputs: Vec::new(),
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: "MainState".to_string(),
                state_values: state_values("MainState", &["MainState"]),
                message_type: "MainMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: "WorkerState".to_string(),
                state_values: state_values("WorkerState", &["WorkerState"]),
                message_type: "WorkerMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "helper".to_string(),
                    target: ProcessId::new(2),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Continue,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(2),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Helper".to_string(),
                state_type: "HelperState".to_string(),
                state_values: state_values("HelperState", &["HelperState"]),
                message_type: "HelperMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Continue,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn sequence_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: STRATA_SOURCE_LANGUAGE.to_string(),
        module: "actor_sequence".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        outputs: vec![
            "worker handled First".to_string(),
            "worker handled Second".to_string(),
        ],
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: "MainState".to_string(),
                state_values: state_values("MainState", &["MainState"]),
                message_type: "MainMsg".to_string(),
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(1),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: "WorkerState".to_string(),
                state_values: state_values("WorkerState", &["Waiting", "SawFirst", "Done"]),
                message_type: "WorkerMsg".to_string(),
                message_variants: vec![
                    ArtifactMessageVariant::unit("First"),
                    ArtifactMessageVariant::unit("Second"),
                ],
                process_refs: Vec::new(),
                mailbox_bound: 2,
                init_state: StateId::new(0),
                transitions: vec![
                    ArtifactTransition {
                        message: MessageId::new(0),
                        step_result: StepResult::Continue,
                        next_state: NextState::Value(StateId::new(1)),
                        effects: vec![ArtifactEffect::Emit],
                        actions: vec![ArtifactAction::Emit {
                            output: OutputId::new(0),
                        }],
                    },
                    ArtifactTransition {
                        message: MessageId::new(1),
                        step_result: StepResult::Stop,
                        next_state: NextState::Value(StateId::new(2)),
                        effects: vec![ArtifactEffect::Emit],
                        actions: vec![ArtifactAction::Emit {
                            output: OutputId::new(1),
                        }],
                    },
                ],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn state_values(ty: &str, values: &[&str]) -> Vec<ArtifactStateValue> {
    values
        .iter()
        .map(|value| ArtifactStateValue::new(ty, *value))
        .collect()
}

fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique_artifact_name(name))
}

fn unique_current_dir_artifact_path(name: &str) -> PathBuf {
    PathBuf::from(unique_artifact_name(name))
}

fn unique_artifact_name(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    format!("strata-{name}-{}-{nanos}.mta", std::process::id())
}
