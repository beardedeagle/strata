use super::support::*;

#[test]
fn validate_accepts_process_ref_type_id_at_boundary() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload(
        "Assign",
        PROCESS_REF_WORKER,
    )];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ProcessRef {
            ty: PROCESS_REF_WORKER,
            target_process: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }),
    };

    artifact
        .validate()
        .expect("process reference type IDs should validate through the type table");
}

#[test]
fn validate_rejects_received_payload_send_target_with_non_process_ref_type() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: "Job".to_string(),
        }),
    };
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::Send {
        target: ArtifactSendTarget::ReceivedPayload {
            ty: JOB,
            target_process: ProcessId::new(1),
        },
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: "Job".to_string(),
        }),
    }];
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Send];

    let err = artifact
        .validate()
        .expect_err("received payload send target must require ProcessRef type");

    assert!(err.to_string().contains(
        "artifact field send target payload type type id 4 must be a process reference type"
    ));
}

#[test]
fn validate_rejects_process_ref_template_with_non_process_ref_type() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ProcessRef {
            ty: JOB,
            target_process: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("process reference payload template must require ProcessRef type");

    assert!(err.to_string().contains(
        "artifact field process reference payload type type id 4 must be a process reference type"
    ));
}

#[test]
fn validate_rejects_process_ref_template_target_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Assign", PROCESS_REF_MAIN)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ProcessRef {
            ty: PROCESS_REF_MAIN,
            target_process: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("process reference type target mismatch should fail");

    assert!(err.to_string().contains(
        "artifact field process reference payload type type id 8 targets process id 0, expected 1"
    ));
}

#[test]
fn validate_rejects_received_payload_send_target_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload(
        "Assign",
        PROCESS_REF_WORKER,
    )];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ProcessRef {
            ty: PROCESS_REF_WORKER,
            target_process: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }),
    };
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::Send {
        target: ArtifactSendTarget::ReceivedPayload {
            ty: PROCESS_REF_WORKER,
            target_process: ProcessId::new(0),
        },
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ProcessRef {
            ty: PROCESS_REF_WORKER,
            target_process: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }),
    }];
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Send];

    let err = artifact
        .validate()
        .expect_err("received process reference target mismatch should fail");

    assert!(err.to_string().contains(
        "artifact field send target payload type type id 9 targets process id 1, expected 0"
    ));
}

#[test]
fn validate_rejects_nested_process_ref_payload_template() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", BOX)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Record {
            ty: BOX,
            fields: vec![ArtifactValueTemplateField {
                name: "reply_to".to_string(),
                value: ArtifactValueTemplate::ProcessRef {
                    ty: PROCESS_REF_WORKER,
                    target_process: ProcessId::new(1),
                    process_ref: ProcessRefId::new(0),
                },
            }],
        }),
    };

    let err = artifact
        .validate()
        .expect_err("nested process reference template should fail");

    assert!(err.to_string().contains(
        "process Main transition 0 send payload.field.reply_to process reference template must be a direct message payload"
    ));
}

#[test]
fn validate_rejects_projected_process_ref_payload_template() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload(
        "Assign",
        PROCESS_REF_WORKER,
    )];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::RecordField {
            ty: PROCESS_REF_WORKER,
            record: Box::new(ArtifactValueTemplate::Literal {
                ty: BOX,
                value: "Box{reply_to:ProcessRef_Worker}".to_string(),
            }),
            field: "reply_to".to_string(),
        }),
    };

    let err = artifact
        .validate()
        .expect_err("projected process reference template should fail");

    assert!(err.to_string().contains(
        "process Main transition 0 send payload process reference template must be a direct message payload"
    ));
}

#[test]
fn validate_rejects_received_payload_template_without_payload_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants = vec![ArtifactMessageVariant::payload("Assign", JOB)];
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::ReceivedPayload { ty: JOB }),
    };

    let err = artifact
        .validate()
        .expect_err("received payload template from unit transition should fail");

    assert!(err.to_string().contains(
        "process Main transition 0 send payload requires a payload-bearing transition message"
    ));
}

#[test]
fn validate_rejects_process_ref_payload_enum_next_state_template() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0] =
        ArtifactMessageVariant::payload("Route", PROCESS_REF_WORKER);
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: WORKER_STATE,
            variant: "Routed".to_string(),
            payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: PROCESS_REF_WORKER,
            }),
        });

    let err = artifact
        .validate()
        .expect_err("process ref payload next-state template should fail");

    assert!(err.to_string().contains(
        "process Worker message id 0 next_state_template.payload process reference template must be a direct message payload"
    ));
}
