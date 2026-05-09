use super::support::*;

#[test]
fn runtime_process_lookup_indexes_by_pid() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(
        &program,
        &mut host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    );
    let main_pid = run
        .spawn_process(ProcessId::new(0), None)
        .expect("entry process should spawn");
    let worker_pid = run
        .spawn_process(ProcessId::new(1), Some(main_pid))
        .expect("worker process should spawn");

    assert_eq!(run.process_index_for_pid(main_pid).expect("main pid"), 0);
    assert_eq!(
        run.process_index_for_pid(worker_pid).expect("worker pid"),
        1
    );

    run.send_message(
        worker_pid,
        RuntimeMessageEnvelope::new(MessageId::new(0), None),
        Some(main_pid),
    )
    .expect("send should address worker by pid index");
    assert_eq!(run.processes[1].mailbox.len(), 1);
}

#[test]
fn runtime_process_lookup_rejects_unspawned_pid() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(
        &program,
        &mut host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    );
    run.spawn_process(ProcessId::new(0), None)
        .expect("entry process should spawn");
    let missing_pid = RuntimeProcessId::from_u64(2).expect("valid pid should construct");

    let err = run
        .process_index_for_pid(missing_pid)
        .expect_err("unspawned pid should be rejected");

    assert!(err.to_string().contains("runtime process 2 is not spawned"));
}
