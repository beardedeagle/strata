use super::*;

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
