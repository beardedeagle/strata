use super::support::*;

#[test]
fn rejects_emit_without_declared_effect() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("undeclared emit should be rejected");
    assert!(
        err.to_string()
            .contains("step uses effect emit but does not declare it")
    );
}

#[test]
fn rejects_spawn_without_declared_effect() {
    let source = ACTOR_PING.replace("! [spawn, send]", "! [send]");

    let err = check_source(&source).expect_err("undeclared spawn should be rejected");

    assert!(
        err.to_string()
            .contains("step uses effect spawn but does not declare it")
    );
}

#[test]
fn rejects_send_without_declared_effect() {
    let source = ACTOR_PING.replace("! [spawn, send]", "! [spawn]");

    let err = check_source(&source).expect_err("undeclared send should be rejected");

    assert!(
        err.to_string()
            .contains("step uses effect send but does not declare it")
    );
}

#[test]
fn rejects_unused_declared_effect() {
    let source = HELLO.replace("! [emit]", "! [emit, send]");

    let err = check_source(&source).expect_err("unused declared effect should be rejected");

    assert!(
        err.to_string()
            .contains("step declares effect send but does not use it")
    );
}

#[test]
fn rejects_duplicate_declared_effect() {
    let source = HELLO.replace("! [emit]", "! [emit, emit]");

    let err = check_source(&source).expect_err("duplicate declared effect should be rejected");

    assert!(
        err.to_string()
            .contains("step declares duplicate effect emit")
    );
}

#[test]
fn rejects_unknown_effect_name() {
    let source = HELLO.replace("! [emit]", "! [write]");

    let err = parse_source(&source).expect_err("unknown effect should fail");

    assert!(err.to_string().contains("unsupported effect write"));
}
