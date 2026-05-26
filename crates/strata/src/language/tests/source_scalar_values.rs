use mantle_artifact::{ArtifactAction, ArtifactValueTemplate, NextState};

use super::support::*;

#[test]
fn parses_typed_scalar_literals_and_operator_precedence() {
    let source = r#"
module source_scalar_precedence;

enum Bool { False, True }
record MainState;
enum MainMsg { Start }

fn arithmetic() -> U32 ! [] ~ [] @det {
    return 1_u32 + 2_u32 * 3_u32 - 4_u32 / 2_u32 % 2_u32;
}

fn ordering() -> Bool ! [] ~ [] @det {
    return 1_u32 + 2_u32 <= 3_u32 * 4_u32;
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

    let module = parse_source(source).expect("scalar source should parse");
    let arithmetic = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "arithmetic")
        .expect("arithmetic function should parse");
    let Some(FunctionBody::Block(body)) = &arithmetic.body else {
        panic!("arithmetic should parse as a block body");
    };
    let ReturnExpr::Value(value) = &body.returns else {
        panic!("arithmetic should return a value");
    };
    assert_eq!(
        value.to_string(),
        "1_u32 + 2_u32 * 3_u32 - 4_u32 / 2_u32 % 2_u32"
    );

    let ordering = module
        .functions
        .iter()
        .find(|function| function.name.as_str() == "ordering")
        .expect("ordering function should parse");
    let Some(FunctionBody::Block(body)) = &ordering.body else {
        panic!("ordering should parse as a block body");
    };
    let ReturnExpr::Value(value) = &body.returns else {
        panic!("ordering should return a value");
    };
    assert_eq!(value.to_string(), "1_u32 + 2_u32 <= 3_u32 * 4_u32");
}

#[test]
fn checks_all_admitted_scalar_literal_suffixes() {
    let source = r#"
module source_scalar_suffixes;

record MainState {
    u8_value: U8,
    u16_value: U16,
    u32_value: U32,
    u64_value: U64,
    i8_value: I8,
    i16_value: I16,
    i32_value: I32,
    i64_value: I64,
}
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            u8_value: 255_u8,
            u16_value: 65535_u16,
            u32_value: 4294967295_u32,
            u64_value: 18446744073709551615_u64,
            i8_value: -128_i8,
            i16_value: -32768_i16,
            i32_value: -2147483648_i32,
            i64_value: -9223372036854775808_i64,
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("all scalar suffixes should check");

    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        [
            "MainState{u8_value:255_u8,u16_value:65535_u16,u32_value:4294967295_u32,u64_value:18446744073709551615_u64,i8_value:-128_i8,i16_value:-32768_i16,i32_value:-2147483648_i32,i64_value:-9223372036854775808_i64}"
        ]
    );
}

#[test]
fn folds_concrete_scalar_bindings_records_lists_maps_and_if() {
    let source = r#"
module source_scalar_concrete;

enum Bool { False, True }
enum Priority { Normal, High }
record Job { weight: U32 }
record MainState {
    adjusted: U32,
    urgent: Bool,
    same: Bool,
    priority: Priority,
    values: List<U32,3>,
    mapping: Map<U32,U32,1>,
}
enum MainMsg { Start }

    fn adjusted_weight(Job { weight }) -> U32 ! [] ~ [] @det {
        let base_local: U32 = weight;
        let adjusted_local: U32 = (base_local + 2_u32 * 3_u32);
        return adjusted_local;
    }

fn classify(job: Job) -> Priority ! [] ~ [] @det {
    let adjusted_local: U32 = adjusted_weight(job);
    let urgent_local: Bool = adjusted_local >= 10_u32;
    return if (urgent_local) { High } else { Normal };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            adjusted: adjusted_weight(Job { weight: 4_u32 }),
            urgent: adjusted_weight(Job { weight: 4_u32 }) >= 10_u32,
            same: (adjusted_weight(Job { weight: 4_u32 })) == (10_u32),
            priority: classify(Job { weight: 4_u32 }),
            values: List<U32,3>[1_u32, 1_u32 + 1_u32, 3_u32],
            mapping: Map<U32,U32,1>[1_u32 => 2_u32 + 3_u32],
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("concrete scalar source should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        [
            "MainState{adjusted:10_u32,urgent:True,same:True,priority:High,values:List[1_u32,2_u32,3_u32],mapping:Map[1_u32=>5_u32]}"
        ]
    );

    let artifact = lower_to_artifact(&checked, source).expect("scalar source should lower");
    let encoded = artifact.encode();
    assert!(!encoded.contains(".kind=scalar_"));
    for source_only_name in [
        "base_local",
        "adjusted_local",
        "urgent_local",
        "adjusted_weight",
        "classify",
    ] {
        assert!(
            !encoded.contains(source_only_name),
            "{source_only_name} must not lower into executable artifact meaning"
        );
    }
}

#[test]
fn lowers_runtime_bound_scalar_operators_as_typed_templates() {
    let source = r#"
module runtime_scalar_templates;

enum Bool { False, True }
enum Priority { Normal, High }
record MainState {
    selected: Priority,
    level: U32,
}
enum MainMsg { Start, Assign(U32) }

fn compute_level(weight: U32) -> U32 ! [] ~ [] @det {
    let scalar_base_local: U32 = weight;
    let scalar_sum_local: U32 = scalar_base_local + 2_u32;
    return scalar_sum_local;
}

fn is_high_priority(weight: U32) -> Bool ! [] ~ [] @det {
    return compute_level(weight) >= 10_u32;
}

fn classify(weight: U32) -> Priority ! [] ~ [] @det {
    let adjusted_local: U32 = compute_level(weight);
    let urgent_local: Bool = adjusted_local >= 10_u32;
    return if (urgent_local) { High } else { Normal };
}

fn call_pair_high(weight: U32) -> Bool ! [] ~ [] @det {
    return compute_level(weight) >= threshold_level(weight);
}

fn same_adjusted(weight: U32) -> Bool ! [] ~ [] @det {
    return (weight + 2_u32) == (weight + 1_u32 + 1_u32);
}

fn threshold_level(weight: U32) -> U32 ! [] ~ [] @det {
    let threshold_local: U32 = weight + 1_u32 + 1_u32;
    return threshold_local;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: Normal, level: 0_u32 };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, Assign(weight: U32)) -> ProcResult<MainState> ! [emit] ~ [] @det {
        if (same_adjusted(weight)) {
            emit "same adjusted";
        } else {
            emit "different adjusted";
        }
        if (call_pair_high(weight)) {
            emit "call pair high";
        } else {
            emit "call pair low";
        }
        if (is_high_priority(weight)) {
            emit "selected high";
            return Continue(MainState { selected: classify(weight), level: (weight + 2_u32) });
        } else {
            emit "selected normal";
            return Continue(MainState { selected: classify(weight), level: compute_level(weight) });
        }
    }
}
"#;

    let checked = check_source(source).expect("runtime-bound scalar source should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("runtime-bound scalar source should lower");
    let transition = artifact.processes[0]
        .transitions
        .iter()
        .find(|transition| transition.message.as_u32() == 1)
        .expect("Assign transition should lower");
    let NextState::IfElse {
        condition,
        then_state,
        ..
    } = &transition.next_state
    else {
        panic!("Assign transition should lower to a typed runtime branch");
    };
    let ArtifactValueTemplate::ScalarOrdering { left, right, .. } = condition else {
        panic!("runtime scalar predicate should lower as a scalar ordering template");
    };
    assert!(matches!(
        left.as_ref(),
        ArtifactValueTemplate::ScalarArithmetic { .. }
    ));
    assert!(matches!(
        right.as_ref(),
        ArtifactValueTemplate::Literal { .. }
    ));
    let NextState::Template(ArtifactValueTemplate::Record { fields, .. }) = then_state.as_ref()
    else {
        panic!("then branch should build a state record");
    };
    let selected = fields
        .iter()
        .find(|field| field.name == "selected")
        .expect("selected field should lower");
    assert!(matches!(
        &selected.value,
        ArtifactValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } if matches!(condition.as_ref(), ArtifactValueTemplate::ScalarOrdering { .. })
            && matches!(
                then_value.as_ref(),
                ArtifactValueTemplate::Literal { value, .. } if value == &artifact_value("High")
            )
            && matches!(
                else_value.as_ref(),
                ArtifactValueTemplate::Literal { value, .. } if value == &artifact_value("Normal")
            )
    ));
    let condition = transition
        .actions
        .iter()
        .find_map(|action| match action {
            ArtifactAction::IfElse {
                condition: condition @ ArtifactValueTemplate::Equality { .. },
                ..
            } => Some(condition),
            _ => None,
        })
        .expect("Assign transition should lower scalar equality as one action branch");
    let ArtifactValueTemplate::Equality { left, right, .. } = condition else {
        panic!("runtime scalar equality should lower as an equality template");
    };
    assert!(matches!(
        left.as_ref(),
        ArtifactValueTemplate::ScalarArithmetic { .. }
    ));
    assert!(matches!(
        right.as_ref(),
        ArtifactValueTemplate::ScalarArithmetic { .. }
    ));
    let call_pair_condition = transition
        .actions
        .iter()
        .find_map(|action| match action {
            ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::ScalarOrdering { left, right, .. },
                ..
            } if matches!(
                left.as_ref(),
                ArtifactValueTemplate::ScalarArithmetic { .. }
            ) && matches!(
                right.as_ref(),
                ArtifactValueTemplate::ScalarArithmetic { .. }
            ) =>
            {
                Some((left.as_ref(), right.as_ref()))
            }
            _ => None,
        })
        .expect("call-pair scalar ordering should lower as typed arithmetic operands");
    assert!(matches!(
        call_pair_condition,
        (
            ArtifactValueTemplate::ScalarArithmetic { .. },
            ArtifactValueTemplate::ScalarArithmetic { .. }
        )
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=scalar_ordering"));
    assert!(encoded.contains(".kind=scalar_arithmetic"));
    for source_only_name in [
        "scalar_base_local",
        "scalar_sum_local",
        "threshold_local",
        "compute_level",
        "is_high_priority",
        "classify",
        "call_pair_high",
        "same_adjusted",
        "threshold_level",
    ] {
        assert!(
            !encoded.contains(source_only_name),
            "{source_only_name} must not lower into executable artifact meaning"
        );
    }
}

#[test]
fn rejects_runtime_bound_static_zero_scalar_divisors() {
    for (operator, diagnostic) in [
        ("/", "scalar division by zero"),
        ("%", "scalar modulo by zero"),
    ] {
        let source = runtime_bound_scalar_divisor_source(operator);
        let err = check_source(&source).expect_err("static zero divisor should fail checking");

        assert!(
            err.to_string().contains(diagnostic),
            "expected {diagnostic:?}, got {err}"
        );
    }
}

#[test]
fn rejects_invalid_scalar_source_forms() {
    for (expr, expected) in [
        ("256_u8", "outside U8 range"),
        ("-1_u8", "unsigned scalar literal"),
        ("1_u16", "has type U16, expected U32"),
        ("1_u32 + 2_u64", "has type U64, expected U32"),
        ("4294967295_u32 + 1_u32", "scalar arithmetic result"),
        ("1_u32 / 0_u32", "scalar division by zero"),
        ("1_u32 % 0_u32", "scalar modulo by zero"),
    ] {
        let source = scalar_source_with_init_expr(expr, "U32");
        let err = check_source(&source).expect_err("invalid scalar expression should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err} for {expr}"
        );
    }
}

#[test]
fn rejects_unsuffixed_numeric_value_expressions_and_non_bool_scalar_conditions() {
    let unsuffixed = scalar_source_with_init_expr("1 + 2", "U32");
    let err = parse_source(&unsuffixed).expect_err("unsuffixed numeric value should fail");
    assert!(
        err.to_string()
            .contains("numeric value literals require an explicit scalar suffix"),
        "{err}"
    );

    for (expr, ty) in [("1 _u32", "U32"), ("- 1_i8", "I8")] {
        let source = scalar_source_with_init_expr(expr, ty);
        let err = parse_source(&source).expect_err("spaced scalar literal should fail");
        assert!(
            err.to_string().contains("contiguous"),
            "{expr} failed with unexpected diagnostic: {err}"
        );
    }

    let source = r#"
module source_scalar_non_bool_condition;

enum Bool { False, True }
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return if (1_u32) { MainState } else { MainState };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("scalar condition should fail");
    assert!(err.to_string().contains("if condition must have type Bool"));
}

#[test]
fn rejects_malformed_scalar_suffixes_and_operator_chains() {
    for expr in ["1_u128", "1_u32x"] {
        let source = scalar_source_with_init_expr(expr, "U32");
        let err = parse_source(&source).expect_err("unsupported scalar suffix should fail");
        assert!(
            err.to_string()
                .contains("unsupported scalar literal suffix"),
            "{expr} failed with unexpected diagnostic: {err}"
        );
    }

    for (expr, expected) in [
        (
            "1_u32 < 2_u32 < 3_u32",
            "chained scalar ordering expressions are not supported",
        ),
        (
            "1_u32 == 1_u32 == 1_u32",
            "chained equality expressions are not supported",
        ),
    ] {
        let source = scalar_source_with_init_expr(expr, "Bool");
        let err = parse_source(&source).expect_err("malformed scalar operator chain should fail");
        assert!(
            err.to_string().contains(expected),
            "{expr} failed with unexpected diagnostic: {err}"
        );
    }
}

#[test]
fn bounded_scalar_folding_matches_independent_model_and_binding_expansion() {
    for case in bounded_scalar_cases() {
        let source = format!(
            r#"
module source_scalar_bounded_{name};

record MainState {{
    direct: U8,
    bound: U8,
    ordered: Bool,
}}
enum Bool {{ False, True }}
enum MainMsg {{ Start }}

fn bound_value(seed: U8) -> U8 ! [] ~ [] @det {{
    let scalar_binding_generated: U8 = seed + 0_u8;
    return scalar_binding_generated;
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{
            direct: {expr},
            bound: bound_value({expr}),
            ordered: {ordered_expr},
        }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#,
            name = case.name,
            expr = case.expr,
            ordered_expr = case.ordered_expr,
        );

        let checked = check_source(&source).expect("bounded scalar case should check");
        assert_eq!(
            checked_state_labels(&checked.processes()[0]),
            [format!(
                "MainState{{direct:{value}_u8,bound:{value}_u8,ordered:{ordered}}}",
                value = case.expected,
                ordered = if case.ordered { "True" } else { "False" }
            )]
        );
        let artifact =
            lower_to_artifact(&checked, &source).expect("bounded scalar case should lower");
        let encoded = artifact.encode();
        assert!(!encoded.contains("bound_value"));
        assert!(!encoded.contains("scalar_binding_generated"));
    }
}

fn runtime_bound_scalar_divisor_source(operator: &str) -> String {
    format!(
        r#"
module runtime_scalar_zero_divisor;

record MainState {{ value: U32 }}
enum MainMsg {{ Start, Set(U32) }}

fn broken(weight: U32) -> U32 ! [] ~ [] @det {{
    return weight {operator} 0_u32;
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{ value: 1_u32 }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}

    fn step(state: MainState, Set(weight: U32)) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Continue(MainState {{ value: broken(weight) }});
    }}
}}
"#
    )
}

fn scalar_source_with_init_expr(expr: &str, ty: &str) -> String {
    format!(
        r#"
module source_scalar_invalid;

record MainState {{ value: {ty} }}
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

struct BoundedScalarCase {
    name: &'static str,
    expr: &'static str,
    ordered_expr: &'static str,
    expected: u8,
    ordered: bool,
}

fn bounded_scalar_cases() -> [BoundedScalarCase; 5] {
    [
        BoundedScalarCase {
            name: "add",
            expr: "2_u8 + 3_u8",
            ordered_expr: "2_u8 + 3_u8 > 4_u8",
            expected: 2u8.checked_add(3).expect("bounded model should fit"),
            ordered: 2u8.checked_add(3).expect("bounded model should fit") > 4,
        },
        BoundedScalarCase {
            name: "sub",
            expr: "5_u8 - 3_u8",
            ordered_expr: "5_u8 - 3_u8 < 3_u8",
            expected: 5u8.checked_sub(3).expect("bounded model should fit"),
            ordered: 5u8 - 3 < 3,
        },
        BoundedScalarCase {
            name: "mul",
            expr: "4_u8 * 6_u8",
            ordered_expr: "4_u8 * 6_u8 > 20_u8",
            expected: 4u8.checked_mul(6).expect("bounded model should fit"),
            ordered: 4u8 * 6 > 20,
        },
        BoundedScalarCase {
            name: "div",
            expr: "9_u8 / 2_u8",
            ordered_expr: "9_u8 / 2_u8 < 5_u8",
            expected: 9u8.checked_div(2).expect("bounded model should fit"),
            ordered: 9u8.checked_div(2).expect("bounded model should fit") < 5,
        },
        BoundedScalarCase {
            name: "mod",
            expr: "9_u8 % 4_u8",
            ordered_expr: "9_u8 % 4_u8 > 0_u8",
            expected: 9u8.checked_rem(4).expect("bounded model should fit"),
            ordered: 9u8.checked_rem(4).expect("bounded model should fit") > 0,
        },
    ]
}
