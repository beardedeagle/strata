use super::support::*;

const CORE_BOOL_SOURCE: &str = r#"
module core_bool_builtin;
enum Mode { Cold, Warm }
record MainState {
    selected: Bool,
    ordered: Bool,
}
enum MainMsg { Boot, Start(Bool) }

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

fn ordered(value: U32) -> Bool ! [] ~ [] @det {
    return value < 2_u32;
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { selected: is_warm(Warm), ordered: ordered(1_u32) };
    }

    fn step(state: MainState, Boot) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Start(flag: Bool)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { selected: flag, ordered: 3_u32 >= 3_u32 });
    }
}
"#;

#[test]
fn core_bool_is_available_without_source_declaration() {
    let module = parse_source(CORE_BOOL_SOURCE).expect("core Bool source should parse");
    assert!(
        module.enums.iter().all(|item| item.name.as_str() != "Bool"),
        "source must not declare Bool explicitly"
    );

    let checked = check_module(module).expect("core Bool source should check");
    let bool_type = checked
        .types()
        .iter()
        .find(|ty| ty.label() == "Bool")
        .expect("checked IR should contain core Bool type");
    assert!(matches!(
        bool_type.kind(),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum { variants }
        } if variants.len() == 2
            && variants[0].name.as_str() == "False"
            && variants[0].payload_type.is_none()
            && variants[1].name.as_str() == "True"
            && variants[1].payload_type.is_none()
    ));

    let artifact =
        lower_to_artifact(&checked, CORE_BOOL_SOURCE).expect("core Bool source should lower");
    let bool_type_id = artifact_type_id(&artifact, "Bool");
    let artifact_bool = artifact
        .types
        .get(bool_type_id.index())
        .expect("artifact Bool type id should resolve");
    assert!(matches!(
        &artifact_bool.shape,
        Some(ArtifactValueShape::Enum { variants })
            if variants.len() == 2
                && variants[0].label == "False"
                && variants[0].payload_type.is_none()
                && variants[1].label == "True"
                && variants[1].payload_type.is_none()
    ));
}

#[test]
fn core_bool_rejects_conflicting_user_declarations() {
    for (declaration, expected) in [
        ("record Bool;", "record Bool conflicts with core Bool type"),
        (
            "enum Bool { No, Yes }",
            "enum Bool conflicts with core Bool type",
        ),
        (
            "record Flag { Bool: Bool }",
            "record field Bool conflicts with core Bool type",
        ),
        (
            "record Flag { True: Bool }",
            "record field True conflicts with core Bool value constructor",
        ),
        (
            "enum Flag { Bool, On }",
            "enum variant Bool conflicts with core Bool type",
        ),
        (
            "enum Flag { False, On }",
            "enum variant False conflicts with core Bool value constructor",
        ),
        (
            "enum Flag { Off, True(Unit) }",
            "enum variant True conflicts with core Bool value constructor",
        ),
        (
            "fn True(flag: Bool) -> Bool ! [] ~ [] @det { return flag; }",
            "module function True conflicts with core Bool value constructor",
        ),
        (
            "fn Bool(flag: Bool) -> Bool ! [] ~ [] @det { return flag; }",
            "module function Bool conflicts with core Bool type",
        ),
    ] {
        let source = format!(
            r#"
module core_bool_conflict;
{declaration}
record MainState;
enum MainMsg {{ Start }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
        );
        let err = check_source(&source).expect_err("core Bool conflict should fail");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?} for {declaration}, got {err}"
        );
    }
}

#[test]
fn core_bool_rejects_process_function_conflict() {
    let source = r#"
module core_bool_process_function_conflict;
record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn Bool(flag: Bool) -> Bool ! [] ~ [] @det {
        return flag;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("process function Bool should fail");
    assert!(
        err.to_string()
            .contains("process function Bool conflicts with core Bool type"),
        "unexpected error: {err}"
    );
}

#[test]
fn core_bool_lowers_deterministically_across_declaration_order() {
    let first = r#"
module core_bool_order_first;
enum Mode { Cold, Warm }
record MainState { flag: Bool, mode: Mode }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { flag: True, mode: Warm };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { flag: False, mode: Cold });
    }
}
"#;
    let second = r#"
module core_bool_order_second;
record MainState { flag: Bool, mode: Mode }
enum Mode { Cold, Warm }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { flag: True, mode: Warm };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { flag: False, mode: Cold });
    }
}
"#;

    let first_checked = check_source(first).expect("first Bool order source should check");
    let second_checked = check_source(second).expect("second Bool order source should check");
    let first_artifact = lower_to_artifact(&first_checked, first).expect("first source lowers");
    let second_artifact = lower_to_artifact(&second_checked, second).expect("second source lowers");

    assert_eq!(
        artifact_type_id(&first_artifact, "Bool"),
        artifact_type_id(&second_artifact, "Bool")
    );
    assert_eq!(
        first_artifact
            .types
            .iter()
            .find(|ty| ty.label == "Bool")
            .map(|ty| &ty.shape),
        second_artifact
            .types
            .iter()
            .find(|ty| ty.label == "Bool")
            .map(|ty| &ty.shape)
    );
}
