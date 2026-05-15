use super::support::*;
use crate::{ProcessStatus, RuntimeEvent, RuntimeProcessId};

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const PING_MESSAGE: MessageId = MessageId::new(0);
const UNSPAWNED_WORKER_PID: u64 = 99;

#[test]
fn runtime_rejects_send_to_stopped_process_before_acceptance() {
    let artifact = artifact_with_worker_process_ref_payload();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let main_pid = run
            .spawn_process(MAIN_PROCESS, None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, Some(main_pid))
            .expect("worker process should spawn");
        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                PING_MESSAGE,
                Some(worker_process_ref_payload(worker_pid.as_u64())),
            ),
            Some(main_pid),
        )
        .expect("first send should be accepted before worker stops");
        run.drain_mailboxes(RunLimits::default().max_dispatches)
            .expect("worker should stop after consuming the first message");

        let worker_index = process_index_for_pid(&run, worker_pid);
        assert_eq!(run.processes[worker_index].status, ProcessStatus::Stopped);
        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert_eq!(run.delivered_messages.len(), 1);

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    PING_MESSAGE,
                    Some(worker_process_ref_payload(UNSPAWNED_WORKER_PID)),
                ),
                Some(main_pid),
            )
            .expect_err("send to stopped process should fail closed");

        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert_eq!(run.delivered_messages.len(), 1);
        err.to_string()
    };

    assert_eq!(err, "send to process Worker failed because it is stopped");
    assert_eq!(worker_ping_accepted_count(host.events()), 1);
    assert_eq!(worker_ping_dequeued_count(host.events()), 1);
    assert_eq!(worker_stopped_count(host.events()), 1);
}

#[test]
fn runtime_rejects_send_to_failed_process_before_acceptance() {
    let mut artifact = artifact_with_worker_process_ref_payload();
    artifact.processes[1].transitions[0].step_result = StepResult::Panic;
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let main_pid = run
            .spawn_process(MAIN_PROCESS, None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, Some(main_pid))
            .expect("worker process should spawn");
        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                PING_MESSAGE,
                Some(worker_process_ref_payload(worker_pid.as_u64())),
            ),
            Some(main_pid),
        )
        .expect("first send should be accepted before worker fails");
        let panic_err = run
            .drain_mailboxes(RunLimits::default().max_dispatches)
            .expect_err("worker panic should fail after consuming the first message");
        assert_eq!(
            panic_err.to_string(),
            "process Worker panicked after consuming message Ping; message will not be replayed"
        );

        let worker_index = process_index_for_pid(&run, worker_pid);
        assert_eq!(run.processes[worker_index].status, ProcessStatus::Failed);
        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert_eq!(run.delivered_messages.len(), 1);

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    PING_MESSAGE,
                    Some(worker_process_ref_payload(UNSPAWNED_WORKER_PID)),
                ),
                Some(main_pid),
            )
            .expect_err("send to failed process should fail closed");

        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert_eq!(run.delivered_messages.len(), 1);
        err.to_string()
    };

    assert_eq!(err, "send to process Worker failed because it has failed");
    assert_eq!(worker_ping_accepted_count(host.events()), 1);
    assert_eq!(worker_ping_dequeued_count(host.events()), 1);
    assert_eq!(worker_failed_count(host.events()), 1);
}

#[test]
fn runtime_rejects_full_mailbox_before_second_acceptance() {
    let artifact = artifact_with_worker_process_ref_payload();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let main_pid = run
            .spawn_process(MAIN_PROCESS, None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, Some(main_pid))
            .expect("worker process should spawn");
        let worker_index = process_index_for_pid(&run, worker_pid);

        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                PING_MESSAGE,
                Some(worker_process_ref_payload(worker_pid.as_u64())),
            ),
            Some(main_pid),
        )
        .expect("first send should fill worker mailbox");

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    PING_MESSAGE,
                    Some(worker_process_ref_payload(UNSPAWNED_WORKER_PID)),
                ),
                Some(main_pid),
            )
            .expect_err("second send to full mailbox should fail closed");

        assert_eq!(run.processes[worker_index].mailbox.len(), 1);
        assert_eq!(run.delivered_messages.len(), 1);
        err.to_string()
    };

    assert_eq!(
        err,
        "mailbox for process Worker is full; message was not accepted"
    );
    assert_eq!(worker_ping_accepted_count(host.events()), 1);
}

#[test]
fn runtime_rejects_unhandled_messages_after_stopped_process_drain() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].mailbox_bound = 2;
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let main_pid = run
            .spawn_process(MAIN_PROCESS, None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, Some(main_pid))
            .expect("worker process should spawn");
        let worker_index = process_index_for_pid(&run, worker_pid);

        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(PING_MESSAGE, None),
            Some(main_pid),
        )
        .expect("first send should be accepted");
        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(PING_MESSAGE, None),
            Some(main_pid),
        )
        .expect("second send should be accepted");

        run.drain_mailboxes(RunLimits::default().max_dispatches)
            .expect("drain should stop after worker stops");
        let err = run
            .reject_unhandled_messages()
            .expect_err("stopped process must not retain unhandled messages");

        assert_eq!(run.processes[worker_index].status, ProcessStatus::Stopped);
        assert_eq!(run.processes[worker_index].mailbox.len(), 1);
        assert_eq!(run.delivered_messages.len(), 2);
        err.to_string()
    };

    assert_eq!(err, "process Worker has 1 unhandled message(s)");
    assert_eq!(worker_ping_accepted_count(host.events()), 2);
    assert_eq!(worker_ping_dequeued_count(host.events()), 1);
    assert_eq!(worker_stopped_count(host.events()), 1);
}

#[test]
fn runtime_rejects_unspawned_process_ref_payload_before_acceptance() {
    let artifact = artifact_with_worker_process_ref_payload();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let main_pid = run
            .spawn_process(MAIN_PROCESS, None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, Some(main_pid))
            .expect("worker process should spawn");
        let worker_index = process_index_for_pid(&run, worker_pid);

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    PING_MESSAGE,
                    Some(worker_process_ref_payload(UNSPAWNED_WORKER_PID)),
                ),
                Some(main_pid),
            )
            .expect_err("unspawned process ref payload should fail closed");

        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert!(run.delivered_messages.is_empty());
        err.to_string()
    };

    assert_eq!(err, "runtime process 99 is not spawned");
    assert_no_worker_ping_accepted_event(host.events());
}

fn artifact_with_worker_process_ref_payload() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
    artifact
}

fn worker_process_ref_payload(pid: u64) -> RuntimePayload {
    runtime_payload(ArtifactPayload {
        ty: PROCESS_REF_WORKER,
        value: ArtifactValue::process_ref(PROCESS_REF_WORKER, pid),
        process_ref: Some(ArtifactProcessRefPayload {
            target_process: WORKER_PROCESS,
            pid,
        }),
    })
}

fn new_test_run<'program, 'host>(
    program: &'program LoadedProgram,
    host: &'host mut InMemoryRuntimeHost,
) -> RuntimeRun<'program, 'host, InMemoryRuntimeHost> {
    RuntimeRun::new(
        program,
        host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    )
}

fn process_index_for_pid(
    run: &RuntimeRun<'_, '_, InMemoryRuntimeHost>,
    pid: RuntimeProcessId,
) -> usize {
    run.process_index_for_pid(pid)
        .expect("spawned pid should resolve to a process index")
}

fn assert_no_worker_ping_accepted_event(events: &[RuntimeEvent]) {
    assert_eq!(worker_ping_accepted_count(events), 0);
}

fn worker_ping_accepted_count(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RuntimeEvent::MessageAccepted {
                    process_id,
                    process,
                    message_id,
                    message,
                    ..
                } if *process_id == WORKER_PROCESS
                    && process == "Worker"
                    && *message_id == PING_MESSAGE
                    && message == "Ping"
            )
        })
        .count()
}

fn worker_ping_dequeued_count(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RuntimeEvent::MessageDequeued {
                    process_id,
                    process,
                    message_id,
                    message,
                    ..
                } if *process_id == WORKER_PROCESS
                    && process == "Worker"
                    && *message_id == PING_MESSAGE
                    && message == "Ping"
            )
        })
        .count()
}

fn worker_stopped_count(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RuntimeEvent::ProcessStopped {
                    process_id,
                    process,
                    ..
                } if *process_id == WORKER_PROCESS && process == "Worker"
            )
        })
        .count()
}

fn worker_failed_count(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                RuntimeEvent::ProcessFailed {
                    process_id,
                    process,
                    ..
                } if *process_id == WORKER_PROCESS && process == "Worker"
            )
        })
        .count()
}
