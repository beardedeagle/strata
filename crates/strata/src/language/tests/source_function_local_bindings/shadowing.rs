use super::*;

#[test]
fn rejects_source_local_binding_shadowing_parameter() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "let current_local: Phase = status(work);",
        "let work: Work = Work { phase: Active };",
    );

    let err = check_source(&source).expect_err("parameter shadowing should fail");

    assert!(
        err.to_string()
            .contains("source-local binding work conflicts with an existing source value binding")
    );
}

#[test]
fn rejects_source_local_binding_shadowing_match_pattern_binding() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "let echo_local: Phase = echo_source_local;",
        "let echo_source_local: Phase = Active;",
    );

    let err = check_source(&source).expect_err("pattern binding shadowing should fail");

    assert!(err.to_string().contains(
        "source-local binding echo_source_local conflicts with an existing source value binding"
    ));
}

#[test]
fn rejects_source_local_binding_declared_name_conflicts() {
    for (needle, replacement, expected) in [
        (
            "let current_local: Phase = status(work);",
            "let Active: Phase = Active;",
            "source-local binding Active conflicts with a declared type or value constructor",
        ),
        (
            "let current_local: Phase = status(work);",
            "let Main: Phase = Active;",
            "source-local binding Main conflicts with a process declaration",
        ),
        (
            "let current_local: Phase = status(work);",
            "let route: Phase = Active;",
            "source-local binding route conflicts with a source function declaration",
        ),
    ] {
        let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(needle, replacement);
        let err = check_source(&source).expect_err("declared name conflict should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_source_function_binding_parameter_declared_name_conflicts() {
    for (needle, replacement, expected) in [
        (
            "fn route(work: Work) -> Route ! [] ~ [] @det {",
            "fn route(Active: Work) -> Route ! [] ~ [] @det {",
            "source function parameter Active conflicts with a declared type or value constructor",
        ),
        (
            "fn route(work: Work) -> Route ! [] ~ [] @det {",
            "fn route(Main: Work) -> Route ! [] ~ [] @det {",
            "source function parameter Main conflicts with a process declaration",
        ),
        (
            "fn route(work: Work) -> Route ! [] ~ [] @det {",
            "fn route(route: Work) -> Route ! [] ~ [] @det {",
            "source function parameter route conflicts with a source function declaration",
        ),
    ] {
        let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(needle, replacement);
        let err = check_source(&source).expect_err("parameter name conflict should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_source_function_binding_parameter_shadowing_process_ref_binding() {
    let source = r#"
module source_parameter_process_ref_shadow;

enum Phase { Idle, Active }
record MainState { selected: Phase }
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn route(worker: Phase) -> Phase ! [] ~ [] @det {
        return worker;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        return Stop(MainState { selected: route(Active) });
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("process ref parameter shadowing should fail");

    assert!(
        err.to_string().contains(
            "process Main function route source function parameter worker conflicts with a process reference binding"
        ),
        "{err}"
    );
}

#[test]
fn rejects_source_local_binding_shadowing_process_ref_binding() {
    let source = r#"
module source_local_binding_process_ref_shadow;

enum Phase { Idle, Active }
record MainState { selected: Phase }
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn route(phase: Phase) -> Phase ! [] ~ [] @det {
        let worker: Phase = phase;
        return worker;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        return Stop(MainState { selected: route(Active) });
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("process ref shadowing should fail");

    assert!(
        err.to_string().contains(
            "process Main function route source-local binding worker conflicts with a process reference binding"
        ),
        "{err}"
    );
}

#[test]
fn rejects_source_pattern_binding_shadowing_process_ref_binding() {
    let source = r#"
module source_pattern_binding_process_ref_shadow;

enum Phase { Idle, Active }
record MainState { selected: Phase }
record WorkerState;
record Boxed { worker: Phase }
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn route(Boxed { worker }) -> Phase ! [] ~ [] @det {
        return worker;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        return Stop(MainState { selected: route(Boxed { worker: Active }) });
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("process ref pattern shadowing should fail");

    assert!(
        err.to_string().contains(
            "process Main function route record pattern binding worker conflicts with a process reference binding"
        ),
        "{err}"
    );
}

#[test]
fn rejects_source_pattern_binding_shadowing_source_function_declaration() {
    let source = SOURCE_FUNCTION_LOCAL_BINDINGS.replace(
        "Route { selected: echo_source_local }",
        "Route { selected: echo_route }",
    );

    let err = check_source(&source).expect_err("source function pattern shadowing should fail");

    assert!(
        err.to_string().contains(
            "function echo_route return match record pattern binding echo_route conflicts with a source function declaration"
        ),
        "{err}"
    );
}
