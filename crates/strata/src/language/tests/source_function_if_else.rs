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

fn choose_block(flag: Bool) -> Readiness ! [] ~ [] @det {
    if (flag) {
        return WarmReady;
    } else {
        return ColdReady;
    }
}

fn readiness(mode: Mode) -> Readiness ! [] ~ [] @det {
    return choose(is_warm(mode));
}

fn readiness_block(mode: Mode) -> Readiness ! [] ~ [] @det {
    return choose_block(is_warm(mode));
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { init: readiness_block(Warm), step: readiness(Cold) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { init: readiness(Cold), step: readiness_block(Warm) });
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
        .expect("choose function should parse");
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
    assert!(!encoded.contains("choose_block"));
    assert!(!encoded.contains("readiness"));
    assert!(!encoded.contains("readiness_block"));
}

#[test]
fn parses_checks_and_lowers_source_function_braced_return_if_else() {
    let module = parse_source(FUNCTION_IF_ELSE).expect("if/else source should parse");
    let choose_block = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "choose_block")
        .expect("choose_block function should parse");
    let Some(FunctionBody::Block(body)) = &choose_block.body else {
        panic!("choose_block should parse as a block body");
    };
    assert!(matches!(body.returns, ReturnExpr::IfElse { .. }));

    let checked = check_module(module).expect("braced source return-if should check");
    let artifact =
        lower_to_artifact(&checked, FUNCTION_IF_ELSE).expect("braced return-if should lower");
    let encoded = artifact.encode();
    assert!(!encoded.contains("choose_block"));
    assert!(!encoded.contains("readiness_block"));
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        [
            "MainState{init:WarmReady,step:ColdReady}",
            "MainState{init:ColdReady,step:WarmReady}"
        ]
    );
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
fn lowers_if_else_runtime_bound_condition_as_typed_next_state() {
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

    let checked = check_source(source).expect("runtime-bound value if should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("runtime-bound value if should lower");
    let transition = artifact.processes[0]
        .transitions
        .iter()
        .find(|transition| transition.message.as_u32() == 1)
        .expect("Start transition should lower");

    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::ReceivedPayload { .. },
            then_state,
            else_state,
        } if matches!(then_state.as_ref(), NextState::Value(_))
            && matches!(else_state.as_ref(), NextState::Value(_))
    ));
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
fn rejects_braced_return_if_branch_statements_in_source_function() {
    let source = FUNCTION_IF_ELSE.replace(
        "return WarmReady;",
        "emit \"branch effect\";\n        return WarmReady;",
    );

    let err = check_source(&source).expect_err("braced return-if statements should fail");

    assert!(err.to_string().contains(
        "source function choose_block return-if then branch must not perform statements"
    ));
}

#[test]
fn rejects_braced_return_if_missing_branch_return_in_source_function() {
    let source = FUNCTION_IF_ELSE.replace(
        "        return ColdReady;",
        "        emit \"missing return\";",
    );

    let err =
        parse_source(&source).expect_err("braced source-function return-if branch must return");

    assert!(
        err.to_string()
            .contains("return-if else branch must contain a top-level return"),
        "{err}"
    );
}

#[test]
fn rejects_braced_return_if_else_branch_type_mismatch() {
    let source = FUNCTION_IF_ELSE.replace("return ColdReady;", "return True;");

    let err = check_source(&source).expect_err("mismatched braced return-if branch should fail");

    assert!(
        err.to_string()
            .contains("if else branch must produce Readiness"),
        "{err}"
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

#[test]
fn property_generated_braced_return_if_selects_concrete_source_branch() {
    for (flag, expected) in [("False", "ColdReady"), ("True", "WarmReady")] {
        let source = FUNCTION_IF_ELSE.replace(
            "return MainState { init: readiness_block(Warm), step: readiness(Cold) };",
            &format!("return MainState {{ init: choose_block({flag}), step: readiness(Cold) }};"),
        );

        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("generated braced return-if source should check: {err}"));
        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated braced return-if source should lower: {err}"));
        assert!(
            artifact_state_labels(&artifact.processes[0])[0].contains(expected),
            "generated braced return-if should select {expected}"
        );
        assert!(
            !artifact.encode().contains("choose_block"),
            "function name must not lower into artifact dispatch"
        );
    }
}
