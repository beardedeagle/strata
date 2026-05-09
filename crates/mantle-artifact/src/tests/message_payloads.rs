use super::support::*;

#[test]
fn validate_accepts_payload_message_metadata() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: "Job{phase:Ready}".to_string(),
        }),
    };

    artifact
        .validate()
        .expect("payload message labels should remain separate from typed payloads");

    let decoded =
        MantleArtifact::decode(&artifact.encode()).expect("payload metadata should decode");
    assert_eq!(
        decoded.processes[1].message_variants,
        artifact.processes[1].message_variants
    );
}

#[test]
fn validate_rejects_unknown_message_payload_type_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Assign", TypeId::new(99))];

    let err = artifact
        .validate()
        .expect_err("unknown message payload type should fail artifact validation");

    assert!(
        err.to_string()
            .contains("process Worker message Assign payload_type_id 99 is invalid: artifact type id 99 is not defined")
    );
}

#[test]
fn validate_rejects_missing_required_send_payload() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];

    let err = artifact
        .validate()
        .expect_err("missing send payload should fail");

    assert!(
        err.to_string()
            .contains("process Main sends process id 1 message id 0 without required payload")
    );
}

#[test]
fn validate_rejects_payload_for_unit_message_variant() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: "Job{phase:Ready}".to_string(),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("payload sent to unit message should fail");

    assert!(err.to_string().contains(
        "process Main sends payload to process id 1 message id 0, which does not accept one"
    ));
}

#[test]
fn validate_rejects_send_payload_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: OTHER_JOB,
            value: "OtherJob{phase:Ready}".to_string(),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("wrong payload type should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 send payload has type id 5, expected 4")
    );
}
