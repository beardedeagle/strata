use super::*;

#[test]
fn validate_rejects_enum_variant_metadata_above_artifact_limit() {
    let mut artifact = valid_artifact();
    artifact.types[MAIN_MSG.index()] = ArtifactType::enum_value(
        "MainMsg",
        (0..=MAX_ENUM_VARIANTS_PER_TYPE)
            .map(|index| format!("V{index}"))
            .collect(),
    );

    let err = artifact
        .validate()
        .expect_err("enum variant metadata above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "type.{}.enum_variant_count must be no greater than {MAX_ENUM_VARIANTS_PER_TYPE}",
        MAIN_MSG.index()
    )));
}

#[test]
fn validate_rejects_state_value_outside_declared_enum_variants() {
    let mut artifact = valid_artifact();
    artifact.processes[1].state_values[0] = state_value(WORKER_STATE, "Bogus");

    let err = artifact
        .validate()
        .expect_err("state value outside enum variants should fail admission");

    assert!(
        err.to_string()
            .contains("state value value Bogus is not a member of enum type WorkerState"),
        "{err}"
    );
}

#[test]
fn validate_rejects_literal_payload_outside_declared_enum_variants() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0] =
        ArtifactMessageVariant::payload("Ping", WORKER_STATE);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: None,
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: artifact_value("Bogus"),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("literal payload outside enum variants should fail admission");

    assert!(
        err.to_string()
            .contains("send payload value Bogus is not a member of enum type WorkerState"),
        "{err}"
    );
}

#[test]
fn validate_accepts_structured_state_value_labels() {
    let mut artifact = valid_artifact();
    artifact.types[MAIN_STATE.index()] = ArtifactType::record(
        "MainState",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    );
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
fn validate_rejects_record_field_value_with_wrong_declared_type() {
    let mut artifact = valid_artifact();
    artifact.types[MAIN_STATE.index()] = ArtifactType::record(
        "MainState",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    );
    artifact.processes[0].state_values = state_values(MAIN_STATE, &["MainState{phase:MainState}"]);

    let err = artifact
        .validate()
        .expect_err("record field with wrong typed value should fail");

    assert!(
        err.to_string().contains(
            "state value.field.phase value MainState is not a member of enum type WorkerState"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_list_element_value_with_wrong_declared_type() {
    let mut artifact = valid_artifact();
    artifact.types[JOB.index()] = ArtifactType::list("JobList", WORKER_STATE, 2);
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = state_values(JOB, &["List[Idle,MainState]"]);

    let err = artifact
        .validate()
        .expect_err("list element with wrong typed value should fail");

    assert!(
        err.to_string().contains(
            "state value.item.1 value MainState is not a member of enum type WorkerState"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_map_value_with_wrong_declared_type() {
    let mut artifact = valid_artifact();
    artifact.types[JOB.index()] = ArtifactType::map("JobMap", WORKER_STATE, WORKER_STATE, 2);
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = state_values(JOB, &["Map[Idle=>MainState]"]);

    let err = artifact
        .validate()
        .expect_err("map value with wrong typed value should fail");

    assert!(
        err.to_string().contains(
            "state value.entry.0.value value MainState is not a member of enum type WorkerState"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_duplicate_map_keys_after_typed_key_validation() {
    let mut artifact = valid_artifact();
    artifact.types[JOB.index()] = ArtifactType::map("JobMap", WORKER_STATE, WORKER_STATE, 2);
    artifact.processes[0].state_type = JOB;
    let value = ArtifactValue::Map(vec![
        ArtifactMapEntry {
            key: artifact_value("Idle"),
            value: artifact_value("Handled"),
        },
        ArtifactMapEntry {
            key: artifact_value("Idle"),
            value: artifact_value("Done"),
        },
    ]);
    artifact.processes[0].state_values = vec![ArtifactStateValue {
        ty: JOB,
        label: value.label(),
        value,
        payload: None,
    }];

    let err = artifact
        .validate()
        .expect_err("duplicate typed map keys should fail");

    assert!(
        err.to_string().contains("state value duplicates key Idle"),
        "{err}"
    );
}

#[test]
fn validate_rejects_enum_payload_with_wrong_declared_type() {
    let mut artifact = valid_artifact();
    artifact.types[JOB.index()] = ArtifactType::record(
        "Job",
        vec![ArtifactTypeField {
            name: "phase".to_string(),
            ty: WORKER_STATE,
        }],
    );
    artifact.types[WORKER_STATE.index()] = ArtifactType::enum_value_with_payloads(
        "WorkerState",
        vec![
            ArtifactEnumVariant {
                label: "Idle".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Boxed".to_string(),
                payload_type: Some(JOB),
            },
        ],
    );
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Boxed(MainState)", "Idle"]);

    let err = artifact
        .validate()
        .expect_err("enum payload with wrong typed value should fail");

    assert!(
        err.to_string()
            .contains("state value.payload value MainState does not match record type Job"),
        "{err}"
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
