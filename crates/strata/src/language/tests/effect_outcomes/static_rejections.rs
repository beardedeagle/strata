use super::super::support::*;
use super::EFFECT_OUTCOMES;
use std::collections::BTreeSet;
use std::fmt::Write as _;

#[test]
fn rejects_direct_spawn_outcome_equality_between_process_ref_successes() {
    let source = r#"
module spawn_outcome_direct_equality;

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
        if (spawned == spawned) {
            emit "same";
        } else {
            emit "different";
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

    let err = check_source(source).expect_err("direct spawn outcome equality should fail");

    assert!(
        err.to_string()
            .contains("process-reference equality is not supported"),
        "{err}"
    );
}

#[test]
fn rejects_payload_structural_equality_inside_send_error_pattern() {
    let source = r#"
module outcome_payload_structural_equality;

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
        if (sent == Err(Full(Work(Job { phase: Ready })))) {
            emit "same payload";
        } else {
            emit "different payload";
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

    let err = check_source(source).expect_err("payload structural equality should fail");

    assert!(
        err.to_string().contains("record equality is not supported")
            || err
                .to_string()
                .contains("equality type WorkerMsg must not declare payload-bearing enum variants"),
        "{err}"
    );
}

#[test]
fn rejects_send_outcome_annotation_that_does_not_preserve_target_message_type() {
    let source = EFFECT_OUTCOMES.replace(
        "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;",
        "let sent: Result<Unit,SendError<Unit>> = send worker Ping;",
    );
    let err = check_source(&source).expect_err("wrong send outcome type should fail");

    assert!(
        err.to_string()
            .contains("send outcome binding must have type Result<Unit,SendError<WorkerMsg>>"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_outcome_annotation_without_process_ref_success_shape() {
    let source = EFFECT_OUTCOMES.replace(
        "let spawned: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;",
        "let spawned: Result<Unit,SpawnError<Unit>> = spawn Worker;",
    );
    let err = check_source(&source).expect_err("wrong spawn outcome type should fail");

    assert!(
        err.to_string().contains(
            "spawn outcome binding must have type Result<ProcessRef<Worker>,SpawnError<Unit>>"
        ),
        "{err}"
    );
}

#[test]
fn rejects_effect_outcome_binding_that_conflicts_with_process_reference() {
    let source = EFFECT_OUTCOMES.replace(
        "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: sent });",
        "let worker: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: worker });",
    );
    let err = check_source(&source).expect_err("process ref collision should fail");

    assert!(
        err.to_string()
            .contains("effect outcome binding worker conflicts with a process reference binding"),
        "{err}"
    );
}

#[test]
fn rejects_effect_outcome_binding_that_conflicts_with_step_state_parameter() {
    let source = EFFECT_OUTCOMES.replace(
        "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: sent });",
        "let state: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: state });",
    );
    let err = check_source(&source).expect_err("state parameter collision should fail");

    assert!(
        err.to_string()
            .contains("effect outcome binding state conflicts with the step state parameter"),
        "{err}"
    );
}

#[test]
fn rejects_effect_outcome_binding_that_conflicts_with_declared_value_constructor() {
    for name in ["Unit", "Ok", "Err", "Full"] {
        let source = EFFECT_OUTCOMES.replace(
            "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: sent });",
            &format!(
                "let {name}: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState {{ outcome: {name} }});"
            ),
        );
        let err = check_source(&source).expect_err("constructor collision should fail");
        let expected = format!(
            "effect outcome binding {name} conflicts with a declared type or value constructor"
        );

        assert!(
            err.to_string().contains(&expected),
            "expected `{expected}` in `{err}`"
        );
    }
}

#[test]
fn rejects_effect_outcome_binding_that_conflicts_with_source_function() {
    let source = EFFECT_OUTCOMES
        .replace(
            "proc Main mailbox bounded(1) {",
            "fn sent(state: MainState) -> MainState ! [] ~ [] @det {\n    return state;\n}\n\nproc Main mailbox bounded(1) {",
        );
    let err = check_source(&source).expect_err("source function collision should fail");

    assert!(
        err.to_string()
            .contains("effect outcome binding sent conflicts with a source function declaration"),
        "{err}"
    );
}

#[test]
fn rejects_effect_outcome_binding_after_ordinary_effect_statement() {
    let source = EFFECT_OUTCOMES.replace(
        "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;",
        "send worker Ping;\n        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;",
    );
    let err = check_source(&source).expect_err("outcome after ordinary send should fail");

    assert!(
        err.to_string().contains(
            "effect outcome binding sent must appear before ordinary effect statements in the step body"
        ),
        "{err}"
    );
}

#[test]
fn rejects_effect_outcome_use_before_binding() {
    let source = EFFECT_OUTCOMES
        .replace("! [spawn, send]", "! [spawn, emit, send]")
        .replace(
            "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;",
            "if (sent == sent) {\n            emit \"future outcome\";\n        } else {\n        }\n        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;",
        );
    let err = check_source(&source).expect_err("future outcome use should fail");

    assert!(
        err.to_string()
            .contains("effect outcome binding sent is used before it is bound"),
        "{err}"
    );
}

#[test]
fn rejects_more_than_max_effect_outcome_bindings_in_one_step() {
    let mut outcomes = String::new();
    for index in 0..=MAX_EFFECT_OUTCOMES_PER_TRANSITION {
        writeln!(
            outcomes,
            "        let sent{index}: Result<Unit,SendError<WorkerMsg>> = send worker Ping;"
        )
        .expect("writing to String should not fail");
    }
    let source = EFFECT_OUTCOMES.replace(
        "let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;\n        return Stop(MainState { outcome: sent });",
        &format!("{outcomes}        return Stop(state);"),
    );
    let err = check_source(&source).expect_err("too many effect outcomes should fail");
    let expected = format!(
        "process Main step binds more than {MAX_EFFECT_OUTCOMES_PER_TRANSITION} effect outcomes"
    );

    assert!(
        err.to_string().contains(&expected),
        "expected `{expected}` in `{err}`"
    );
}

#[test]
fn rejects_outcome_branch_over_wrong_error_variant() {
    let source = EFFECT_OUTCOMES
        .replace("! [spawn, send]", "! [emit, spawn, send]")
        .replace(
            "return Stop(MainState { outcome: sent });",
            "if (sent == Err(Exhausted(Unit))) {\n            emit \"wrong\";\n        } else {\n            emit \"right\";\n        }\n        return Stop(MainState { outcome: sent });",
        );
    let err = check_source(&source).expect_err("wrong outcome branch variant should fail");

    assert!(
        err.to_string()
            .contains("value Exhausted is not a variant of enum SendError"),
        "{err}"
    );
}

#[test]
fn bounded_effect_outcome_state_admission_matches_commit_or_return_model() {
    let checked = check_source(include_str!(
        "../../../../../../examples/effect_outcomes.str"
    ))
    .expect("effect outcome example should check");
    let main = checked
        .processes()
        .get(checked.entry_process().index())
        .expect("Main process should exist");
    let labels = main
        .state_values()
        .iter()
        .map(|state| state.label())
        .collect::<BTreeSet<_>>();

    assert_eq!(main.state_values().len(), 5);
    assert!(labels.contains("MainState{sent:Ok(Unit)}"));
    assert!(labels.contains("MainState{sent:Err(Full(Work))}"));
    assert!(labels.contains("MainState{sent:Err(Stopped(Work))}"));
    assert!(labels.contains("MainState{sent:Err(Crashed(Work))}"));
    assert!(labels.contains("MainState{sent:Err(MailboxClosed(Work))}"));
}

#[test]
fn rejects_process_ref_send_outcome_as_state_authority() {
    let source = r#"
module process_ref_send_outcome_state;

record MainState { sent: Result<Unit,SendError<WorkerMsg>> }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Forward(ProcessRef<Worker>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { sent: Ok(Unit) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Forward(worker);
        return Stop(MainState { sent: sent });
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
    let err = check_source(source).expect_err("process-ref outcome state should fail");

    assert!(
        err.to_string().contains("contains a process reference")
            || err
                .to_string()
                .contains("process references must be direct message payloads"),
        "{err}"
    );
}

#[test]
fn rejects_spawn_outcome_as_state_because_success_carries_process_ref() {
    let source = r#"
module spawn_outcome_state_ref;

record MainState { spawned: Result<ProcessRef<Worker>,SpawnError<Unit>> }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Work }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { spawned: Err(Exhausted(Unit)) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let spawned: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
        return Stop(MainState { spawned: spawned });
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
    let err = check_source(source).expect_err("spawn outcome state authority should fail");

    assert!(
        err.to_string().contains(
            "field spawned type Result<ProcessRef<Worker>,SpawnError<Unit>> contains a process reference"
        ),
        "{err}"
    );
}

#[test]
fn rejects_outcome_next_state_when_payload_space_is_not_finitely_admitted() {
    let source = r#"
module effect_outcome_nonfinite_state;

record MainState { outcome: Result<Unit,SendError<WorkerMsg>> }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Work(U32) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { outcome: Err(Full(Work(1_u32))) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Work(1_u32);
        return Stop(MainState { outcome: sent });
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Work(weight: U32)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let err = check_source(source).expect_err("non-finite outcome state should fail");

    assert!(
        err.to_string().contains(
            "effect outcome binding sent cannot be used as a next-state value because type Result<Unit,SendError<WorkerMsg>> has non-finite payload values"
        ),
        "{err}"
    );
}

#[test]
fn rejects_cumulative_effect_outcome_state_expansion_past_process_limit() {
    let source = r#"
module effect_outcome_expansion_limit;

record MainState {
    a: Result<Unit,SendError<WorkerMsg>>,
    b: Result<Unit,SendError<WorkerMsg>>,
    c: Result<Unit,SendError<WorkerMsg>>,
    d: Result<Unit,SendError<WorkerMsg>>,
    e: Result<Unit,SendError<WorkerMsg>>,
    f: Result<Unit,SendError<WorkerMsg>>,
}
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Work }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            a: Err(Full(Work)),
            b: Err(Full(Work)),
            c: Err(Full(Work)),
            d: Err(Full(Work)),
            e: Err(Full(Work)),
            f: Err(Full(Work)),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let a: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let b: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let c: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let d: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let e: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        let f: Result<Unit,SendError<WorkerMsg>> = send worker Work;
        return Stop(MainState { a: a, b: b, c: c, d: d, e: e, f: f });
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
    let err = check_source(source).expect_err("cumulative outcome expansion should fail");

    assert!(
        err.to_string()
            .contains("exceeding maximum state_value_count"),
        "{err}"
    );
}
