use super::support::*;

#[test]
fn validate_accepts_sorted_typed_target_requirements() {
    let artifact = valid_artifact();

    artifact
        .validate()
        .expect("fixture should declare valid target requirements");

    assert_eq!(
        artifact.target_requirements.source_language.as_ref(),
        TEST_SOURCE_LANGUAGE
    );
    assert!(
        artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::LocalSpawn)
    );
}

#[test]
fn validate_rejects_source_language_mismatch() {
    let mut artifact = valid_artifact();
    artifact.target_requirements.source_language = "other_frontend".into();

    let err = artifact
        .validate()
        .expect_err("source language mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("target requirements source_language")
    );
}

#[test]
fn validate_rejects_duplicate_target_requirement() {
    let mut artifact = valid_artifact();
    artifact
        .target_requirements
        .features
        .push(RuntimeFeature::TypedValueTemplates);

    let err = artifact
        .validate()
        .expect_err("duplicate target requirement should fail closed");

    assert!(
        err.to_string()
            .contains("duplicate target requirement runtime feature typed_value_templates")
    );
}

#[test]
fn validate_rejects_unsorted_target_requirements() {
    let mut artifact = valid_artifact();
    artifact.target_requirements.features = vec![
        RuntimeFeature::LocalExecution,
        RuntimeFeature::BoundedMailbox,
    ];

    let err = artifact
        .validate()
        .expect_err("unsorted target requirements should fail closed");

    assert!(
        err.to_string()
            .contains("target requirement runtime features must be sorted")
    );
}

#[test]
fn validate_rejects_underdeclared_target_requirements() {
    let mut artifact = valid_artifact();
    artifact.target_requirements =
        ArtifactTargetRequirements::new(TEST_SOURCE_LANGUAGE, vec![RuntimeFeature::BoundedMailbox]);

    let err = artifact
        .validate()
        .expect_err("underdeclared target requirements should fail closed");

    assert!(
        err.to_string()
            .contains("target requirements do not declare required runtime feature jsonl_trace"),
        "{err}"
    );
}

#[test]
fn validate_checks_artifact_shape_before_target_requirement_coverage() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.target_requirements =
        ArtifactTargetRequirements::new(TEST_SOURCE_LANGUAGE, vec![RuntimeFeature::BoundedMailbox]);
    artifact.processes[1].transitions[0].actions = vec![nested_if_else_action(
        MAX_VALUE_TEMPLATE_DEPTH + 1,
        bool_type,
    )];

    let err = artifact
        .validate()
        .expect_err("overly nested artifact should fail structural validation first");

    assert!(err.to_string().contains(&format!(
        "artifact action nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
    )));
}

#[test]
fn target_requirements_canonicalize_feature_insertion_order() {
    let left = ArtifactTargetRequirements::new(
        TEST_SOURCE_LANGUAGE,
        vec![
            RuntimeFeature::LocalSpawn,
            RuntimeFeature::BoundedMailbox,
            RuntimeFeature::LocalSend,
        ],
    );
    let right = ArtifactTargetRequirements::new(
        TEST_SOURCE_LANGUAGE,
        vec![
            RuntimeFeature::LocalSend,
            RuntimeFeature::LocalSpawn,
            RuntimeFeature::BoundedMailbox,
            RuntimeFeature::LocalSpawn,
        ],
    );

    assert_eq!(left, right);
    assert_eq!(
        left.features,
        vec![
            RuntimeFeature::BoundedMailbox,
            RuntimeFeature::LocalSend,
            RuntimeFeature::LocalSpawn,
        ]
    );
}

#[test]
fn render_target_requirements_is_deterministic() {
    let artifact = valid_artifact();
    let text = render_artifact_target_requirements(
        &artifact,
        "fixture.mta",
        TargetRequirementsFormat::Text,
    )
    .expect("requirements should render");
    let json = render_artifact_target_requirements(
        &artifact,
        "fixture.mta",
        TargetRequirementsFormat::Json,
    )
    .expect("requirements should render");

    assert!(text.contains("mantle target requirements fixture.mta"));
    assert!(text.contains("  - bounded_mailbox"));
    assert!(json.contains("\"features\":[\"bounded_mailbox\""));
    assert_eq!(
        text,
        render_artifact_target_requirements(
            &artifact,
            "fixture.mta",
            TargetRequirementsFormat::Text
        )
        .expect("requirements should render deterministically")
    );
}

#[test]
fn render_target_requirements_json_escapes_control_chars_precisely() {
    let artifact = valid_artifact();

    let json = render_artifact_target_requirements(
        &artifact,
        "fixture\u{0001}\u{001f}\n.mta",
        TargetRequirementsFormat::Json,
    )
    .expect("requirements should render");

    assert!(json.contains("\"target\":\"fixture\\u0001\\u001f\\n.mta\""));
}
