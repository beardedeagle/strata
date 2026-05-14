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
            value: artifact_value("Job{phase:Ready}"),
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
fn validate_accepts_payload_guarded_transitions_as_typed_artifact_metadata() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Ready}")),
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Done}")),
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    artifact
        .validate()
        .expect("payload-guarded transitions should validate");
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id=4"));
    assert!(encoded.contains(".payload_guard_value=Job{phase:Ready}"));
    assert!(encoded.contains(".payload_guard_value=Job{phase:Done}"));

    let decoded = MantleArtifact::decode(&encoded).expect("payload guards should decode");
    assert_eq!(
        decoded.processes[1].transitions[0].payload_guard,
        artifact.processes[1].transitions[0].payload_guard
    );
    assert_eq!(
        decoded.processes[1].transitions[1].payload_guard,
        artifact.processes[1].transitions[1].payload_guard
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
fn validate_rejects_payload_guard_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].transitions[0].payload_guard =
        Some(artifact_payload(OTHER_JOB, "OtherJob{phase:Ready}"));

    let err = artifact
        .validate()
        .expect_err("transition payload guard type mismatch should fail");

    assert!(err.to_string().contains(
        "process Worker transition message id 0 payload guard has type id 5, expected 4"
    ));
}

#[test]
fn validate_rejects_process_ref_payload_guard_sidecar() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].transitions[0].payload_guard = Some(ArtifactPayload {
        ty: JOB,
        value: artifact_value("Job{phase:Ready}"),
        process_ref: Some(ArtifactProcessRefPayload {
            target_process: ProcessId::new(1),
            pid: 7,
        }),
    });

    let err = artifact
        .validate()
        .expect_err("transition payload guard process-ref sidecar should fail");

    assert!(err.to_string().contains(
        "process Worker transition message id 0 payload guard cannot be a process reference payload"
    ));
}

#[test]
fn validate_rejects_mixed_payload_guarded_and_unguarded_transitions_for_same_base() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].transitions.push(ArtifactTransition {
        current_state: None,
        message: MessageId::new(0),
        payload_guard: Some(artifact_payload(JOB, "Job{phase:Ready}")),
        step_result: StepResult::Stop,
        next_state: NextState::Current,
        effects: Vec::new(),
        actions: Vec::new(),
    });

    let err = artifact
        .validate()
        .expect_err("mixed payload-guarded and unguarded transitions should fail");

    assert!(err.to_string().contains(
        "process Worker mixes payload-guarded and unguarded transitions for message id 0 current_state None"
    ));
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
            value: artifact_value("Job{phase:Ready}"),
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
            value: artifact_value("OtherJob{phase:Ready}"),
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
