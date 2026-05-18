use super::support::*;

const SOURCE_EQUALITY: &str = r#"
module source_value_equality;

enum Bool { False, True }
enum Mode { Cold, Warm }
record MainState {
    bool_eq: Bool,
    enum_eq: Bool,
    enum_ne: Bool,
}
enum MainMsg { Start }

fn bool_eq(flag: Bool) -> Bool ! [] ~ [] @det {
    return True == True;
}

fn enum_eq(mode: Mode) -> Bool ! [] ~ [] @det {
    return Warm == Warm;
}

fn enum_ne(mode: Mode) -> Bool ! [] ~ [] @det {
    return Cold != Warm;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { bool_eq: True == True, enum_eq: Warm == Warm, enum_ne: Cold != Warm };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[test]
fn folds_concrete_bool_and_fieldless_enum_equality() {
    let module = parse_source(SOURCE_EQUALITY).expect("source equality should parse");
    let bool_eq = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "bool_eq")
        .expect("bool_eq helper should parse");
    let Some(FunctionBody::Block(body)) = &bool_eq.body else {
        panic!("bool_eq should parse as a block body");
    };
    assert!(matches!(
        body.returns,
        ReturnExpr::Value(ValueExpr::Equality { .. })
    ));

    let checked = check_module(module).expect("source equality should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{bool_eq:True,enum_eq:True,enum_ne:True}"]
    );

    let artifact =
        lower_to_artifact(&checked, SOURCE_EQUALITY).expect("source equality should lower");
    assert_eq!(
        artifact_state_labels(&artifact.processes[0]),
        ["MainState{bool_eq:True,enum_eq:True,enum_ne:True}"]
    );
    assert!(
        !artifact.encode().contains(".kind=equality"),
        "fully concrete source equality should fold before lowering"
    );
}

#[test]
fn folds_concrete_equality_before_static_map_key_validation() {
    let source = r#"
module source_value_equality_map_key;

enum Bool { False, True }
enum Mode { Cold, Warm }
record MainState { modes: Map<Bool,Mode,1> }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { modes: Map<Bool,Mode,1>[True == True => Warm] };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("concrete equality map key should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{modes:Map[True=>Warm]}"]
    );
}

#[test]
fn infers_ambiguous_fieldless_variant_from_typed_equality_peer() {
    let source = r#"
module source_value_equality_context;

enum Bool { False, True }
enum Other { True }
record MainState;
enum MainMsg { Start }

fn same_bool(flag: Bool) -> Bool ! [] ~ [] @det {
    return flag == True;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("typed Bool operand should disambiguate True");
}

#[test]
fn equality_operand_diagnostics_do_not_use_match_scrutinee_wording() {
    for source in [
        r#"
module source_equality_ambiguous_operand;

enum Bool { False, True }
enum Other { True }
record MainState;
enum MainMsg { Start }

fn same(flag: Bool) -> Bool ! [] ~ [] @det {
    return True == True;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
        r#"
module runtime_equality_ambiguous_operand;

enum Bool { False, True }
enum Other { True }
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        if (True == True) {
            return Stop(state);
        } else {
            return Stop(state);
        }
    }
}
"#,
    ] {
        let err = check_source(source).expect_err("ambiguous equality operand should fail");
        let err = err.to_string();
        assert!(
            err.contains("equality operand True"),
            "expected equality operand context, got {err}"
        );
        assert!(
            !err.contains("match scrutinee"),
            "equality diagnostics should not leak match wording: {err}"
        );
    }
}

#[test]
fn rejects_source_equality_invalid_operand_types() {
    for (expr, expected) in [
        (
            "Cold == OtherCold",
            "equality operands must have the same type",
        ),
        ("Cold == True", "equality operands must have the same type"),
        (
            "Job { phase: Ready } == Job { phase: Ready }",
            "equality operands must be Bool or fieldless enum values",
        ),
        (
            "List<Bool,2>[True, False] == List<Bool,2>[True, False]",
            "equality operands must be Bool or fieldless enum values",
        ),
        (
            "Map<Mode,Mode,1>[Cold => Warm] == Map<Mode,Mode,1>[Cold => Warm]",
            "equality operands must be Bool or fieldless enum values",
        ),
        (
            "Wrapped(Ready) == Wrapped(Ready)",
            "equality operands must be Bool or fieldless enum values",
        ),
    ] {
        let source = source_with_equality_expr(expr);
        let err = check_source(&source).expect_err("invalid equality operand should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err} for {expr}"
        );
    }
}

#[test]
fn rejects_bound_record_list_and_map_equality() {
    for (source, expected) in [
        (
            r#"
module source_record_equality_reject;

enum Bool { False, True }
enum Phase { Ready }
record Job { phase: Phase }
record MainState;
enum MainMsg { Start }

fn same_job(job: Job) -> Bool ! [] ~ [] @det {
    return job == job;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
            "record equality is not supported in this source slice",
        ),
        (
            r#"
module source_list_equality_reject;

enum Bool { False, True }
record MainState;
enum MainMsg { Start }

fn same_list(items: List<Bool,1>) -> Bool ! [] ~ [] @det {
    return items == items;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
            "list and map equality are not supported in this source slice",
        ),
        (
            r#"
module source_map_equality_reject;

enum Bool { False, True }
enum Phase { Ready }
record MainState;
enum MainMsg { Start }

fn same_map(items: Map<Phase,Bool,1>) -> Bool ! [] ~ [] @det {
    return items == items;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#,
            "list and map equality are not supported in this source slice",
        ),
    ] {
        let err = check_source(source).expect_err("bound invalid equality operand should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_process_reference_equality() {
    let source = r#"
module source_process_ref_equality;

record MainState;
record WorkerState;
enum Bool { False, True }
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        if (worker == worker) {
            return Stop(state);
        } else {
            return Stop(state);
        }
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

    let err = check_source(source).expect_err("process-reference equality should fail");
    assert!(
        err.to_string()
            .contains("equality operand worker must be a Bool or fieldless enum value"),
        "{err}"
    );
}

#[test]
fn rejects_non_equality_operators_and_arithmetic_conditions() {
    for (source, expected) in [
        (source_with_equality_expr("True < True"), "expected symbol"),
        (source_with_equality_expr("True <= True"), "expected symbol"),
        (source_with_equality_expr("True > True"), "expected symbol"),
        (source_with_equality_expr("True >= True"), "expected symbol"),
        (
            source_with_equality_expr("\"a\" == \"a\""),
            "expected identifier",
        ),
        (
            source_with_equality_expr("True + True"),
            "unsupported character '+'",
        ),
    ] {
        let err = parse_source(&source).expect_err("unsupported operator should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

fn source_with_equality_expr(expr: &str) -> String {
    format!(
        r#"
module source_value_equality_reject;

enum Bool {{ False, True }}
enum Mode {{ Cold, Warm }}
enum Other {{ OtherCold }}
enum Phase {{ Ready }}
enum PayloadEnum {{ Wrapped(Phase) }}
record Job {{ phase: Phase }}
record MainState {{ value: Bool }}
enum MainMsg {{ Start }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{ value: {expr} }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}
