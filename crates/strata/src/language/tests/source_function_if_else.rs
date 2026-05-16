use super::support::*;

const FUNCTION_IF_ELSE: &str = r#"
module source_function_if_else;

enum Bool { False, True }
enum Mode { Cold, Warm }
enum Readiness { ColdReady, WarmReady }
record MainState {
    init: Readiness,
    step: Readiness,
}
enum MainMsg { Start }

fn is_warm(mode: Mode) -> Bool ! [] ~ [] @det {
    return match mode {
        Cold => {
            return False;
        }
        Warm => {
            return True;
        }
    };
}

fn choose(flag: Bool) -> Readiness ! [] ~ [] @det {
    return if (flag) { WarmReady } else { ColdReady };
}

fn readiness(mode: Mode) -> Readiness ! [] ~ [] @det {
    return choose(is_warm(mode));
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { init: readiness(Warm), step: readiness(Cold) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { init: readiness(Cold), step: readiness(Warm) });
    }
}
"#;

#[test]
fn parses_checks_and_lowers_source_function_if_else() {
    let module = parse_source(FUNCTION_IF_ELSE).expect("if/else source should parse");
    let choose = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "choose")
        .expect("choose helper should parse");
    let Some(FunctionBody::Block(body)) = &choose.body else {
        panic!("choose should parse as a block body");
    };
    assert!(matches!(
        body.returns,
        ReturnExpr::Value(ValueExpr::IfElse { .. })
    ));

    let checked = check_module(module).expect("if/else source should check");
    let main = &checked.processes()[0];
    assert_eq!(
        checked_state_labels(main),
        [
            "MainState{init:WarmReady,step:ColdReady}",
            "MainState{init:ColdReady,step:WarmReady}"
        ]
    );

    let artifact =
        lower_to_artifact(&checked, FUNCTION_IF_ELSE).expect("if/else source should lower");
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        [
            "MainState{init:WarmReady,step:ColdReady}",
            "MainState{init:ColdReady,step:WarmReady}"
        ]
    );
    let encoded = artifact.encode();
    assert!(!encoded.contains("is_warm"));
    assert!(!encoded.contains("choose"));
    assert!(!encoded.contains("readiness"));
}

#[test]
fn rejects_if_else_without_declared_bool_contract() {
    let source = FUNCTION_IF_ELSE.replace("enum Bool { False, True }", "enum Bool { No, Yes }");

    let err = check_source(&source).expect_err("malformed Bool contract should fail");

    assert!(
        err.to_string()
            .contains("if condition requires enum Bool { False, True }")
    );
}

#[test]
fn rejects_if_else_reversed_bool_contract() {
    let source = FUNCTION_IF_ELSE.replace("enum Bool { False, True }", "enum Bool { True, False }");

    let err = check_source(&source).expect_err("reversed Bool contract should fail");

    assert!(
        err.to_string()
            .contains("if condition requires enum Bool { False, True }")
    );
}

#[test]
fn rejects_if_else_payload_bool_contract() {
    let source = FUNCTION_IF_ELSE.replace(
        "enum Bool { False, True }",
        "enum Bool { False, True(Mode) }",
    );

    let err = check_source(&source).expect_err("payload Bool contract should fail");

    assert!(
        err.to_string()
            .contains("if condition requires enum Bool { False, True }")
    );
}

#[test]
fn rejects_if_else_non_bool_condition() {
    let source = FUNCTION_IF_ELSE.replace("return if (flag)", "return if (Cold)");

    let err = check_source(&source).expect_err("non-Bool if condition should fail");

    assert!(err.to_string().contains("if condition must have type Bool"));
}

#[test]
fn rejects_if_else_branch_type_mismatch() {
    let source = FUNCTION_IF_ELSE.replace(
        "return if (flag) { WarmReady } else { ColdReady };",
        "return if (flag) { WarmReady } else { True };",
    );

    let err = check_source(&source).expect_err("mismatched if branch should fail");

    assert!(
        err.to_string()
            .contains("if else branch must produce Readiness")
    );
}

#[test]
fn rejects_if_else_runtime_bound_condition() {
    let source = r#"
module source_function_if_else_runtime_condition;

record MainState;
enum Bool { False, True }
enum MainMsg { Boot, Start(Bool) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Boot) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, Start(flag: Bool)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(if (flag) { MainState } else { MainState });
    }
}
"#;

    let err = check_source(source).expect_err("runtime-bound condition should fail closed");
    let message = err.to_string();

    assert!(
        message.contains("if condition requires a concrete Bool value"),
        "{message}"
    );
}

#[test]
fn rejects_direct_if_else_non_bool_condition() {
    let source = r#"
module source_function_if_else_direct_non_bool;

record MainState;
enum Bool { False, True }
enum Mode { Cold, Warm }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return if (Cold) { MainState } else { MainState };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState);
    }
}
"#;

    let err = check_source(source).expect_err("direct non-Bool condition should fail");
    let message = err.to_string();

    assert!(
        message.contains("if condition must have type Bool"),
        "{message}"
    );
}

#[test]
fn rejects_direct_if_else_branch_type_mismatch() {
    let source = r#"
module source_function_if_else_direct_branch_mismatch;

record MainState;
enum Bool { False, True }
enum Other { ElseBad }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return if (True) { MainState } else { ElseBad };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState);
    }
}
"#;

    let err = check_source(source).expect_err("direct mismatched branch should fail");
    let message = err.to_string();

    assert!(
        message.contains("if else branch must produce MainState"),
        "{message}"
    );
}

#[test]
fn rejects_if_else_branch_statements() {
    let source = FUNCTION_IF_ELSE.replace(
        "return if (flag) { WarmReady } else { ColdReady };",
        r#"return if (flag) { emit "branch effect"; WarmReady } else { ColdReady };"#,
    );

    let err = parse_source(&source).expect_err("if branch statements should fail to parse");

    assert!(
        err.to_string()
            .contains("if branches are pure value expressions and must not perform statements")
    );
}

#[test]
fn rejects_if_else_branch_statements_after_value() {
    let source = FUNCTION_IF_ELSE.replace(
        "return if (flag) { WarmReady } else { ColdReady };",
        r#"return if (flag) { WarmReady; emit "branch effect"; } else { ColdReady };"#,
    );

    let err = parse_source(&source).expect_err("if branch statements should fail to parse");

    assert!(
        err.to_string()
            .contains("if branches are pure value expressions and must not perform statements")
    );
}

#[test]
fn rejects_source_function_call_cycle_through_if_else() {
    let source = FUNCTION_IF_ELSE.replace(
        "return if (flag) { WarmReady } else { ColdReady };",
        "return if (flag) { choose(False) } else { ColdReady };",
    );

    let err = check_source(&source).expect_err("recursive if branch should fail");

    assert!(
        err.to_string()
            .contains("module source function call cycle choose -> choose is not supported")
    );
}
