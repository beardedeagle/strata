use super::support::*;

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
