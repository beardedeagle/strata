use super::support::*;

const SOURCE_EQUALITY: &str = r#"
module source_value_equality;
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
        .expect("bool_eq function should parse");
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
fn folds_concrete_boolean_predicate_composition() {
    let source = r#"
module source_boolean_predicates;
record MainState {
    all_true: Bool,
    false_and: Bool,
    true_or: Bool,
    not_false: Bool,
    grouped: Bool,
    precedence: Bool,
}
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            all_true: True && True,
            false_and: True && False,
            true_or: False || True,
            not_false: !(False),
            grouped: !(False || False),
            precedence: True || False && False,
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("concrete boolean predicates should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        [
            "MainState{all_true:True,false_and:False,true_or:True,not_false:True,grouped:True,precedence:True}"
        ]
    );

    let artifact =
        lower_to_artifact(&checked, source).expect("concrete boolean predicates should lower");
    let encoded = artifact.encode();
    assert!(
        !encoded.contains(".kind=boolean_"),
        "fully concrete boolean predicate composition should fold before lowering"
    );
}

#[test]
fn boolean_predicate_display_uses_readable_precedence() {
    let and_inside_or = ValueExpr::BooleanBinary {
        operator: ValueBooleanOperator::Or,
        left: Box::new(ValueExpr::BooleanBinary {
            operator: ValueBooleanOperator::And,
            left: Box::new(ValueExpr::Identifier(ident("flag"))),
            right: Box::new(ValueExpr::Identifier(ident("ready"))),
        }),
        right: Box::new(ValueExpr::Identifier(ident("open"))),
    };
    assert_eq!(and_inside_or.to_string(), "flag && ready || open");

    let or_inside_and = ValueExpr::BooleanBinary {
        operator: ValueBooleanOperator::And,
        left: Box::new(ValueExpr::BooleanBinary {
            operator: ValueBooleanOperator::Or,
            left: Box::new(ValueExpr::Identifier(ident("flag"))),
            right: Box::new(ValueExpr::Identifier(ident("ready"))),
        }),
        right: Box::new(ValueExpr::BooleanNot {
            operand: Box::new(ValueExpr::BooleanBinary {
                operator: ValueBooleanOperator::And,
                left: Box::new(ValueExpr::Identifier(ident("open"))),
                right: Box::new(ValueExpr::Identifier(ident("enabled"))),
            }),
        }),
    };
    assert_eq!(
        or_inside_and.to_string(),
        "(flag || ready) && !(open && enabled)"
    );
}

#[test]
fn rejects_boolean_predicate_non_bool_operands() {
    for (expr, expected) in [
        ("!Cold", "boolean ! operand must produce Bool"),
        ("Cold && True", "left operand of && must produce Bool"),
        ("True || Cold", "right operand of || must produce Bool"),
    ] {
        let source = source_with_equality_expr(expr);
        let err = check_source(&source).expect_err("non-Bool boolean operand should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err} for {expr}"
        );
    }
}

#[test]
fn accepts_parenthesized_non_bool_value_grouping() {
    let source = r#"
module source_grouping_accepts_value;
enum Mode { Cold, Warm }
record MainState { mode: Mode }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { mode: (Cold) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("parenthesized non-Bool value should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{mode:Cold}"]
    );
}

#[test]
fn resolves_core_bool_constructor_from_typed_equality_peer() {
    let source = r#"
module source_value_equality_context;
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

    check_source(source).expect("typed Bool operand should resolve core True");
}

#[test]
fn equality_operand_diagnostics_do_not_use_match_scrutinee_wording() {
    for source in [
        r#"
module source_equality_payload_operand;
enum Other { Maybe(Bool) }
record MainState;
enum MainMsg { Start }

fn same(flag: Bool) -> Bool ! [] ~ [] @det {
    return Maybe == Maybe;
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
module runtime_equality_payload_operand;
enum Other { Maybe(Bool) }
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        if (Maybe == Maybe) {
            return Stop(state);
        } else {
            return Stop(state);
        }
    }
}
"#,
    ] {
        let err = check_source(source).expect_err("payload equality operand should fail");
        let err = err.to_string();
        assert!(
            err.contains("equality operand Maybe"),
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
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
        ),
        (
            "List<Bool,2>[True, False] == List<Bool,2>[True, False]",
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
        ),
        (
            "Map<Mode,Mode,1>[Cold => Warm] == Map<Mode,Mode,1>[Cold => Warm]",
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
        ),
        (
            "Wrapped(Ready) == Wrapped(Ready)",
            "equality operands must be Bool, String, Bytes, scalar values, or fieldless enum values",
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
            "record equality is not supported",
        ),
        (
            r#"
module source_list_equality_reject;
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
            "list and map equality are not supported",
        ),
        (
            r#"
module source_map_equality_reject;
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
            "list and map equality are not supported",
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
fn rejects_direct_builtin_payload_enum_equality_between_runtime_values() {
    let source = r#"
module source_builtin_payload_enum_equality_reject;
enum Phase { Ready }
record MainState;
enum MainMsg { Start }

fn same_option(input: Option<Phase>) -> Bool ! [] ~ [] @det {
    return input == input;
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

    let err = check_source(source).expect_err("direct builtin payload enum equality should fail");

    assert!(
        err.to_string()
            .contains("requires one operand to be a safe built-in variant pattern"),
        "{err}"
    );
}

#[test]
fn rejects_process_reference_equality() {
    let source = r#"
module source_process_ref_equality;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

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
        err.to_string().contains(
            "equality operand worker must be a Bool, String, Bytes, scalar value, or fieldless enum value"
        ),
        "{err}"
    );
}

#[test]
fn rejects_non_equality_operators_and_accepts_primitive_equality() {
    for (expr, expected) in [
        (
            "True < True",
            "scalar operand True must be a scalar value binding",
        ),
        (
            "True <= True",
            "scalar operand True must be a scalar value binding",
        ),
        (
            "True > True",
            "scalar operand True must be a scalar value binding",
        ),
        (
            "True >= True",
            "scalar operand True must be a scalar value binding",
        ),
        ("True + True", "type Bool is not a scalar integer type"),
    ] {
        let source = source_with_equality_expr(expr);
        let err = check_source(&source).expect_err("unsupported Bool operator should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }

    for expr in ["\"a\" == \"a\"", "b\"\\x01\" != b\"\\x02\""] {
        let source = source_with_equality_expr(expr);
        let checked = check_source(&source).expect("primitive equality should check");
        assert_eq!(
            checked_state_labels(&checked.processes()[0]),
            ["MainState{value:True}"]
        );
    }
}

fn source_with_equality_expr(expr: &str) -> String {
    format!(
        r#"
module source_value_equality_reject;
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
