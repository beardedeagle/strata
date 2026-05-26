use super::support::*;

#[test]
fn rejects_scalar_failures_in_unselected_source_if_branches() {
    for case in scalar_failure_cases() {
        for (context, source) in [
            (
                "value-if else branch",
                hidden_value_if_failure_source("True", "1_u8", case.expr),
            ),
            (
                "value-if then branch",
                hidden_value_if_failure_source("False", case.expr, "1_u8"),
            ),
            (
                "return-if else branch",
                hidden_return_if_failure_source("True", "1_u8", case.expr),
            ),
            (
                "return-if then branch",
                hidden_return_if_failure_source("False", case.expr, "1_u8"),
            ),
        ] {
            let err = check_source(&source).expect_err(context);
            assert!(
                err.to_string().contains(case.diagnostic),
                "expected {:?}, got {err} for {context}",
                case.diagnostic
            );
        }
    }
}

#[test]
fn bounded_scalar_expression_shapes_cover_bindings_collections_and_source_if() {
    for case in scalar_bounded_cases() {
        let source = bounded_scalar_shape_source(&case);
        let checked = check_source(&source).expect("bounded scalar shape should check");
        let selected = case.selected();
        let ordered = case.ordered_label();
        assert_eq!(
            checked_state_labels(&checked.processes()[0]),
            [format!(
                "MainState{{direct:{value}_u8,via_binding:{value}_u8,inline_selected:{selected}_u8,returned_selected:{selected}_u8,values:List[{value}_u8,{value}_u8,{selected}_u8],mapping:Map[{value}_u8=>{value}_u8],ordered:{ordered}}}",
                value = case.expected,
            )]
        );

        let artifact = lower_to_artifact(&checked, &source).expect("bounded shape should lower");
        let encoded = artifact.encode();
        for source_only_name in [
            "bind_value",
            "choose_with_return_if",
            "expand_value",
            "scalar_binding_generated",
            "scalar_second_generated",
        ] {
            assert!(
                !encoded.contains(source_only_name),
                "{source_only_name} must not lower into executable artifact meaning"
            );
        }
    }
}

fn hidden_value_if_failure_source(condition: &str, then_expr: &str, else_expr: &str) -> String {
    format!(
        r#"
module hidden_value_if_failure;

enum Bool {{ False, True }}
record MainState {{ value: U8 }}
enum MainMsg {{ Start }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{ value: if ({condition}) {{ {then_expr} }} else {{ {else_expr} }} }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn hidden_return_if_failure_source(condition: &str, then_expr: &str, else_expr: &str) -> String {
    format!(
        r#"
module hidden_return_if_failure;

enum Bool {{ False, True }}
record MainState {{ value: U8 }}
enum MainMsg {{ Start }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return if ({condition}) {{
            MainState {{ value: {then_expr} }}
        }} else {{
            MainState {{ value: {else_expr} }}
        }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn bounded_scalar_shape_source(case: &ScalarBoundedCase) -> String {
    format!(
        r#"
module source_scalar_bounded_shapes_{name};

enum Bool {{ False, True }}
record MainState {{
    direct: U8,
    via_binding: U8,
    inline_selected: U8,
    returned_selected: U8,
    values: List<U8,3>,
    mapping: Map<U8,U8,1>,
    ordered: Bool,
}}
enum MainMsg {{ Start }}

fn bind_value(seed: U8) -> U8 ! [] ~ [] @det {{
    let scalar_binding_generated: U8 = seed + 0_u8;
    return scalar_binding_generated;
}}

fn expand_value(seed: U8) -> U8 ! [] ~ [] @det {{
    let scalar_binding_generated: U8 = bind_value(seed);
    let scalar_second_generated: U8 = scalar_binding_generated + 0_u8;
    return scalar_second_generated;
}}

fn choose_with_return_if(seed: U8) -> U8 ! [] ~ [] @det {{
    return if (seed >= {threshold}_u8) {{
        seed
    }} else {{
        seed + 1_u8
    }};
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{
            direct: {expr},
            via_binding: expand_value({expr}),
            inline_selected: if (({expr}) >= {threshold}_u8) {{ {expr} }} else {{ ({expr}) + 1_u8 }},
            returned_selected: choose_with_return_if({expr}),
            values: List<U8,3>[{expr}, expand_value({expr}), choose_with_return_if({expr})],
            mapping: Map<U8,U8,1>[{expr} => expand_value({expr})],
            ordered: ({expr}) >= {threshold}_u8,
        }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#,
        name = case.name,
        expr = case.expr,
        threshold = case.threshold,
    )
}

struct ScalarFailureCase {
    expr: &'static str,
    diagnostic: &'static str,
}

fn scalar_failure_cases() -> [ScalarFailureCase; 3] {
    [
        ScalarFailureCase {
            expr: "255_u8 + 1_u8",
            diagnostic: "scalar arithmetic result 256 is outside U8 range",
        },
        ScalarFailureCase {
            expr: "1_u8 / 0_u8",
            diagnostic: "scalar division by zero",
        },
        ScalarFailureCase {
            expr: "1_u8 % 0_u8",
            diagnostic: "scalar modulo by zero",
        },
    ]
}

struct ScalarBoundedCase {
    name: &'static str,
    expr: &'static str,
    expected: u8,
    threshold: u8,
}

impl ScalarBoundedCase {
    fn selected(&self) -> u8 {
        if self.expected >= self.threshold {
            self.expected
        } else {
            self.expected
                .checked_add(1)
                .expect("bounded selected value should fit")
        }
    }

    fn ordered_label(&self) -> &'static str {
        if self.expected >= self.threshold {
            "True"
        } else {
            "False"
        }
    }
}

fn scalar_bounded_cases() -> [ScalarBoundedCase; 4] {
    [
        ScalarBoundedCase {
            name: "tree_add_mul_div_mod",
            expr: "((2_u8 + 3_u8) * 4_u8 / 2_u8) % 9_u8",
            expected: model_tree_add_mul_div_mod(),
            threshold: 5,
        },
        ScalarBoundedCase {
            name: "tree_sub_mul_add",
            expr: "(8_u8 - 3_u8) * (2_u8 + 1_u8)",
            expected: model_tree_sub_mul_add(),
            threshold: 12,
        },
        ScalarBoundedCase {
            name: "tree_div_mod_add",
            expr: "(12_u8 / 3_u8) + (7_u8 % 4_u8)",
            expected: model_tree_div_mod_add(),
            threshold: 7,
        },
        ScalarBoundedCase {
            name: "value_if_tree",
            expr: "if (1_u8 + 2_u8 >= 3_u8) { 7_u8 } else { 9_u8 }",
            expected: 7,
            threshold: 8,
        },
    ]
}

fn model_tree_add_mul_div_mod() -> u8 {
    checked_rem(checked_div(checked_mul(checked_add(2, 3), 4), 2), 9)
}

fn model_tree_sub_mul_add() -> u8 {
    checked_mul(checked_sub(8, 3), checked_add(2, 1))
}

fn model_tree_div_mod_add() -> u8 {
    checked_add(checked_div(12, 3), checked_rem(7, 4))
}

fn checked_add(left: u8, right: u8) -> u8 {
    left.checked_add(right)
        .expect("bounded scalar model addition should fit")
}

fn checked_sub(left: u8, right: u8) -> u8 {
    left.checked_sub(right)
        .expect("bounded scalar model subtraction should fit")
}

fn checked_mul(left: u8, right: u8) -> u8 {
    left.checked_mul(right)
        .expect("bounded scalar model multiplication should fit")
}

fn checked_div(left: u8, right: u8) -> u8 {
    left.checked_div(right)
        .expect("bounded scalar model division should fit")
}

fn checked_rem(left: u8, right: u8) -> u8 {
    left.checked_rem(right)
        .expect("bounded scalar model modulo should fit")
}
