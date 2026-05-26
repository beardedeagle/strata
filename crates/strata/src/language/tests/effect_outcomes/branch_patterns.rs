use super::super::support::*;
use super::{EFFECT_OUTCOMES, contains_effect_outcome_template};

#[test]
fn checks_and_lowers_typed_effect_outcome_bindings_without_source_name_dispatch() {
    let checked = check_source(EFFECT_OUTCOMES).expect("effect outcomes should check");
    let main = checked
        .processes()
        .get(checked.entry_process().index())
        .expect("Main process should exist");

    assert!(matches!(
        main.transitions()[0].actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::SendOutcome { .. }
        ]
    ));
    assert!(
        main.transitions()
            .iter()
            .any(|transition| matches!(transition.actions(), [CheckedAction::SpawnOutcome { .. }]))
    );

    let artifact = lower_to_artifact(&checked, EFFECT_OUTCOMES)
        .expect("effect outcomes should lower to artifact");
    let main_artifact = &artifact.processes[checked.entry_process().index()];
    let encoded = artifact.encode();

    assert!(matches!(
        main_artifact.transitions[0].actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome { .. }
        ]
    ));
    assert!(main_artifact.transitions.iter().any(|transition| matches!(
        transition.actions.as_slice(),
        [ArtifactAction::SpawnOutcome { .. }]
    )));
    assert!(contains_effect_outcome_template(
        &main_artifact.transitions[0].next_state
    ));
    assert!(!encoded.contains("sent"));
    assert!(!encoded.contains("spawned"));
}

#[test]
fn branches_over_typed_effect_outcome_for_follow_up_effects() {
    let checked = check_source(include_str!(
        "../../../../../../examples/effect_outcomes.str"
    ))
    .expect("effect outcome branching example should check");
    let artifact = lower_to_artifact(
        &checked,
        include_str!("../../../../../../examples/effect_outcomes.str"),
    )
    .expect("effect outcome branching example should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::SpawnOutcome { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome { .. },
            ArtifactAction::IfElse { .. },
            ArtifactAction::IfElse { .. }
        ]
    ));
}

#[test]
fn checks_and_lowers_send_outcome_for_direct_process_ref_message_payload() {
    let source = r#"
module process_ref_send_outcome;

enum MainState { Ready }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Forward(ProcessRef<Worker>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Forward(worker);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Forward(reply_to: ProcessRef<Worker>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("process-ref send outcome should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("process-ref send outcome should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome {
                payload: Some(ArtifactValueTemplate::ProcessRef { .. }),
                ..
            }
        ]
    ));
    assert!(!artifact.encode().contains("sent"));
}

#[test]
fn branches_over_send_outcome_with_payload_bearing_message_by_success_variant() {
    let source = r#"
module payload_send_outcome_branch;

enum MainState { Ready }
enum MainMsg { Start }
enum Bool { False, True }
record Job { phase: Phase }
enum Phase { Ready }
enum WorkerState { Idle }
enum WorkerMsg { Work(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit, spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Work(Job { phase: Ready });
        if (sent == Ok(Unit)) {
            emit "sent";
        } else {
            emit "returned";
        }
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Work(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("payload send outcome success branch should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("payload send outcome branch should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome { .. },
            ArtifactAction::IfElse { .. }
        ]
    ));
}

#[test]
fn branches_over_process_ref_send_outcome_by_success_variant() {
    let source = r#"
module process_ref_send_outcome_branch;

enum MainState { Ready }
enum MainMsg { Start }
enum Bool { False, True }
enum WorkerState { Idle }
enum WorkerMsg { Forward(ProcessRef<Worker>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit, spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Forward(worker);
        if (sent == Ok(Unit)) {
            emit "forwarded";
        } else {
            emit "returned";
        }
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Forward(reply_to: ProcessRef<Worker>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("process-ref outcome success branch should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("process-ref outcome branch should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome { .. },
            ArtifactAction::IfElse { .. }
        ]
    ));
}

#[test]
fn branches_over_spawn_outcome_by_error_variant_without_process_ref_equality() {
    let source = r#"
module spawn_outcome_branch;

enum MainState { Ready }
enum MainMsg { Start }
enum Bool { False, True }
enum WorkerState { Idle }
enum WorkerMsg { Work }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit, spawn] ~ [] @det {
        let spawned: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
        if (spawned != Err(Exhausted(Unit))) {
            emit "spawned";
        } else {
            emit "not spawned";
        }
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("spawn outcome failure branch should check");
    let artifact = lower_to_artifact(&checked, source).expect("spawn outcome branch should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::SpawnOutcome { .. },
            ArtifactAction::IfElse { .. }
        ]
    ));
}

#[test]
fn static_runtime_order_binds_send_outcome_before_branching_without_state_dependency() {
    let source = r#"
module effect_outcome_runtime_order_branch;

enum MainState { Ready }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Work }
enum Bool { False, True }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Ready;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit, spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let first: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        if (sent == Err(Full(Work))) {
            emit "full";
        } else {
            emit "unexpected";
        }
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source)
        .expect("send outcome branch should check after binding typed Full result");
    let artifact = lower_to_artifact(&checked, source).expect("send outcome branch should lower");
    let main = &artifact.processes[checked.entry_process().index()];

    assert!(matches!(
        main.transitions[0].actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::SendOutcome { .. },
            ArtifactAction::SendOutcome { .. },
            ArtifactAction::IfElse { .. }
        ]
    ));
}
