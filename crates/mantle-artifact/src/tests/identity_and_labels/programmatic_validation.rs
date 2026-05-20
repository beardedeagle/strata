use super::*;

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
fn validate_rejects_programmatic_invalid_enum_payload_template_variant() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EnumPayload {
            ty: MAIN_STATE,
            value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Idle"),
            }),
            variant: EnumVariantId::new(99),
        });

    let err = artifact
        .validate()
        .expect_err("invalid enum payload projection variant should fail validation");

    assert!(
        err.to_string().contains(
            "next_state_template.variant_id artifact type id 2 has no enum variant id 99"
        ),
        "unexpected error: {err}"
    );
}
