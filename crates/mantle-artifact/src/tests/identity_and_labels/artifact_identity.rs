use super::*;

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
