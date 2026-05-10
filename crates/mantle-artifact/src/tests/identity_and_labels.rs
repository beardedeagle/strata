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
        .expect("structured state labels should remain display metadata");

    let decoded =
        MantleArtifact::decode(&artifact.encode()).expect("structured labels should decode");
    assert_eq!(
        decoded.processes[0].state_values,
        artifact.processes[0].state_values
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
fn projection_helpers_reject_duplicate_record_fields() {
    let err = project_canonical_record_field("Job{phase:Ready,phase:Done}", "phase")
        .expect_err("duplicate record fields must fail closed");

    assert!(
        err.to_string().contains("duplicates field phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn projection_helpers_reject_duplicate_map_keys() {
    let keys = vec!["Ready".to_string()];
    let err = project_canonical_map_value("Map[Ready=>Ready,Ready=>Done]", "Ready", &keys)
        .expect_err("duplicate map keys must fail closed");

    assert!(
        err.to_string().contains("duplicates key Ready"),
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
