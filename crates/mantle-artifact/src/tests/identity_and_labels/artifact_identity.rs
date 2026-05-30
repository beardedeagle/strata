use super::*;

#[test]
fn validate_accepts_language_neutral_source_language() {
    let mut artifact = valid_artifact();
    artifact.source_language = "lattice".into();
    artifact.target_requirements.source_language = "lattice".into();

    artifact
        .validate()
        .expect("artifact source language should be language-neutral");

    let decoded = MantleArtifact::decode(&artifact.encode())
        .expect("language-neutral artifact should decode");
    assert_eq!(decoded.source_language.as_ref(), "lattice");
    assert_eq!(
        decoded.target_requirements.source_language.as_ref(),
        "lattice"
    );
}

#[test]
fn validate_rejects_invalid_source_language_identifier() {
    let mut artifact = valid_artifact();
    artifact.source_language = "not-valid".into();

    let err = artifact
        .validate()
        .expect_err("invalid source language should fail");

    assert!(
        err.to_string()
            .contains("artifact field source_language must be an identifier")
    );
}
