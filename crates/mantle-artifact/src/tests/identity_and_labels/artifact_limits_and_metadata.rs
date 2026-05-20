use super::*;

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
