use super::support::*;

const STEP_RETURN_MATCH: &str = r#"
module process_return_match;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, SawReady, Done }
enum WorkerMsg { Envelope(Route) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return match phase {
            Ready => {
                return Continue(SawReady);
            }
            Done => {
                return Stop(Done);
            }
        };
    }
}
"#;

#[test]
fn checks_step_return_match_over_concrete_message_payload_binding() {
    let checked = check_source(STEP_RETURN_MATCH).expect("step return match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert_eq!(checked_state_labels(worker), ["Idle", "SawReady", "Done"]);
    assert_eq!(worker.transitions().len(), 2);
    let mut transitions = worker
        .transitions()
        .iter()
        .map(|transition| {
            (
                transition
                    .payload_guard()
                    .map(|payload| payload.label().to_string())
                    .expect("return-match transition should carry a concrete payload guard"),
                transition.step_result(),
                transition.next_state(),
            )
        })
        .collect::<Vec<_>>();
    transitions.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        transitions,
        [
            (
                "Assign(Done)".to_string(),
                CheckedStepResult::Stop,
                CheckedNextState::Value(checked_state_id(2)),
            ),
            (
                "Assign(Ready)".to_string(),
                CheckedStepResult::Continue,
                CheckedNextState::Value(checked_state_id(1)),
            ),
        ]
    );

    lower_to_artifact(&checked, STEP_RETURN_MATCH)
        .expect("step return match should lower without runtime schema changes");
}

#[test]
fn checks_step_return_match_constructor_payload_binding() {
    let source = r#"
module process_return_match_payload_binding;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum PhaseBox { Wrap(Phase) }
enum Route { Assign(PhaseBox) }
enum WorkerState { Idle, Seen(Phase) }
enum WorkerMsg { Envelope(Route) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Wrap(Ready)));
        send worker Envelope(Assign(Wrap(Done)));
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(boxed: PhaseBox))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return match boxed {
            Wrap(phase: Phase) => {
                return Continue(Seen(phase));
            }
        };
    }
}
"#;

    let checked = check_source(source).expect("constructor payload return match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "Seen(Done)", "Seen(Ready)"]
    );
    let mut transitions = worker
        .transitions()
        .iter()
        .map(|transition| {
            (
                transition
                    .payload_guard()
                    .map(|payload| payload.label().to_string())
                    .expect("return-match transition should carry a concrete payload guard"),
                transition.next_state(),
            )
        })
        .collect::<Vec<_>>();
    transitions.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        transitions,
        [
            (
                "Assign(Wrap(Done))".to_string(),
                CheckedNextState::Value(checked_state_id(1)),
            ),
            (
                "Assign(Wrap(Ready))".to_string(),
                CheckedNextState::Value(checked_state_id(2)),
            ),
        ]
    );

    lower_to_artifact(&checked, source)
        .expect("constructor payload return match should lower through typed state values");
}

#[test]
fn checks_step_return_match_arm_can_preserve_current_state() {
    let source = STEP_RETURN_MATCH.replace("return Continue(SawReady);", "return Continue(state);");

    let checked =
        check_source(&source).expect("step return match should allow current-state returns");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let ready_transition = worker
        .transitions()
        .iter()
        .find(|transition| {
            transition
                .payload_guard()
                .map(|payload| payload.label() == "Assign(Ready)")
                .unwrap_or(false)
        })
        .expect("Ready transition should be generated");

    assert_eq!(ready_transition.next_state(), CheckedNextState::Current);
    lower_to_artifact(&checked, &source)
        .expect("current-state return-match arm should lower through existing transition shape");
}

#[test]
fn rejects_step_return_match_arm_binding_conflict_when_returning_current_state() {
    let source = r#"
module process_return_match_current_state_binding_conflict;

record MainState;
enum MainMsg { Start }
enum Phase { Ready }
enum PhaseBox { Wrap(Phase) }
enum Route { Assign(PhaseBox) }
enum WorkerState { Idle }
enum WorkerMsg { Envelope(Route) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Wrap(Ready)));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(boxed: PhaseBox))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return match boxed {
            Wrap(boxed: Phase) => {
                return Continue(state);
            }
        };
    }
}
"#;

    let err = check_source(source).expect_err("return-match binding conflict should fail");

    assert!(err.to_string().contains(
        "process Worker step return match payload binding boxed conflicts with an existing source value binding"
    ));
}

#[test]
fn checks_step_return_match_state_is_available_to_state_match_expansion() {
    let source = r#"
module process_return_match_state_match_domain;

record MainState;
enum MainMsg { Start }
enum Phase { Ready }
enum Route { Assign(Phase) }
enum WorkerState { Idle, Seen(Phase), Done }
enum WorkerMsg { Envelope(Route), Tick }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Tick;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return match phase {
            Ready => {
                return Continue(Seen(phase));
            }
        };
    }

    fn step(state: WorkerState, Tick) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Idle => {
                return Stop(Idle);
            }
            Seen(phase: Phase) => {
                return Stop(Done);
            }
            Done => {
                return Stop(Done);
            }
        }
    }
}
"#;

    let checked =
        check_source(source).expect("return-match state should feed state-match expansion");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let labels = checked_state_labels(worker);
    let seen_state = labels
        .iter()
        .position(|label| *label == "Seen(Ready)")
        .map(checked_state_id)
        .expect("return match should preadmit Seen(Ready)");

    assert!(
        worker.transitions().iter().any(|transition| {
            transition.current_state() == Some(seen_state)
                && transition.step_result() == CheckedStepResult::Stop
        }),
        "Tick should have a state-specific transition for Seen(Ready)"
    );

    lower_to_artifact(&checked, source)
        .expect("return-match-fed state match should lower through typed state ids");
}

#[test]
fn checks_step_return_match_over_concrete_state_payload_binding() {
    let source = r#"
module process_return_match_state_payload_binding;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, Seen(Phase), Done }
enum WorkerMsg { Envelope(Route), Tick }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        send worker Tick;
        return Stop(state);
    }
}

proc Worker mailbox bounded(3) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Seen(phase));
    }

    fn step(state: WorkerState, Tick) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Idle => {
                return Stop(Idle);
            }
            Seen(phase: Phase) => {
                return match phase {
                    Ready => {
                        return Continue(Done);
                    }
                    Done => {
                        return Stop(Done);
                    }
                };
            }
            Done => {
                return Stop(Done);
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("state payload return match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let labels = checked_state_labels(worker);
    let seen_ready = labels
        .iter()
        .position(|label| *label == "Seen(Ready)")
        .map(checked_state_id)
        .expect("Seen(Ready) should be admitted");
    let seen_done = labels
        .iter()
        .position(|label| *label == "Seen(Done)")
        .map(checked_state_id)
        .expect("Seen(Done) should be admitted");

    assert!(worker.transitions().iter().any(|transition| {
        transition.current_state() == Some(seen_ready)
            && transition.step_result() == CheckedStepResult::Continue
    }));
    assert!(worker.transitions().iter().any(|transition| {
        transition.current_state() == Some(seen_done)
            && transition.step_result() == CheckedStepResult::Stop
    }));

    lower_to_artifact(&checked, source)
        .expect("state payload return match should lower through typed current-state cases");
}

#[test]
fn rejects_step_return_match_over_non_concrete_payload_binding() {
    let source = STEP_RETURN_MATCH
        .replace("Envelope(Assign(phase: Phase))", "Envelope(route: Route)")
        .replace("return match phase", "return match route");

    let err = check_source(&source).expect_err("non-concrete return match should fail");

    assert!(err.to_string().contains(
        "process Worker step return match scrutinee route requires a discovered concrete message payload case"
    ));
}

#[test]
fn rejects_step_return_match_over_direct_state() {
    let source = STEP_RETURN_MATCH.replace("return match phase", "return match state");

    let err = check_source(&source).expect_err("direct state return match should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match scrutinee state must be a concrete enum source value binding"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_non_exhaustive_step_return_match() {
    let source = STEP_RETURN_MATCH.replace(
        r#"            Done => {
                return Stop(Done);
            }
"#,
        "",
    );

    let err = check_source(&source).expect_err("non-exhaustive step return match should fail");

    assert!(
        err.to_string()
            .contains("process Worker step return match must handle variant Done"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_overlapping_step_return_match_arm() {
    let source = STEP_RETURN_MATCH.replace(
        r#"            Done => {
                return Stop(Done);
            }
"#,
        r#"            Ready => {
                return Stop(Done);
            }
            Done => {
                return Stop(Done);
            }
"#,
    );

    let err = check_source(&source).expect_err("overlapping step return match should fail");

    assert!(
        err.to_string()
            .contains("process Worker step return match pattern Ready overlaps an earlier pattern"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unreachable_step_return_match_wildcard() {
    let source = STEP_RETURN_MATCH.replace(
        r#"            Done => {
                return Stop(Done);
            }
"#,
        r#"            Done => {
                return Stop(Done);
            }
            _ => {
                return Stop(Done);
            }
"#,
    );

    let err =
        check_source(&source).expect_err("unreachable step return match wildcard should fail");

    assert!(
        err.to_string()
            .contains("process Worker step return match wildcard pattern is unreachable"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_that_returns_bare_state() {
    let source = STEP_RETURN_MATCH.replace("return Continue(SawReady);", "return SawReady;");

    let err = check_source(&source).expect_err("bare state return-match arm should fail");

    assert!(
        err.to_string().contains(
            "step return match arm must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_statement() {
    let source = STEP_RETURN_MATCH
        .replace(
            "fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {",
            "fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit] ~ [] @det {",
        )
        .replace(
            "            Ready => {\n                return Continue(SawReady);",
            "            Ready => {\n                if (Ready == Ready) {\n                    emit \"return-match arm runtime if is unsupported\";\n                } else {\n                    emit \"return-match arm runtime if else is unsupported\";\n                }\n                return Continue(SawReady);",
        );

    let err = check_source(&source).expect_err("return-match arm runtime if should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match arm cannot perform runtime if in this source slice"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn checks_step_return_match_after_uniform_effect_prefix() {
    let source = r#"
module process_return_match_effect_prefix;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, SawReady, Done }
enum WorkerMsg { Envelope(Route) }
record SinkState;
enum SinkMsg { Tick }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit, spawn, send] ~ [] @det {
        emit "return-match uniform prefix";
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Tick;
        return match phase {
            Ready => {
                return Continue(SawReady);
            }
            Done => {
                return Stop(Done);
            }
        };
    }
}

proc Sink mailbox bounded(2) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Tick) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("uniform effect prefix should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert_eq!(worker.transitions().len(), 2);
    for transition in worker.transitions() {
        assert!(
            matches!(
                transition.payload_guard().map(|payload| payload.label()),
                Some("Assign(Ready)" | "Assign(Done)")
            ),
            "unexpected payload guard: {:?}",
            transition.payload_guard()
        );
        assert_eq!(
            transition.effects(),
            &[Effect::Emit, Effect::Spawn, Effect::Send]
        );
        assert_eq!(transition.actions().len(), 3);
        assert_eq!(
            transition.actions()[0],
            CheckedAction::Emit {
                output: checked_output_id(0)
            }
        );
        assert!(matches!(
            transition.actions()[1],
            CheckedAction::Spawn { .. }
        ));
        assert!(matches!(
            transition.actions()[2],
            CheckedAction::Send { .. }
        ));
    }

    lower_to_artifact(&checked, source)
        .expect("uniform effect prefix should lower onto each typed transition");
}
