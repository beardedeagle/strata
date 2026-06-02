use super::*;

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
    assert_direct_send_outcome_failure(
        ProcessStatus::Stopped,
        "Err(Stopped(Ping))",
        RuntimeEffectOutcomeResult::Stopped,
    );
}

#[test]
fn runtime_send_outcome_returns_crashed_before_acceptance_and_preserves_message() {
    assert_direct_send_outcome_failure(
        ProcessStatus::Failed,
        "Err(Crashed(Ping))",
        RuntimeEffectOutcomeResult::Crashed,
    );
}
