use super::support::*;

#[test]
fn in_memory_host_preserves_current_next_state() {
    let mut artifact = valid_artifact();
    artifact.entry_process = ProcessId::new(0);
    artifact.entry_message = MessageId::new(0);
    artifact.processes = vec![ArtifactProcess {
        debug_name: "Worker".to_string(),
        state_type: WORKER_STATE,
        state_values: state_values(WORKER_STATE, &["Idle", "Handled"]),
        message_type: WORKER_MSG,
        message_variants: vec![ArtifactMessageVariant::unit("Ping")],
        authorities: Vec::new(),
        spawn_sites: Vec::new(),
        supervisor_plans: Vec::new(),
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(1),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: None,
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
