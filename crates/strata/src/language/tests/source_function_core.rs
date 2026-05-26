use super::support::*;

#[test]
fn parses_checks_and_lowers_source_functions_with_pattern_matching() {
    let module = parse_source(FUNCTION_MATCH).expect("function match source should parse");
    assert_eq!(module.functions.len(), 4);
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|function| function.name.as_str() == "readiness_sig")
            .count(),
        2
    );
    let main = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Main")
        .expect("Main should parse");
    assert_eq!(main.functions.len(), 1);
    let worker = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Worker")
        .expect("Worker should parse");
    assert_eq!(worker.functions.len(), 1);

    let checked = check_module(module).expect("function match source should check");
    let main = &checked.processes()[0];
    let worker = &checked.processes()[1];
    assert_eq!(
        checked_state_labels(main),
        ["MainState{signature:WarmReady,body:WarmReady}"]
    );
    assert_eq!(
        checked_state_labels(worker),
        [
            "WorkerState{job:Job{phase:Done}}",
            "WorkerState{job:Job{phase:Ready}}"
        ]
    );
    assert!(matches!(
        only_transition(worker).next_state(),
        CheckedNextState::Template(_)
    ));

    let artifact =
        lower_to_artifact(&checked, FUNCTION_MATCH).expect("function match should lower");
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{signature:WarmReady,body:WarmReady}"]
    );
    assert_eq!(
        artifact_state_labels(&artifact.processes[1]),
        [
            "WorkerState{job:Job{phase:Done}}",
            "WorkerState{job:Job{phase:Ready}}"
        ]
    );
    let encoded = artifact.encode();
    assert!(!encoded.contains("readiness_sig"));
    assert!(!encoded.contains("readiness_body"));
    assert!(!encoded.contains("ready_job"));
    assert!(!encoded.contains("state_for"));
    assert!(!encoded.contains("with_job"));
}

#[test]
fn parses_checks_and_lowers_source_functions_with_payload_matching() {
    let module =
        parse_source(FUNCTION_PAYLOAD_MATCH).expect("function payload match source should parse");
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|function| function.name.as_str() == "status_sig")
            .count(),
        2
    );

    let checked = check_module(module).expect("function payload match source should check");
    let main = &checked.processes()[0];
    let worker = &checked.processes()[1];
    assert_eq!(
        checked_state_labels(main),
        ["MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}"]
    );
    assert_eq!(
        checked_state_labels(worker),
        [
            "WorkerState{work:Empty}",
            "WorkerState{work:Assigned(Job{phase:Ready})}"
        ]
    );
    assert!(matches!(
        only_transition(worker).next_state(),
        CheckedNextState::Template(_)
    ));

    let artifact = lower_to_artifact(&checked, FUNCTION_PAYLOAD_MATCH)
        .expect("function payload match should lower");
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}"]
    );
    assert_eq!(
        artifact_state_labels(&artifact.processes[1]),
        [
            "WorkerState{work:Empty}",
            "WorkerState{work:Assigned(Job{phase:Ready})}"
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains("kind=enum_variant"));
    assert!(encoded.contains("variant_id=1"));
    assert!(!encoded.contains("variant=Assigned"));
    assert!(!encoded.contains("status_sig"));
    assert!(!encoded.contains("status_body"));
    assert!(!encoded.contains("state_for"));
}

#[test]
fn rejects_unknown_source_function_call() {
    let source = FUNCTION_MATCH.replace("state_for(Warm)", "missing(Warm)");

    let err = check_source(&source).expect_err("unknown source function should fail");

    assert!(err.to_string().contains("function missing is not declared"));
}

#[test]
fn rejects_unknown_source_function_call_in_unused_function() {
    let source = FUNCTION_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn unused(mode: StartupMode) -> Readiness ! [] ~ [] @det {
    return missing(mode);
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source).expect_err("unused invalid source function should fail");

    assert!(err.to_string().contains("function missing is not declared"));
}

#[test]
fn rejects_module_source_function_call_to_process_local_function() {
    let source = FUNCTION_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn invalid_module_function(mode: StartupMode) -> MainState ! [] ~ [] @det {
    return state_for(mode);
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source)
        .expect_err("module source function should not see process-local functions");

    assert!(
        err.to_string()
            .contains("function state_for is not declared")
    );
}

#[test]
fn rejects_source_function_undeclared_parameter_type() {
    let source = FUNCTION_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn unused(input: Missing) -> Readiness ! [] ~ [] @det {
    return ColdReady;
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source).expect_err("undeclared function parameter type should fail");

    assert!(err.to_string().contains(
        "module function unused parameter input must use a declared record, enum, scalar, list, or map type without process-reference authority, found Missing"
    ));
}

#[test]
fn rejects_source_function_undeclared_return_type() {
    let source = FUNCTION_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn unused(mode: StartupMode) -> Missing ! [] ~ [] @det {
    return mode;
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source).expect_err("undeclared function return type should fail");

    assert!(err.to_string().contains(
        "module function unused return type must use a declared record, enum, scalar, list, or map type without process-reference authority, found Missing"
    ));
}

#[test]
fn rejects_unused_module_source_function_call_cycle() {
    let source = FUNCTION_MATCH.replace(
        "proc Main mailbox bounded(1) {",
        r#"fn loop_readiness(mode: StartupMode) -> Readiness ! [] ~ [] @det {
    return loop_readiness(mode);
}

proc Main mailbox bounded(1) {"#,
    );

    let err = check_source(&source).expect_err("recursive module source function should fail");

    assert!(err.to_string().contains(
        "module source function call cycle loop_readiness -> loop_readiness is not supported"
    ));
}

#[test]
fn rejects_unused_process_source_function_call_cycle() {
    let source = FUNCTION_MATCH.replace(
        "    fn init() -> MainState ! [] ~ [] @det {",
        r#"    fn local_loop(mode: StartupMode) -> Readiness ! [] ~ [] @det {
        return local_loop(mode);
    }

    fn init() -> MainState ! [] ~ [] @det {"#,
    );

    let err = check_source(&source).expect_err("recursive process source function should fail");

    assert!(err.to_string().contains(
        "process Main source function call cycle local_loop -> local_loop is not supported"
    ));
}

#[test]
fn rejects_source_function_call_with_wrong_argument_type() {
    let source = FUNCTION_MATCH.replace("state_for(Warm)", "state_for(WarmReady)");

    let err = check_source(&source).expect_err("wrong function argument type should fail");

    assert!(
        err.to_string()
            .contains("value WarmReady is not a variant of enum StartupMode")
    );
}

#[test]
fn rejects_source_function_statements() {
    let source = FUNCTION_MATCH.replace(
        "return WorkerState { job: job };",
        "emit \"function tried to mutate behavior\";\n        return WorkerState { job: job };",
    );

    let err = check_source(&source).expect_err("source function statement should fail");

    assert!(
        err.to_string()
            .contains("process Worker function with_job must not perform statements")
    );
}
