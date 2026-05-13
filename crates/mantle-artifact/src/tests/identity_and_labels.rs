use super::support::*;

#[test]
fn validate_accepts_language_neutral_source_language() {
    let mut artifact = valid_artifact();
    artifact.source_language = "lattice".to_string();

    artifact
        .validate()
        .expect("artifact source language should be language-neutral");

    let decoded = MantleArtifact::decode(&artifact.encode())
        .expect("language-neutral artifact should decode");
    assert_eq!(decoded.source_language, "lattice");
}

#[test]
fn validate_rejects_invalid_source_language_identifier() {
    let mut artifact = valid_artifact();
    artifact.source_language = "not-valid".to_string();

    let err = artifact
        .validate()
        .expect_err("invalid source language should fail");

    assert!(
        err.to_string()
            .contains("artifact field source_language must be an identifier")
    );
}

#[test]
fn validate_accepts_structured_state_value_labels() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_values = state_values(
        MAIN_STATE,
        &["MainState{phase:Idle}", "MainState{phase:Handled}"],
    );
    artifact.processes[0].transitions[0].next_state = NextState::Value(StateId::new(1));

    artifact
        .validate()
        .expect("structured state labels should validate");

    let decoded =
        MantleArtifact::decode(&artifact.encode()).expect("structured labels should decode");
    assert_eq!(
        decoded.processes[0].state_values,
        artifact.processes[0].state_values
    );
}

#[test]
fn artifact_state_value_rejects_mismatched_ordered_label() {
    let err = ArtifactStateValue::with_label(MAIN_STATE, artifact_value("MainState"), "Spoofed")
        .expect_err("state labels must match typed state values");

    assert!(
        err.to_string()
            .contains("state value label Spoofed does not match ordered value label MainState"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_state_value_label_defines_artifact_metadata_boundary() {
    validate_state_value_label("MainState{phase:Idle}")
        .expect("structured state labels should be valid artifact metadata");

    for (value, expected) in [
        (
            "",
            "state values must be non-empty and contain no control characters",
        ),
        (
            "MainState\n",
            "state values must be non-empty and contain no control characters",
        ),
    ] {
        let err = validate_state_value_label(value).expect_err("invalid label should fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }

    let oversized = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let err = validate_state_value_label(&oversized).expect_err("oversized label should fail");
    assert!(
        err.to_string()
            .contains("state value exceeds maximum length")
    );
}

#[test]
fn validate_payload_value_label_defines_artifact_metadata_boundary() {
    validate_payload_value_label("Job{phase:Ready}")
        .expect("structured payload labels should be valid artifact metadata");

    for (value, expected) in [
        (
            "",
            "payload value must be non-empty and contain no control characters",
        ),
        (
            "Job\n",
            "payload value must be non-empty and contain no control characters",
        ),
    ] {
        let err = validate_payload_value_label(value).expect_err("invalid label should fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }

    let oversized = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let err = validate_payload_value_label(&oversized).expect_err("oversized label should fail");
    assert!(
        err.to_string()
            .contains("payload value exceeds maximum length")
    );
}

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
fn projection_helpers_reject_duplicate_record_fields() {
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
fn projection_helpers_reject_duplicate_map_keys() {
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

#[test]
fn map_projection_rejects_duplicate_expected_keys() {
    let map =
        ArtifactValue::parse("Map[Done=>Ready,Ready=>Done]").expect("test map value should parse");
    let key = ArtifactValue::parse("Ready").expect("test key should parse");
    let keys = vec![key.clone(), key.clone()];

    let err = map
        .project_map_value(&key, &keys, MapProjectionMode::Exact)
        .expect_err("duplicate projection keys must fail closed");

    assert!(
        err.to_string()
            .contains("map projection duplicates expected map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn map_rest_projection_rejects_duplicate_excluded_keys() {
    let map =
        ArtifactValue::parse("Map[Done=>Ready,Ready=>Done]").expect("test map value should parse");
    let key = ArtifactValue::parse("Ready").expect("test key should parse");
    let keys = vec![key.clone(), key];

    let err = map
        .project_map_rest(&keys)
        .expect_err("duplicate rest projection keys must fail closed");

    assert!(
        err.to_string()
            .contains("map rest projection duplicates excluded map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn map_rest_projection_reports_missing_excluded_key_precisely() {
    let map = ArtifactValue::parse("Map[Done=>Ready]").expect("test map value should parse");
    let keys = vec![ArtifactValue::parse("Ready").expect("test key should parse")];

    let err = map
        .project_map_rest(&keys)
        .expect_err("missing excluded rest key must fail closed");

    assert!(
        err.to_string()
            .contains("map rest projection expected excluded map key Ready, found [Done]"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_prefix_projection_returns_prefix_element_from_longer_list() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let element = list
        .project_list_prefix_element(0, 1)
        .expect("list prefix projection should produce prefix element");

    assert_eq!(element.label(), "Ready");
}

#[test]
fn list_prefix_projection_rejects_index_outside_prefix() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let err = list
        .project_list_prefix_element(1, 1)
        .expect_err("outside-prefix list element should fail closed");

    assert!(
        err.to_string()
            .contains("list prefix projection index 1 is outside prefix length 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_rest_projection_constructs_suffix_list() {
    let list = ArtifactValue::parse("List[Ready,Done]").expect("test list value should parse");

    let rest = list
        .project_list_rest(1)
        .expect("list rest projection should produce suffix");

    assert_eq!(rest.label(), "List[Done]");
}

#[test]
fn list_rest_projection_rejects_zero_prefix() {
    let list = ArtifactValue::parse("List[Ready]").expect("test list value should parse");

    let err = list
        .project_list_rest(0)
        .expect_err("zero prefix list rest should fail closed");

    assert!(
        err.to_string()
            .contains("list rest projection requires at least one prefix element"),
        "unexpected error: {err}"
    );
}

#[test]
fn list_rest_projection_reports_short_list_precisely() {
    let list = ArtifactValue::parse("List[Ready]").expect("test list value should parse");

    let err = list
        .project_list_rest(2)
        .expect_err("short list rest projection should fail closed");

    assert!(
        err.to_string().contains(
            "list rest projection requires at least 2 prefix elements, found 1 in List[Ready]"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn artifact_value_validate_rejects_invalid_shape_before_materializing_label() {
    let value = ArtifactValue::List(vec![
        ArtifactValue::Atom("A".repeat(MAX_IDENTIFIER_BYTES));
        MAX_VALUE_TEMPLATE_FIELDS + 1
    ]);

    let err = value
        .validate("oversized list")
        .expect_err("oversized list shape should fail before label validation");

    assert!(
        err.to_string()
            .contains("oversized list.item_count must be no greater than"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_rejects_programmatic_state_value_ordered_label_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[0].state_values[0] = ArtifactStateValue {
        ty: MAIN_STATE,
        value: artifact_value("MainState"),
        label: "Spoofed".to_string(),
        payload: None,
    };

    let err = artifact
        .validate()
        .expect_err("programmatic state label mismatch should fail validation");

    assert!(
        err.to_string()
            .contains("state value label Spoofed does not match ordered value label MainState"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_rejects_programmatic_invalid_state_value_shape() {
    let mut artifact = valid_artifact();
    let invalid = ArtifactValue::Atom("not-valid".to_string());
    artifact.processes[0].state_values[0] = ArtifactStateValue {
        ty: MAIN_STATE,
        value: invalid,
        label: "not-valid".to_string(),
        payload: None,
    };

    let err = artifact
        .validate()
        .expect_err("programmatic invalid state value should fail validation");

    assert!(
        err.to_string()
            .contains("artifact field state value must be an identifier"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_rejects_programmatic_empty_record_state_value_shape() {
    let mut artifact = valid_artifact();
    let invalid = ArtifactValue::Record {
        constructor: "MainState".to_string(),
        fields: Vec::new(),
    };
    artifact.processes[0].state_values[0] = ArtifactStateValue {
        ty: MAIN_STATE,
        value: invalid,
        label: "MainState{}".to_string(),
        payload: None,
    };

    let err = artifact
        .validate()
        .expect_err("empty record state value should fail artifact validation");

    assert!(
        err.to_string()
            .contains("state value.field_count must be greater than zero"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_rejects_programmatic_invalid_literal_template_shape() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: MAIN_STATE,
            value: ArtifactValue::Atom("not-valid".to_string()),
        });

    let err = artifact
        .validate()
        .expect_err("programmatic invalid literal template should fail validation");

    assert!(
        err.to_string()
            .contains("next_state_template must be an identifier"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_rejects_invalid_type_label_identifier() {
    let mut artifact = valid_artifact();
    artifact.types[JOB.index()].label = "not-valid".to_string();

    let err = artifact
        .validate()
        .expect_err("invalid type metadata label should fail");

    assert!(
        err.to_string()
            .contains("artifact field type.4.label must be an identifier")
    );
}

#[test]
fn validate_rejects_unknown_type_table_target_process() {
    let mut artifact = valid_artifact();
    artifact.types[PROCESS_REF_WORKER.index()].kind = ArtifactTypeKind::ProcessRef {
        target: ProcessId::new(99),
    };

    let err = artifact
        .validate()
        .expect_err("process reference type target should be bounded");

    assert!(
        err.to_string()
            .contains("type id 9 targets undefined process id 99")
    );
}

#[test]
fn validate_rejects_encoded_artifacts_above_size_limit() {
    let mut artifact = valid_artifact();
    let text = "a".repeat(MAX_FIELD_VALUE_BYTES);
    artifact.outputs = (0..70).map(|_| text.clone()).collect();

    let err = artifact
        .validate()
        .expect_err("encoded artifact size should be bounded");

    assert!(
        err.to_string()
            .contains("encoded artifact exceeds maximum size")
    );
}

#[test]
fn validate_rejects_duplicate_process_debug_names() {
    let mut artifact = valid_artifact();
    artifact.processes[1].debug_name = "Main".to_string();

    let err = artifact
        .validate()
        .expect_err("duplicate debug labels should fail");

    assert!(
        err.to_string()
            .contains("duplicate process debug_name Main")
    );
}

#[test]
fn validate_treats_debug_names_as_metadata_not_targets() {
    let mut artifact = valid_artifact();
    artifact.processes[1].debug_name = "RenamedWorker".to_string();

    artifact
        .validate()
        .expect("renaming debug metadata should not affect indexed references");
}
