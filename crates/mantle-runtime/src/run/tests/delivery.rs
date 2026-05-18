use std::collections::BTreeMap;

use super::support::*;
use crate::{ProcessStatus, RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeProcessId};
use mantle_artifact::ArtifactBranch;

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const PING_MESSAGE: MessageId = MessageId::new(0);
const UNSPAWNED_WORKER_PID: u64 = 99;
const BOOL: TypeId = TypeId::new(10);

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
fn runtime_action_send_rejects_stopped_process_before_payload_template_evaluation() {
    let artifact = artifact_with_worker_job_payload();
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
        run.processes[worker_index].status = ProcessStatus::Stopped;

        let mut process_refs = worker_ref_binding(worker_pid);
        let step = main_step(main_pid);
        let action = failing_current_state_payload_send();
        let err = run
            .execute_action(
                &mut process_refs,
                &step,
                &action,
                RuntimeBranchPath::root(),
                &[],
            )
            .expect_err("stopped target should reject before payload template evaluation");

        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert!(run.delivered_messages.is_empty());
        err.to_string()
    };

    assert_eq!(err, "send to process Worker failed because it is stopped");
    assert_no_worker_ping_accepted_event(host.events());
}

#[test]
fn runtime_action_send_rejects_failed_process_before_payload_template_evaluation() {
    let artifact = artifact_with_worker_job_payload();
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
        run.processes[worker_index].status = ProcessStatus::Failed;

        let mut process_refs = worker_ref_binding(worker_pid);
        let step = main_step(main_pid);
        let action = failing_current_state_payload_send();
        let err = run
            .execute_action(
                &mut process_refs,
                &step,
                &action,
                RuntimeBranchPath::root(),
                &[],
            )
            .expect_err("failed target should reject before payload template evaluation");

        assert_eq!(run.processes[worker_index].mailbox.len(), 0);
        assert!(run.delivered_messages.is_empty());
        err.to_string()
    };

    assert_eq!(err, "send to process Worker failed because it has failed");
    assert_no_worker_ping_accepted_event(host.events());
}

#[test]
fn runtime_action_send_rejects_full_mailbox_before_payload_template_evaluation() {
    let artifact = artifact_with_worker_job_payload();
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
            RuntimeMessageEnvelope::new(PING_MESSAGE, Some(job_payload())),
            Some(main_pid),
        )
        .expect("first send should fill worker mailbox");

        let mut process_refs = worker_ref_binding(worker_pid);
        let step = main_step(main_pid);
        let action = failing_current_state_payload_send();
        let err = run
            .execute_action(
                &mut process_refs,
                &step,
                &action,
                RuntimeBranchPath::root(),
                &[],
            )
            .expect_err("full mailbox should reject before payload template evaluation");

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

#[test]
fn runtime_traces_distinct_nested_branches_with_identical_conditions() {
    let artifact = artifact_with_nested_worker_bool_branch();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program
        .validate_admission()
        .expect("nested branch artifact should admit");
    let mut host = InMemoryRuntimeHost::default();

    {
        let mut run = new_test_run(&program, &mut host);
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, None)
            .expect("worker process should spawn");
        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(PING_MESSAGE, Some(bool_payload(true))),
            None,
        )
        .expect("Bool payload should be accepted");
        run.drain_mailboxes(RunLimits::default().max_dispatches)
            .expect("worker should process nested branch");
    }

    let selected_branch_paths = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::BranchSelected {
                process_id,
                process,
                message_id,
                branch,
                scope,
                branch_path,
                condition,
                ..
            } if *process_id == WORKER_PROCESS
                && process == "Worker"
                && *message_id == PING_MESSAGE
                && *branch == ArtifactBranch::Then
                && *scope == RuntimeBranchScope::NextState
                && condition == "True" =>
            {
                Some(*branch_path)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        selected_branch_paths.len(),
        2,
        "outer and nested branch nodes should each record branch_selected"
    );
    assert_eq!(selected_branch_paths[0], RuntimeBranchPath::root());
    assert_eq!(
        selected_branch_paths[1].segments(),
        [0x4000],
        "nested then-state branch should carry a stable typed branch path"
    );

    let distinct_paths = selected_branch_paths
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    assert_eq!(
        distinct_paths, 2,
        "branch_selected events with identical scope and condition must remain distinguishable"
    );
}

#[test]
fn runtime_rejects_message_payload_outside_declared_enum_before_acceptance() {
    let artifact = artifact_with_nested_worker_bool_branch();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program
        .validate_admission()
        .expect("Bool payload artifact should admit");
    let mut host = InMemoryRuntimeHost::default();

    let err = {
        let mut run = new_test_run(&program, &mut host);
        let worker_pid = run
            .spawn_process(WORKER_PROCESS, None)
            .expect("worker process should spawn");
        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                PING_MESSAGE,
                Some(
                    RuntimePayload::value(BOOL, RuntimeValue::Atom("Maybe".to_string()))
                        .expect("test malformed Bool payload should construct"),
                ),
            ),
            None,
        )
        .expect_err("payload outside enum variants should fail before acceptance")
        .to_string()
    };

    assert!(
        err.contains("payload value Maybe is not a member of enum type Bool"),
        "{err}"
    );
    assert_no_worker_ping_accepted_event(host.events());
}

fn artifact_with_worker_process_ref_payload() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
    artifact
}

fn artifact_with_nested_worker_bool_branch() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    assert_eq!(bool_type, BOOL);
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Idle", "Handled", "Done"]);
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", bool_type)];
    let condition = ArtifactValueTemplate::ReceivedPayload { ty: bool_type };
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: condition.clone(),
        then_state: Box::new(NextState::IfElse {
            condition,
            then_state: Box::new(NextState::Value(StateId::new(1))),
            else_state: Box::new(NextState::Value(StateId::new(2))),
        }),
        else_state: Box::new(NextState::Value(StateId::new(2))),
    };
    artifact
}

fn artifact_with_worker_job_payload() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Ping", JOB)];
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

fn job_payload() -> RuntimePayload {
    runtime_payload(
        ArtifactPayload::value(JOB, artifact_value("Job{phase:Ready}"))
            .expect("test job payload should construct"),
    )
}

fn bool_payload(value: bool) -> RuntimePayload {
    let label = if value { "True" } else { "False" };
    runtime_payload(
        ArtifactPayload::value(BOOL, artifact_value(label))
            .expect("test Bool payload should construct"),
    )
}

fn failing_current_state_payload_send() -> LoadedAction {
    LoadedAction::Send {
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: PING_MESSAGE,
        payload: Some(loaded_template(
            ArtifactValueTemplate::CurrentStatePayload { ty: JOB },
        )),
    }
}

fn worker_ref_binding(worker_pid: RuntimeProcessId) -> BTreeMap<ProcessRefId, RuntimeProcessId> {
    BTreeMap::from([(ProcessRefId::new(0), worker_pid)])
}

fn main_step(main_pid: RuntimeProcessId) -> ActiveStep {
    ActiveStep {
        pid: main_pid,
        process_id: MAIN_PROCESS,
        process_name: "Main".to_string(),
        current_state: StateId::new(0),
        message: PING_MESSAGE,
        message_label: "Start".to_string(),
        payload: None,
    }
}

fn new_test_run<'program, 'host>(
    program: &'program LoadedProgram,
    host: &'host mut InMemoryRuntimeHost,
) -> RuntimeRun<'program, 'host, InMemoryRuntimeHost> {
    RuntimeRun::new(program, host, RunLimits::default())
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
