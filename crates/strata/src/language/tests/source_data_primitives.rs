use super::support::*;
use mantle_artifact::{ArtifactPrimitiveType, MAX_PRIMITIVE_DATA_BYTES};

const SOURCE_DATA_PRIMITIVES: &str =
    include_str!("../../../../../examples/source_contract_data_primitives.str");
const READY_STRING_LABEL: &str = "String(7265616479)";
const READY_BYTES_LABEL: &str = "Bytes(010262696e)";

#[test]
fn primitive_literal_display_is_canonical() {
    let string = ValueExpr::StringLiteral(
        SourceStringLiteral::new("line\n\"\\").expect("string literal should build"),
    );
    assert_eq!(string.to_string(), r#""line\n\"\\""#);

    let bytes = ValueExpr::BytesLiteral(
        SourceBytesLiteral::new(vec![0x00, b'a', b'"', b'\\', 0xff])
            .expect("bytes literal should build"),
    );
    assert_eq!(bytes.to_string(), r#"b"\x00a\"\\\xff""#);
}

#[test]
fn checks_lowers_and_preserves_typed_string_and_bytes_values() {
    let checked =
        check_source(SOURCE_DATA_PRIMITIVES).expect("source data primitives should check");
    let worker = &checked.processes()[1];
    let state_label = checked_state_labels(worker)
        .into_iter()
        .next()
        .expect("worker should have an initial state");
    assert!(state_label.contains(READY_STRING_LABEL), "{state_label}");
    assert!(state_label.contains(READY_BYTES_LABEL), "{state_label}");

    let artifact = lower_to_artifact(&checked, SOURCE_DATA_PRIMITIVES)
        .expect("source data primitives should lower");
    assert_eq!(
        artifact.types[artifact_type_id(&artifact, "String").index()].shape,
        Some(ArtifactValueShape::Primitive {
            primitive: ArtifactPrimitiveType::String,
        })
    );
    assert_eq!(
        artifact.types[artifact_type_id(&artifact, "Bytes").index()].shape,
        Some(ArtifactValueShape::Primitive {
            primitive: ArtifactPrimitiveType::Bytes,
        })
    );

    let encoded = artifact.encode();
    assert!(encoded.contains("type.1.shape=primitive"));
    assert!(encoded.contains("type.1.primitive_type=string"));
    assert!(encoded.contains("type.2.shape=primitive"));
    assert!(encoded.contains("type.2.primitive_type=bytes"));
    assert!(encoded.contains("target_requirements.feature.7=typed_value_templates"));
    assert!(encoded.contains("condition.left.operand_type_id=1"));
    assert!(encoded.contains("condition.left.right.value=String(7265616479)"));
    assert!(encoded.contains("condition.right.operand_type_id=2"));
    assert!(encoded.contains("condition.right.right.value=Bytes(010262696e)"));
}

#[test]
fn concrete_primitive_equality_folds_before_lowering() {
    let source = r#"
module primitive_equality_folds;
record MainState { string_eq: Bool, bytes_ne: Bool }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            string_eq: "ready" == "ready",
            bytes_ne: b"\x01" != b"\x02",
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("primitive equality should check");
    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{string_eq:True,bytes_ne:True}"]
    );

    let artifact = lower_to_artifact(&checked, source).expect("primitive equality should lower");
    assert!(
        !artifact.encode().contains(".kind=equality"),
        "fully concrete primitive equality should fold before lowering"
    );
}

#[test]
fn source_functions_accept_primitive_parameter_and_return_types() {
    let source = r#"
module primitive_source_functions;
record MainState { text: String, raw: Bytes }
enum MainMsg { Start }

fn echo_text(value: String) -> String ! [] ~ [] @det { return value; }
fn echo_raw(value: Bytes) -> Bytes ! [] ~ [] @det { return value; }
fn same_text(value: String) -> Bool ! [] ~ [] @det { return value == "ready"; }
fn same_raw(value: Bytes) -> Bool ! [] ~ [] @det { return value == b"\x01"; }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { text: echo_text("ready"), raw: echo_raw(b"\x01") };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        if (same_text("ready") && same_raw(b"\x01")) {
            emit "primitive source functions checked";
        } else {
        }
        return Stop(state);
    }
}
"#;

    check_source(source).expect("primitive source functions should check");
}

#[test]
fn primitive_equality_diagnostic_describes_ambiguous_equality_operand() {
    let source = r#"
module primitive_equality_if_else_diagnostic;
record MainState;
enum MainMsg { Start }

fn same_text(flag: Bool) -> Bool ! [] ~ [] @det {
    return (if (flag) { "ready" } else { "wait" }) == (if (flag) { "ready" } else { "done" });
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

    let err = check_source(source).expect_err("ambiguous primitive equality operand should fail");
    let message = err.to_string();
    assert!(
        message
            .contains("equality operand type is ambiguous; use a typed local binding or literal"),
        "unexpected error: {err}"
    );
    assert!(
        !message.contains("scalar equality operand"),
        "primitive equality diagnostic should not be scalar-only: {err}"
    );
}

#[test]
fn rejects_malformed_and_oversized_primitive_literals() {
    for (source, expected) in [
        (
            r#"module bad_string; record MainState { value: String } enum MainMsg { Start } proc Main mailbox bounded(1) { type State = MainState; type Msg = MainMsg; fn init() -> MainState ! [] ~ [] @det { return MainState { value: "\q" }; } fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); } }"#,
            "unsupported string escape",
        ),
        (
            r#"module bad_bytes; record MainState { value: Bytes } enum MainMsg { Start } proc Main mailbox bounded(1) { type State = MainState; type Msg = MainMsg; fn init() -> MainState ! [] ~ [] @det { return MainState { value: b"\xz1" }; } fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); } }"#,
            "must use two hex digits",
        ),
        (
            r#"module bad_bytes_unicode; record MainState { value: Bytes } enum MainMsg { Start } proc Main mailbox bounded(1) { type State = MainState; type Msg = MainMsg; fn init() -> MainState ! [] ~ [] @det { return MainState { value: b"é" }; } fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); } }"#,
            "raw data must be printable ASCII or escaped",
        ),
    ] {
        let err = parse_source(source).expect_err("malformed primitive literal should fail parse");
        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }

    let oversized = "a".repeat(MAX_PRIMITIVE_DATA_BYTES + 1);
    let source = format!(
        r#"module oversized_string;
record MainState {{ value: String }}
enum MainMsg {{ Start }}
proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState {{ value: "{oversized}" }}; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{ return Stop(state); }}
}}
"#
    );
    let err = parse_source(&source).expect_err("oversized string literal should fail parse");
    assert!(
        err.to_string()
            .contains("String literal exceeds maximum primitive data length"),
        "unexpected error: {err}"
    );

    let oversized_bytes = "\\x61".repeat(MAX_PRIMITIVE_DATA_BYTES + 1);
    let source = format!(
        r#"module oversized_bytes;
record MainState {{ value: Bytes }}
enum MainMsg {{ Start }}
proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState {{ value: b"{oversized_bytes}" }}; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{ return Stop(state); }}
}}
"#
    );
    let err = parse_source(&source).expect_err("oversized bytes literal should fail parse");
    assert!(
        err.to_string()
            .contains("Bytes literal exceeds maximum primitive data length"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_payload_enum_variants_that_collide_with_primitive_value_labels() {
    for (source, expected_variant) in [
        (
            r#"module primitive_string_variant; enum Payload { String(String), Other } enum MainMsg { Start } proc Main mailbox bounded(1) { type State = Payload; type Msg = MainMsg; fn init() -> Payload ! [] ~ [] @det { return Other; } fn step(state: Payload, Start) -> ProcResult<Payload> ! [] ~ [] @det { return Stop(state); } }"#,
            "String",
        ),
        (
            r#"module primitive_bytes_variant; enum Payload { Bytes(Bytes), Other } enum MainMsg { Start } proc Main mailbox bounded(1) { type State = Payload; type Msg = MainMsg; fn init() -> Payload ! [] ~ [] @det { return Other; } fn step(state: Payload, Start) -> ProcResult<Payload> ! [] ~ [] @det { return Stop(state); } }"#,
            "Bytes",
        ),
    ] {
        let err = check_source(source)
            .expect_err("payload enum variants must not collide with primitive value labels");
        let message = err.to_string();
        assert!(
            message.contains("payload-bearing variant") && message.contains(expected_variant),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn rejects_redeclaring_string_and_bytes_types() {
    for source in [
        r#"module redeclare_string; record String; record MainState; enum MainMsg { Start } proc Main mailbox bounded(1) { type State = MainState; type Msg = MainMsg; fn init() -> MainState ! [] ~ [] @det { return MainState; } fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); } }"#,
        r#"module redeclare_bytes; enum Bytes { Value } record MainState; enum MainMsg { Start } proc Main mailbox bounded(1) { type State = MainState; type Msg = MainMsg; fn init() -> MainState ! [] ~ [] @det { return MainState; } fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det { return Stop(state); } }"#,
    ] {
        let err = check_source(source).expect_err("built-in primitive type names are reserved");
        assert!(
            err.to_string().contains("type name"),
            "unexpected error: {err}"
        );
    }
}
