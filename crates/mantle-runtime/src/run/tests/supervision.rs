use super::support::*;
use crate::event::{
    RuntimeStopReason, RuntimeSupervisorExitReason, RuntimeSupervisorRestartDecision,
};
use crate::host::RuntimeHost;
use crate::{
    ProcessStatus, RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason, RuntimeProcessId,
};
use mantle_artifact::{
    ArtifactSupervisorChild, ArtifactSupervisorChildMode, ArtifactSupervisorPlan,
    ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy, SupervisorChildId,
    SupervisorId,
};

mod admission;

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const HELPER_PROCESS: ProcessId = ProcessId::new(2);
const CRASH_MESSAGE: MessageId = MessageId::new(0);
const HELPER_STATE: TypeId = TypeId::new(4);
const HELPER_MSG: TypeId = TypeId::new(5);

#[test]
fn bounded_restart_intensity_denies_second_restart_within_window() {
    let artifact = supervisor_artifact(1, 1_000);
    let program = LoadedProgram::from_artifact(&artifact).expect("supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with supervised child");
    let first_child = current_child_pid(&run, main_pid);
    run.send_message(
        first_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("first crash message should enqueue");
    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("first crash should restart within intensity budget");

    let restarted_child = current_child_pid(&run, main_pid);
    assert_ne!(first_child, restarted_child);
    assert_eq!(status_for_pid(&run, first_child), ProcessStatus::Failed);
    assert_eq!(
        status_for_pid(&run, restarted_child),
        ProcessStatus::Running
    );

    run.send_message(
        restarted_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("second crash message should enqueue");
    let err = run
        .drain_mailboxes(RunLimits::default().max_dispatches)
        .expect_err("second crash inside window should be denied");

    assert!(
        err.to_string().contains("restart intensity exceeded"),
        "{err}"
    );
    assert_eq!(current_child_pid_opt(&run, main_pid), None);
    assert_eq!(status_for_pid(&run, main_pid), ProcessStatus::Failed);
    drop(run);

    let restart_decisions = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::SupervisorRestartDecision {
                decision,
                restart_time_ms,
                restart_window_count,
                restart_window_limit,
                restart_window_ms,
                ..
            } => Some((
                decision.as_str(),
                *restart_time_ms,
                *restart_window_count,
                *restart_window_limit,
                *restart_window_ms,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        restart_decisions,
        vec![
            ("restarted", Some(0), 1, 1, 1_000),
            ("denied", Some(1), 1, 1, 1_000),
        ]
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessFailed {
            pid,
            reason: RuntimeFailureReason::SupervisorRestartIntensityExceeded,
            ..
        } if *pid == main_pid
    )));
}

#[test]
fn permanent_child_restarts_after_normal_stop() {
    let mut artifact = supervisor_artifact(2, 1_000);
    artifact.processes[1].transitions[0].step_result = StepResult::Stop;
    let program = LoadedProgram::from_artifact(&artifact).expect("supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with supervised child");
    let first_child = current_child_pid(&run, main_pid);
    run.send_message(
        first_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("normal-stop message should enqueue");

    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("permanent child normal stop should restart");

    let restarted_child = current_child_pid(&run, main_pid);
    assert_ne!(first_child, restarted_child);
    assert_eq!(status_for_pid(&run, first_child), ProcessStatus::Stopped);
    assert_eq!(
        status_for_pid(&run, restarted_child),
        ProcessStatus::Running
    );
    drop(run);

    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::SupervisorRestartDecision {
            reason: RuntimeSupervisorExitReason::Normal,
            decision: RuntimeSupervisorRestartDecision::Restarted,
            new_child_pid: Some(pid),
            ..
        } if *pid == restarted_child
    )));
}

#[test]
fn restarted_supervisor_child_stops_its_old_supervised_subtree() {
    let artifact = nested_supervisor_artifact();
    let program =
        LoadedProgram::from_artifact(&artifact).expect("nested supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn nested supervised child");
    let first_worker = current_child_pid(&run, main_pid);
    let first_helper = current_child_pid(&run, first_worker);
    run.send_message(
        first_worker,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("crash message should enqueue");

    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("worker crash should restart worker and stop old subtree");

    let restarted_worker = current_child_pid(&run, main_pid);
    let restarted_helper = current_child_pid(&run, restarted_worker);
    assert_ne!(first_worker, restarted_worker);
    assert_ne!(first_helper, restarted_helper);
    assert_eq!(status_for_pid(&run, first_worker), ProcessStatus::Failed);
    assert_eq!(status_for_pid(&run, first_helper), ProcessStatus::Stopped);
    assert_eq!(
        status_for_pid(&run, restarted_worker),
        ProcessStatus::Running
    );
    assert_eq!(
        status_for_pid(&run, restarted_helper),
        ProcessStatus::Running
    );
    drop(run);

    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStopped {
            pid,
            reason: RuntimeStopReason::SupervisorFailure,
            ..
        } if *pid == first_helper
    )));
}

#[test]
fn restart_intensity_counts_across_children_in_one_supervisor() {
    let artifact = two_child_supervisor_artifact();
    let program =
        LoadedProgram::from_artifact(&artifact).expect("two-child supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn two supervised children");
    let first_child = current_child_pid_at(&run, main_pid, 0);
    let second_child = current_child_pid_at(&run, main_pid, 1);
    run.send_message(
        first_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("first child crash message should enqueue");
    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("first child restart should fit intensity budget");

    run.send_message(
        second_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("second child crash message should enqueue");
    let err = run
        .drain_mailboxes(RunLimits::default().max_dispatches)
        .expect_err("second child restart should exceed shared supervisor intensity");

    assert!(
        err.to_string().contains("restart intensity exceeded"),
        "{err}"
    );
    assert_eq!(current_child_pid_opt_at(&run, main_pid, 1), None);
    assert_eq!(status_for_pid(&run, main_pid), ProcessStatus::Failed);
}

#[test]
fn default_restart_throttle_fails_scope_on_same_tick_restart() {
    let artifact = supervisor_artifact(2, 1_000);
    let program = LoadedProgram::from_artifact(&artifact).expect("supervisor artifact should load");
    let mut host = StaticClockRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with supervised child");
    let first_child = current_child_pid(&run, main_pid);
    run.send_message(
        first_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("first crash message should enqueue");
    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("first crash should restart");

    let restarted_child = current_child_pid(&run, main_pid);
    run.send_message(
        restarted_child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("second crash message should enqueue");
    let err = run
        .drain_mailboxes(RunLimits::default().max_dispatches)
        .expect_err("same-tick restart should be throttled");

    assert!(err.to_string().contains("restart throttled"), "{err}");
    assert_eq!(current_child_pid_opt(&run, main_pid), None);
    assert_eq!(status_for_pid(&run, main_pid), ProcessStatus::Failed);
    drop(run);

    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessFailed {
            pid,
            reason: RuntimeFailureReason::SupervisorRestartThrottled,
            ..
        } if *pid == main_pid
    )));
}

#[test]
fn supervisor_children_start_in_declaration_order_and_stop_in_reverse_order() {
    let artifact = two_child_supervisor_artifact();
    let program =
        LoadedProgram::from_artifact(&artifact).expect("two-child supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn two supervised children");
    let first_child = current_child_pid_at(&run, main_pid, 0);
    let second_child = current_child_pid_at(&run, main_pid, 1);
    assert_ne!(first_child, second_child);

    run.stop_supervised_children(main_pid, RuntimeStopReason::SupervisorShutdown)
        .expect("supervisor should stop children");
    drop(run);

    let child_start_order = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::SupervisorChildStarted {
                child_id,
                child_pid,
                ..
            } => Some((child_id.as_u32(), child_pid.as_u64())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        child_start_order,
        vec![(0, first_child.as_u64()), (1, second_child.as_u64())]
    );

    let child_stop_order = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::ProcessStopped {
                pid,
                reason: RuntimeStopReason::SupervisorShutdown,
                ..
            } if *pid == first_child || *pid == second_child => Some(pid.as_u64()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        child_stop_order,
        vec![second_child.as_u64(), first_child.as_u64()]
    );
}

#[test]
fn supervisor_child_slot_pid_mismatch_fails_closed() {
    let artifact = supervisor_artifact(2, 1_000);
    let program = LoadedProgram::from_artifact(&artifact).expect("supervisor artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with supervised child");
    let child = current_child_pid(&run, main_pid);
    let main_index = run
        .process_index_for_pid(main_pid)
        .expect("main pid should resolve");
    run.processes[main_index].supervisors[0].children[0].current_pid =
        RuntimeProcessId::from_u64(99).ok();

    run.send_message(
        child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("child crash message should enqueue");
    let err = run
        .drain_mailboxes(RunLimits::default().max_dispatches)
        .expect_err("stale supervisor slot must fail closed");

    assert!(
        err.to_string()
            .contains("runtime supervisor child slot for pid"),
        "{err}"
    );
    assert_eq!(status_for_pid(&run, child), ProcessStatus::Failed);
}

#[test]
fn restart_capacity_denial_records_supervisor_decision() {
    let artifact = supervisor_artifact(2, 1_000);
    let program = LoadedProgram::from_artifact(&artifact).expect("supervisor artifact should load");
    let limits = RunLimits {
        max_runtime_processes: 2,
        ..RunLimits::default()
    };
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, limits);

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with supervised child");
    let child = current_child_pid(&run, main_pid);
    run.send_message(
        child,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("child crash message should enqueue");
    let err = run
        .drain_mailboxes(RunLimits::default().max_dispatches)
        .expect_err("restart should be denied when no process slot remains");

    assert!(
        err.to_string().contains("restart capacity exceeded"),
        "{err}"
    );
    assert_eq!(current_child_pid_opt(&run, main_pid), None);
    assert_eq!(status_for_pid(&run, main_pid), ProcessStatus::Failed);
    drop(run);

    let restart_decisions = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::SupervisorRestartDecision {
                decision,
                restart_time_ms,
                restart_window_count,
                restart_window_limit,
                restart_window_ms,
                new_child_pid,
                ..
            } => Some((
                decision.as_str(),
                *restart_time_ms,
                *restart_window_count,
                *restart_window_limit,
                *restart_window_ms,
                new_child_pid.map(RuntimeProcessId::as_u64),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        restart_decisions,
        vec![("denied", Some(0), 0, 2, 1_000, None)]
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessFailed {
            pid,
            reason: RuntimeFailureReason::SupervisorRestartCapacityExceeded,
            ..
        } if *pid == main_pid
    )));
}

fn supervisor_artifact(max_restarts: u32, within_ms: u64) -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "supervisor_restart_intensity".to_string(),
        entry_process: MAIN_PROCESS,
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
            ArtifactType::value("WorkerState"),
            ArtifactType::enum_value("WorkerMsg", vec!["Crash".to_string()]),
        ],
        outputs: Vec::new(),
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![main_process(max_restarts, within_ms), worker_process()],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn main_process(max_restarts: u32, within_ms: u64) -> ArtifactProcess {
    ArtifactProcess {
        debug_name: "Main".to_string(),
        state_type: MAIN_STATE,
        state_values: state_values(MAIN_STATE, &["MainState"]),
        message_type: MAIN_MSG,
        message_variants: vec![ArtifactMessageVariant::unit("Start")],
        authorities: Vec::new(),
        spawn_sites: vec![ArtifactSpawnSite {
            target: WORKER_PROCESS,
            authority: None,
            supervisor: Some(SupervisorId::new(0)),
            child: Some(SupervisorChildId::new(0)),
            kind: ArtifactSpawnKind::LexicalSupervisorChild,
        }],
        supervisor_plans: vec![ArtifactSupervisorPlan {
            strategy: ArtifactSupervisorStrategy::OneForOne,
            intensity: ArtifactSupervisorRestartIntensity {
                max_restarts,
                within_ms,
            },
            children: vec![ArtifactSupervisorChild {
                debug_name: "worker".to_string(),
                target: WORKER_PROCESS,
                mode: ArtifactSupervisorChildMode::Permanent,
                spawn_site: SPAWN_SITE,
            }],
        }],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(0),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Continue,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    }
}

fn two_child_supervisor_artifact() -> MantleArtifact {
    let mut artifact = supervisor_artifact(1, 1_000);
    artifact.module = "two_child_restart_intensity".to_string();
    artifact.processes[0].spawn_sites.push(ArtifactSpawnSite {
        target: WORKER_PROCESS,
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(1)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    });
    artifact.processes[0].supervisor_plans[0]
        .children
        .push(ArtifactSupervisorChild {
            debug_name: "worker_b".to_string(),
            target: WORKER_PROCESS,
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SpawnSiteId::new(1),
        });
    artifact.processes[0].supervisor_plans[0].children[0].debug_name = "worker_a".to_string();
    artifact
}

fn worker_process() -> ArtifactProcess {
    ArtifactProcess {
        debug_name: "Worker".to_string(),
        state_type: WORKER_STATE,
        state_values: state_values(WORKER_STATE, &["WorkerState"]),
        message_type: WORKER_MSG,
        message_variants: vec![ArtifactMessageVariant::unit("Crash")],
        authorities: Vec::new(),
        spawn_sites: Vec::new(),
        supervisor_plans: Vec::new(),
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(0),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: CRASH_MESSAGE,
            payload_guard: None,
            step_result: StepResult::Panic,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    }
}

fn nested_supervisor_artifact() -> MantleArtifact {
    let mut artifact = supervisor_artifact(2, 1_000);
    artifact.module = "nested_supervisor_restart".to_string();
    artifact.types.push(ArtifactType::value("HelperState"));
    artifact.types.push(ArtifactType::enum_value(
        "HelperMsg",
        vec!["Wait".to_string()],
    ));
    artifact.processes[1].spawn_sites = vec![ArtifactSpawnSite {
        target: HELPER_PROCESS,
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: ArtifactSpawnKind::LexicalSupervisorChild,
    }];
    artifact.processes[1].supervisor_plans = vec![ArtifactSupervisorPlan {
        strategy: ArtifactSupervisorStrategy::OneForOne,
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![ArtifactSupervisorChild {
            debug_name: "helper".to_string(),
            target: HELPER_PROCESS,
            mode: ArtifactSupervisorChildMode::Permanent,
            spawn_site: SPAWN_SITE,
        }],
    }];
    artifact.processes.push(helper_process());
    artifact
}

fn helper_process() -> ArtifactProcess {
    ArtifactProcess {
        debug_name: "Helper".to_string(),
        state_type: HELPER_STATE,
        state_values: state_values(HELPER_STATE, &["HelperState"]),
        message_type: HELPER_MSG,
        message_variants: vec![ArtifactMessageVariant::unit("Wait")],
        authorities: Vec::new(),
        spawn_sites: Vec::new(),
        supervisor_plans: Vec::new(),
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: StateId::new(0),
        transitions: vec![ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Continue,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    }
}

fn current_child_pid<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, H>,
    main_pid: RuntimeProcessId,
) -> RuntimeProcessId {
    current_child_pid_at(run, main_pid, 0)
}

fn current_child_pid_at<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, H>,
    main_pid: RuntimeProcessId,
    child_index: usize,
) -> RuntimeProcessId {
    current_child_pid_opt_at(run, main_pid, child_index)
        .expect("supervisor child should be running")
}

fn current_child_pid_opt<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, H>,
    main_pid: RuntimeProcessId,
) -> Option<RuntimeProcessId> {
    current_child_pid_opt_at(run, main_pid, 0)
}

fn current_child_pid_opt_at<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, H>,
    main_pid: RuntimeProcessId,
    child_index: usize,
) -> Option<RuntimeProcessId> {
    let main_index = run
        .process_index_for_pid(main_pid)
        .expect("main pid should resolve");
    run.processes[main_index].supervisors[0].children[child_index].current_pid
}

fn status_for_pid<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, H>,
    pid: RuntimeProcessId,
) -> ProcessStatus {
    let index = run.process_index_for_pid(pid).expect("pid should resolve");
    run.processes[index].status
}

#[derive(Default)]
struct StaticClockRuntimeHost {
    events: Vec<RuntimeEvent>,
}

impl StaticClockRuntimeHost {
    fn events(&self) -> &[RuntimeEvent] {
        &self.events
    }
}

impl RuntimeHost for StaticClockRuntimeHost {
    fn record_event(&mut self, event: RuntimeEventRecord) -> mantle_artifact::Result<()> {
        self.events.push(event.into_event());
        Ok(())
    }

    fn emit_stdout(&mut self, _text: &str) -> mantle_artifact::Result<()> {
        Ok(())
    }

    fn monotonic_ms(&mut self) -> mantle_artifact::Result<u64> {
        Ok(0)
    }

    fn flush(&mut self) -> mantle_artifact::Result<()> {
        Ok(())
    }
}
