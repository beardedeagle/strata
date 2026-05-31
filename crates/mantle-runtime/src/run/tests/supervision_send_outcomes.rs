use super::support::*;
use crate::RuntimeProcessId;
use crate::host::RuntimeHost;
use crate::program::LoadedAction;
use mantle_artifact::{
    ArtifactEnumVariant, ArtifactSupervisorChild, ArtifactSupervisorChildMode,
    ArtifactSupervisorPlan, ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy,
    EffectOutcomeId, SupervisorChildId, SupervisorId,
};

const MAIN_PROCESS: ProcessId = ProcessId::new(0);
const WORKER_PROCESS: ProcessId = ProcessId::new(1);
const MAIN_STATE: TypeId = TypeId::new(0);
const MAIN_MSG: TypeId = TypeId::new(1);
const WORKER_STATE: TypeId = TypeId::new(2);
const WORKER_MSG: TypeId = TypeId::new(3);
const UNIT: TypeId = TypeId::new(4);
const SEND_ERROR: TypeId = TypeId::new(5);
const SEND_RESULT: TypeId = TypeId::new(6);
const CRASH_MESSAGE: MessageId = MessageId::new(0);

#[test]
fn send_outcome_to_inactive_stopped_supervisor_child_returns_stopped() {
    assert_inactive_supervisor_child_send_outcome(StepResult::Stop, "Err(Stopped(Crash))");
}

#[test]
fn send_outcome_to_inactive_failed_supervisor_child_returns_crashed() {
    assert_inactive_supervisor_child_send_outcome(StepResult::Panic, "Err(Crashed(Crash))");
}

fn assert_inactive_supervisor_child_send_outcome(exit: StepResult, expected: &str) {
    let artifact = supervisor_send_outcome_artifact(exit);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());

    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main process should spawn with temporary child");
    let child_pid = current_child_pid(&run, main_pid);
    run.send_message(
        child_pid,
        RuntimeMessageEnvelope::new(CRASH_MESSAGE, None),
        Some(main_pid),
    )
    .expect("child exit message should enqueue");
    run.drain_mailboxes(RunLimits::default().max_dispatches)
        .expect("temporary child exit should not restart or fail the supervisor");
    assert_eq!(current_child_pid_opt(&run, main_pid), None);
    let deliveries_before = run.delivered_messages.len();

    let step = main_step(main_pid);
    let action = LoadedAction::SendOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SEND_RESULT,
        target: LoadedSendTarget::SupervisorChild {
            supervisor: SupervisorId::new(0),
            child: SupervisorChildId::new(0),
            target_process: WORKER_PROCESS,
        },
        port: None,
        message: CRASH_MESSAGE,
        payload: None,
    };
    let mut refs = LocalProcessRefs::empty();
    let mut outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut refs, &step, &action, &mut outcomes)
        .expect("inactive supervisor child should bind typed send failure");

    assert!(handled);
    assert_eq!(outcomes[0].payload.label(), expected);
    assert_eq!(run.delivered_messages.len(), deliveries_before);
}

fn supervisor_send_outcome_artifact(exit: StepResult) -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.into(),
        schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        source_language: TEST_SOURCE_LANGUAGE.into(),
        target_requirements: test_target_requirements(),
        module: "supervision_send_outcomes".to_string(),
        entry_process: MAIN_PROCESS,
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
            ArtifactType::value("WorkerState"),
            ArtifactType::enum_value("WorkerMsg", vec!["Crash".to_string()]),
            ArtifactType::value("Unit"),
            send_error_type(),
            result_type(),
        ],
        outputs: Vec::new(),
        protocols: Vec::new(),
        ports: Vec::new(),
        components: Vec::new(),
        compositions: Vec::new(),
        processes: vec![main_process(), worker_process(exit)],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn main_process() -> ArtifactProcess {
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
                max_restarts: 2,
                within_ms: 1_000,
            },
            children: vec![ArtifactSupervisorChild {
                debug_name: "worker".to_string(),
                target: WORKER_PROCESS,
                mode: ArtifactSupervisorChildMode::Temporary,
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

fn worker_process(exit: StepResult) -> ArtifactProcess {
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
            step_result: exit,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        }],
    }
}

fn result_type() -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "Result",
        vec![
            ArtifactEnumVariant {
                label: "Ok".to_string(),
                payload_type: Some(UNIT),
            },
            ArtifactEnumVariant {
                label: "Err".to_string(),
                payload_type: Some(SEND_ERROR),
            },
        ],
    )
}

fn send_error_type() -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SendError",
        ["Full", "Stopped", "Crashed", "MailboxClosed"]
            .into_iter()
            .map(|label| ArtifactEnumVariant {
                label: label.to_string(),
                payload_type: Some(WORKER_MSG),
            })
            .collect(),
    )
}

fn current_child_pid<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, '_, H>,
    main_pid: RuntimeProcessId,
) -> RuntimeProcessId {
    current_child_pid_opt(run, main_pid).expect("supervisor child should be running")
}

fn current_child_pid_opt<H: RuntimeHost>(
    run: &RuntimeRun<'_, '_, '_, H>,
    main_pid: RuntimeProcessId,
) -> Option<RuntimeProcessId> {
    let main_index = run
        .process_index_for_pid(main_pid)
        .expect("main pid should resolve");
    run.processes[main_index].supervisors[0].children[0].current_pid
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
