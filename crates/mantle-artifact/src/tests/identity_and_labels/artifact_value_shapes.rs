use super::*;

#[test]
fn artifact_value_parse_uses_generic_artifact_value_context() {
    let oversized = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let err = ArtifactValue::parse(&oversized).expect_err("oversized artifact value should fail");

    assert!(
        err.to_string()
            .contains("artifact value exceeds maximum length"),
        "unexpected error: {err}"
    );
}

#[test]
fn projection_functions_reject_duplicate_record_fields() {
    let err = ArtifactValue::parse("Job{phase:Ready,phase:Done}")
        .expect_err("duplicate record fields must fail closed");

    assert!(
        err.to_string().contains("duplicates field phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn artifact_value_parse_rejects_empty_record_values() {
    let err =
        ArtifactValue::parse("MainState{}").expect_err("empty record values must use atom syntax");

    assert!(
        err.to_string().contains(
            "fieldless record values use MainState; braced record values must declare at least one field"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn artifact_value_parse_rejects_unbalanced_top_level_delimiters() {
    for (label, expected) in [
        ("Unexpected)Record{phase:Ready}", "unbalanced parentheses"),
        ("Unexpected]Record{phase:Ready}", "unbalanced brackets"),
        ("Unexpected}Record{phase:Ready}", "unbalanced braces"),
    ] {
        let err = ArtifactValue::parse(label).expect_err("unbalanced value labels must fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn artifact_value_labels_preserve_record_and_map_entry_order() {
    let record = ArtifactValue::parse("MainState{signature:WarmReady,body:WarmReady}")
        .expect("ordered record value should parse");
    assert_eq!(
        record.label(),
        "MainState{signature:WarmReady,body:WarmReady}"
    );

    let map = ArtifactValue::parse("Map[Ready=>Done,Done=>Ready]")
        .expect("ordered map value should parse");
    assert_eq!(map.label(), "Map[Ready=>Done,Done=>Ready]");
}

#[test]
fn artifact_value_validate_rejects_empty_record_shape() {
    let value = ArtifactValue::Record {
        constructor: "MainState".to_string(),
        fields: Vec::new(),
    };

    let err = value
        .validate("empty record")
        .expect_err("programmatic empty record values must fail closed");

    assert!(
        err.to_string()
            .contains("empty record.field_count must be greater than zero"),
        "unexpected error: {err}"
    );
}

#[test]
fn artifact_value_validate_rejects_programmatic_duplicate_record_fields() {
    let value = ArtifactValue::Record {
        constructor: "MainState".to_string(),
        fields: vec![
            ArtifactRecordField {
                name: "phase".to_string(),
                value: artifact_value("Ready"),
            },
            ArtifactRecordField {
                name: "phase".to_string(),
                value: artifact_value("Done"),
            },
        ],
    };

    let err = value
        .validate("record value")
        .expect_err("programmatic duplicate record fields must fail closed");

    assert!(
        err.to_string()
            .contains("record value duplicates field phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn projection_functions_reject_duplicate_map_keys() {
    let err = ArtifactValue::parse("Map[Ready=>Ready,Ready=>Done]")
        .expect_err("duplicate map keys must fail closed");

    assert!(
        err.to_string().contains("duplicates key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn artifact_value_validate_rejects_programmatic_duplicate_map_keys() {
    let value = ArtifactValue::Map(vec![
        ArtifactMapEntry {
            key: artifact_value("Ready"),
            value: artifact_value("Ready"),
        },
        ArtifactMapEntry {
            key: artifact_value("Ready"),
            value: artifact_value("Done"),
        },
    ]);

    let err = value
        .validate("map value")
        .expect_err("programmatic duplicate map keys must fail closed");

    assert!(
        err.to_string().contains("map value duplicates key Ready"),
        "unexpected error: {err}"
    );
}
