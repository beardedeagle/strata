use super::support::*;

#[test]
fn rejects_unknown_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        "fn step(state: WorkerState, Second)",
        "fn step(state: WorkerState, Unknown)",
    );

    let err = check_source(&source).expect_err("unknown step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker step pattern message Unknown is not accepted")
    );
}

#[test]
fn rejects_missing_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
"#,
        "",
    );

    let err = check_source(&source).expect_err("missing step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker must declare step pattern for message Second")
    );
}

#[test]
fn rejects_duplicate_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        "fn step(state: WorkerState, Second)",
        "fn step(state: WorkerState, First)",
    );

    let err = check_source(&source).expect_err("duplicate step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate step pattern for message First")
    );
}

#[test]
fn rejects_duplicate_wildcard_step_pattern() {
    let source = ACTOR_SEQUENCE
        .replace(
            "fn step(state: WorkerState, First)",
            "fn step(state: WorkerState, _)",
        )
        .replace(
            "fn step(state: WorkerState, Second)",
            "fn step(state: WorkerState, _)",
        );

    let err = check_source(&source).expect_err("duplicate wildcard pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate wildcard step pattern")
    );
}

#[test]
fn rejects_unreachable_wildcard_step_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
"#,
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
"#,
    );

    let err = check_source(&source).expect_err("unreachable wildcard pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker wildcard step pattern is unreachable")
    );
}

#[test]
fn rejects_typed_msg_step_parameter() {
    let source = ACTOR_PING.replace(
        "fn step(state: WorkerState, Ping)",
        "fn step(state: WorkerState, msg: WorkerMsg)",
    );

    let err = check_source(&source).expect_err("typed message parameter should fail");

    assert!(err.to_string().contains(
        "step second parameter must be a message constructor pattern or wildcard pattern"
    ));
}

#[test]
fn rejects_match_with_wrong_target() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match state {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("wrong match scrutinee should fail");

    assert!(
        err.to_string()
            .contains("state match step second parameter must be a message constructor pattern or wildcard pattern")
    );
}

#[test]
fn rejects_match_with_wrong_message_parameter_type() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: MainMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("wrong message parameter type should fail");

    assert!(
        err.to_string()
            .contains("process Worker message parameter msg has type MainMsg, expected WorkerMsg")
    );
}

#[test]
fn rejects_missing_match_arm() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled First";
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            First => {
                emit "worker handled First";
                return Continue(SawFirst);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("missing match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker must declare step pattern for message Second")
    );
}

#[test]
fn rejects_duplicate_match_arm() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled First";
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            First => {
                emit "worker handled First";
                return Continue(SawFirst);
            }
            First => {
                emit "worker handled First again";
                return Stop(Done);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("duplicate match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate step pattern for message First")
    );
}

#[test]
fn rejects_unknown_match_arm() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Unknown => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("unknown match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker step pattern message Unknown is not accepted")
    );
}

#[test]
fn rejects_record_pattern_in_step_match_arm() {
    let source = r#"
module step_record_pattern_rejection;

enum Phase {
    Ready,
}
record MainState {
    phase: Phase,
}
enum MainMsg {
    Start,
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Ready };
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            MainState { phase } => {
                return Stop(state);
            }
        }
    }
}
"#;

    let err = check_source(source).expect_err("record step match arm should fail");

    assert!(err.to_string().contains(
        "process Main step pattern MainState destructures a record, but step patterns expect message constructors"
    ));
}

#[test]
fn rejects_mixed_parameter_pattern_and_match_dispatch() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Second => {
                emit "worker handled Second";
                return Stop(Done);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("mixed step dispatch should fail");

    assert!(
        err.to_string()
            .contains("process Worker cannot mix match step bodies with step parameter patterns")
    );
}

#[test]
fn rejects_step_pattern_invalid_next_state() {
    let source = ACTOR_SEQUENCE.replace("Continue(SawFirst)", "Continue(UnknownState)");

    let err = check_source(&source).expect_err("invalid next state should fail");

    assert!(
        err.to_string()
            .contains("value UnknownState is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_match_arm_comma_separator() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            },
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("comma-separated match arms should fail");

    assert!(
        err.to_string()
            .contains("match arms are block-delimited and must not use comma separators")
    );
}

#[test]
fn rejects_match_arm_split_fat_arrow() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping = > {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("split match arm arrow should fail");

    assert!(err.to_string().contains("expected =>"));
}
