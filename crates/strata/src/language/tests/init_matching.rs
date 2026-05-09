use super::support::*;

#[test]
fn rejects_unknown_init_match_arm() {
    let source = INIT_MATCH.replace("Cold =>", "Unknown =>");

    let err = check_source(&source).expect_err("unknown init match arm should fail");

    assert!(
        err.to_string()
            .contains("match pattern Unknown is not a variant of enum StartupMode")
    );
}

#[test]
fn rejects_duplicate_init_match_arm() {
    let source = INIT_MATCH.replace("Warm =>", "Cold =>");

    let err = check_source(&source).expect_err("duplicate init match arm should fail");

    assert!(
        err.to_string()
            .contains("process Main init match declares duplicate pattern for variant Cold")
    );
}

#[test]
fn rejects_missing_init_match_arm() {
    let source = INIT_MATCH.replace(
        r#"
            Warm => {
                return MainState { readiness: WarmReady };
            }"#,
        "",
    );

    let err = check_source(&source).expect_err("missing init match arm should fail");

    assert!(
        err.to_string()
            .contains("process Main init match must handle variant Warm")
    );
}

#[test]
fn rejects_unreachable_init_match_wildcard_arm() {
    let source = INIT_MATCH.replace(
        r#"
            Warm => {
                return MainState { readiness: WarmReady };
            }"#,
        r#"
            Warm => {
                return MainState { readiness: WarmReady };
            }
            _ => {
                return MainState { readiness: ColdReady };
            }"#,
    );

    let err = check_source(&source).expect_err("unreachable init match wildcard should fail");

    assert!(
        err.to_string()
            .contains("process Main init match wildcard pattern is unreachable")
    );
}

#[test]
fn rejects_init_match_binding_on_fieldless_variant() {
    let source = INIT_MATCH.replace("Cold =>", "Cold(mode: StartupMode) =>");

    let err = check_source(&source).expect_err("fieldless init match binding should fail");

    assert!(
        err.to_string()
            .contains("process Main init match pattern Cold does not carry a payload")
    );
}

#[test]
fn rejects_record_pattern_in_init_match_arm() {
    let source = INIT_MATCH.replace("Cold =>", "MainState { readiness } =>");

    let err = check_source(&source).expect_err("record init match arm should fail");

    assert!(err.to_string().contains(
        "process Main init match pattern MainState destructures a record, but this match expects enum constructors"
    ));
}

#[test]
fn rejects_return_match_expression_in_init_body() {
    let source = ACTOR_PING.replace(
        "return Idle;",
        r#"return match Idle {
            Idle => {
                return Idle;
            }
            Handled => {
                return Handled;
            }
        };"#,
    );

    let err = check_source(&source).expect_err("init return match should fail");

    assert!(err.to_string().contains(
        "process Worker init body return match is not supported in init in this source slice"
    ));
}

#[test]
fn checks_payload_bearing_enum_variants_in_init_match_when_binding_is_unused() {
    let source = r#"
module init_match_payload_variant;

record ModePayload;
enum StartupMode { Cold, Warm(ModePayload) }
enum MainState { ColdReady, WarmReady }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        match Cold {
            Cold => {
                return ColdReady;
            }
            Warm(payload: ModePayload) => {
                return WarmReady;
            }
        }
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("payload-bearing init match enum should check");
    let main = &checked.processes()[0];

    assert_eq!(checked_state_labels(main), ["ColdReady", "WarmReady"]);
    assert_eq!(main.init_state(), checked_state_id(0));
}

#[test]
fn rejects_init_match_payload_binding_in_returned_state() {
    let source = r#"
module init_match_payload_binding_return;

record ModePayload;
record MainState { payload: ModePayload }
enum StartupMode { Cold, Warm(ModePayload) }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        match Cold {
            Cold => {
                return MainState { payload: ModePayload };
            }
            Warm(payload: ModePayload) => {
                return MainState { payload: payload };
            }
        }
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("init match payload return should fail");

    assert!(err.to_string().contains(
        "process Main init match arm cannot use payload binding payload in returned state"
    ));
}

#[test]
fn rejects_assignment_style_record_values_in_init_match_arm() {
    let source = r#"
module init_match_assignment;

enum StartupMode { Cold, Warm }
record MainState { mode: StartupMode }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        match Warm {
            Cold => {
                return MainState { mode: Cold };
            }
            Warm => {
                return MainState { mode = Warm };
            }
        }
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = parse_source(source).expect_err("assignment in init match arm should fail");

    assert!(
        err.to_string()
            .contains("record value fields use ':'; assignment syntax is not supported")
    );
}

#[test]
fn rejects_trailing_statement_after_match_body() {
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
            }
        }
        return Stop(Handled);
    }"#,
    );

    let err = parse_source(&source).expect_err("trailing statement after match should fail");

    assert!(
        err.to_string()
            .contains("match body must be the whole function body in this source slice")
    );
}

#[test]
fn rejects_nested_match_body_syntax() {
    let source = ACTOR_PING.replace(
        r#"emit "worker handled Ping";
        return Stop(Handled);"#,
        r#"emit "before match";
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
        return Stop(Handled);"#,
    );

    let err = parse_source(&source).expect_err("nested match body should fail");

    assert!(
        err.to_string()
            .contains("match body must be the whole function body in this source slice")
    );
}
