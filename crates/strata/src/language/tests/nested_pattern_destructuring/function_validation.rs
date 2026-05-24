use super::super::support::*;
use super::shared::*;

#[test]
fn rejects_duplicate_payload_sensitive_function_predicate() {
    let source = payload_sensitive_function_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Ready)) => {
            return Ready;
        }
    }
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("duplicate nested predicate should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope(Assign(Ready)) overlaps an earlier pattern"),
        "expected duplicate nested predicate diagnostic, got {err}"
    );
}

#[test]
fn rejects_guarded_and_unguarded_function_constructor_overlap() {
    let source = payload_sensitive_function_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope => {
            return Done;
        }
    };
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("guarded and unguarded overlap should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope overlaps an earlier pattern"),
        "expected guarded/unguarded overlap diagnostic, got {err}"
    );
}

#[test]
fn rejects_payload_sensitive_function_predicates_that_are_not_provably_disjoint() {
    let source = payload_sensitive_function_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(phase: Phase)) => {
            return phase;
        }
        Envelope(Assign(Ready)) => {
            return Ready;
        }
    }
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("unproven predicate disjointness should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope(Assign(Ready)) overlaps an earlier pattern"),
        "expected unproven overlap diagnostic, got {err}"
    );
}

#[test]
fn rejects_uncovered_payload_sensitive_function_predicate_at_expansion_time() {
    let source = payload_sensitive_function_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    }
}
"#,
        "MainState { phase: route(Envelope(Assign(Other))) }",
    );

    let err = check_source(&source).expect_err("uncovered nested predicate should fail");
    assert!(
        err.to_string()
            .contains("function route match has no matching pattern for Envelope(Assign(Other))"),
        "expected uncovered nested predicate diagnostic, got {err}"
    );
}

#[test]
fn source_functions_reject_fieldless_nested_enum_constructor_mismatches() {
    for (selected_call, expected) in [
        (
            "fieldless_signature(Mark(Done))",
            "function fieldless_signature signature nested payload pattern does not match concrete Done",
        ),
        (
            "fieldless_body(Mark(Done))",
            "function fieldless_body match nested payload pattern does not match concrete Done",
        ),
        (
            "fieldless_return(Mark(Done))",
            "function fieldless_return return match nested payload pattern does not match concrete Done",
        ),
    ] {
        let source = fieldless_function_mismatch_source(selected_call);
        let err = check_source(&source)
            .expect_err("fieldless nested enum constructor function mismatch should fail checking");

        assert!(
            err.to_string().contains(expected),
            "expected diagnostic containing {expected:?}, got {err}"
        );
    }
}
