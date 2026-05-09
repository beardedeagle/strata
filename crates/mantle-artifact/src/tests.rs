use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
const MAIN_STATE: TypeId = TypeId::new(0);
const MAIN_MSG: TypeId = TypeId::new(1);
const WORKER_STATE: TypeId = TypeId::new(2);
const WORKER_MSG: TypeId = TypeId::new(3);
const JOB: TypeId = TypeId::new(4);
const OTHER_JOB: TypeId = TypeId::new(5);
const BOX: TypeId = TypeId::new(6);
const MAIN_PAYLOAD: TypeId = TypeId::new(7);
const PROCESS_REF_MAIN: TypeId = TypeId::new(8);
const PROCESS_REF_WORKER: TypeId = TypeId::new(9);

#[test]
fn artifact_round_trips_and_validates_magic() {
    let artifact = valid_artifact();
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains(&format!("schema_version={ARTIFACT_SCHEMA_VERSION}")));
    assert!(encoded.contains("entry_process=0"));
    assert!(encoded.contains("type.2.label=WorkerState"));
    assert!(encoded.contains("process.1.state_value.1.type_id=2"));
    assert!(encoded.contains("process.1.state_value.1.value=Handled"));
    assert!(encoded.contains("process.1.state_value.1.label=Handled"));
    assert!(encoded.contains("process.0.transition.0.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_value=1"));
    assert!(encoded.contains("process.0.process_ref.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.effect_count=2"));
    assert!(encoded.contains("process.0.transition.0.effect.0=spawn"));
    assert!(encoded.contains("process.0.transition.0.effect.1=send"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.action.0.process_ref=0"));
    assert!(encoded.contains("process.0.transition.0.action.1.target_process_ref=0"));

    let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
    assert!(err.to_string().contains("invalid Mantle artifact magic"));
}

#[test]
fn artifact_round_trips_panic_step_result() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].step_result = StepResult::Panic;

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("panic artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.step_result=Panic"));
}

#[test]
fn decode_rejects_unknown_step_result() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.step_result=Stop",
        "process.1.transition.0.step_result=Crash",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown step result should fail");

    assert!(
        err.to_string()
            .contains("invalid step_result value \"Crash\"")
    );
}

#[test]
fn decode_rejects_unsupported_schema_before_body_fields() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version=0\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unsupported schema should fail first");

    assert!(err.to_string().contains(&format!(
        "unsupported artifact schema version 0; expected {ARTIFACT_SCHEMA_VERSION}"
    )));
}

#[test]
fn decode_reports_duplicate_fields() {
    let encoded = valid_artifact().encode().replace(
        "process.0.debug_name=Main",
        "process.0.debug_name=Main\nprocess.0.debug_name=Other",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

    assert!(
        err.to_string()
            .contains("duplicate artifact field \"process.0.debug_name\"")
    );
}

#[test]
fn decode_reports_unknown_fields() {
    let mut encoded = valid_artifact().encode();
    encoded.push_str("process.0.transition.0.action.0.extra=value\n");

    let err = MantleArtifact::decode(&encoded).expect_err("unknown field should fail");

    assert!(
        err.to_string()
            .contains("unknown artifact field \"process.0.transition.0.action.0.extra\"")
    );
}

#[test]
fn decode_rejects_unbounded_process_count_before_allocation() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version={ARTIFACT_SCHEMA_VERSION}\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("process count should be bounded");

    assert!(
        err.to_string()
            .contains("process_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_nested_counts_before_allocation() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value_count=1",
        &format!(
            "process.0.state_value_count={}",
            MAX_STATE_VALUES_PER_PROCESS + 1
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("state value count should be bounded");

    assert!(
        err.to_string()
            .contains("process.0.state_value_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_transition_current_state_before_validation() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.message=0",
        &format!(
            "process.1.transition.0.current_state={}\nprocess.1.transition.0.message=0",
            MAX_STATE_VALUES_PER_PROCESS
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("current_state id should be bounded");

    assert!(
        err.to_string()
            .contains("process.1.transition.0.current_state must be no greater than")
    );
}

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
fn validate_rejects_aggregate_process_action_count_above_limit() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));
    artifact.processes[1].transitions[0].actions = emit_actions(MAX_ACTIONS_PER_PROCESS / 2);
    artifact.processes[1].transitions.push(ArtifactTransition {
        current_state: None,
        message: MessageId::new(1),
        step_result: StepResult::Stop,
        next_state: NextState::Current,
        effects: vec![ArtifactEffect::Emit],
        actions: emit_actions((MAX_ACTIONS_PER_PROCESS / 2) + 1),
    });

    let err = artifact
        .validate()
        .expect_err("aggregate process action count should be bounded");

    assert!(err.to_string().contains(&format!(
        "action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
}

#[test]
fn validate_rejects_action_without_declared_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];

    let err = artifact
        .validate()
        .expect_err("send without declared send effect should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 uses effect send but does not declare it")
    );
}

#[test]
fn validate_rejects_declared_effect_without_action() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0]
        .effects
        .push(ArtifactEffect::Send);

    let err = artifact
        .validate()
        .expect_err("unused declared effect should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 declares effect send but no action uses it")
    );
}

#[test]
fn validate_rejects_duplicate_transition_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Emit, ArtifactEffect::Emit];

    let err = artifact
        .validate()
        .expect_err("duplicate transition effect should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 declares duplicate effect emit")
    );
}

#[test]
fn decode_rejects_unknown_transition_effect() {
    let encoded = valid_artifact().encode().replace(
        "process.0.transition.0.effect.1=send",
        "process.0.transition.0.effect.1=write",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown effect should fail");

    assert!(
        err.to_string()
            .contains("process.0.transition.0.effect.1: invalid effect value \"write\"")
    );
}

#[test]
fn validate_rejects_unknown_send_message() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(1),
            payload: None,
        });

    let err = artifact
        .validate()
        .expect_err("unknown send message should fail");

    assert!(
        err.to_string()
            .contains("sends message id 1 not accepted by process id 1")
    );
}

#[test]
fn validate_rejects_unknown_send_process_ref() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(99)),
            message: MessageId::new(0),
            payload: None,
        });

    let err = artifact
        .validate()
        .expect_err("unknown send process ref should fail");

    assert!(
        err.to_string()
            .contains("references undefined process reference id 99")
    );
}

#[test]
fn validate_rejects_duplicate_process_ref_name() {
    let mut artifact = valid_artifact();
    artifact.processes[0].process_refs.push(ArtifactProcessRef {
        debug_name: "worker".to_string(),
        target: ProcessId::new(1),
    });

    let err = artifact
        .validate()
        .expect_err("duplicate process reference name should fail");

    assert!(
        err.to_string()
            .contains("duplicate process reference worker")
    );
}

#[test]
fn validate_rejects_process_ref_targeting_entry_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].process_refs = vec![ArtifactProcessRef {
        debug_name: "main".to_string(),
        target: ProcessId::new(0),
    }];

    let err = artifact
        .validate()
        .expect_err("process reference targeting entry process should fail");

    assert!(
        err.to_string()
            .contains("process Worker process reference main targets entry process id 0")
    );
}

#[test]
fn validate_rejects_process_ref_targeting_same_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].process_refs = vec![ArtifactProcessRef {
        debug_name: "self_ref".to_string(),
        target: ProcessId::new(1),
    }];

    let err = artifact
        .validate()
        .expect_err("process reference targeting same process should fail");

    assert!(
        err.to_string()
            .contains("process Worker process reference self_ref targets itself")
    );
}

#[test]
fn validate_rejects_spawn_process_ref_target_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions[0] = ArtifactAction::Spawn {
        target: ProcessId::new(0),
        process_ref: ProcessRefId::new(0),
    };

    let err = artifact
        .validate()
        .expect_err("spawn process reference target mismatch should fail");

    assert!(
        err.to_string()
            .contains("spawn process reference id 0 targets process id 0, expected 1")
    );
}

#[test]
fn validate_rejects_duplicate_spawn_process_ref_with_transition_context() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        });

    let err = artifact
        .validate()
        .expect_err("duplicate spawn process reference should fail");

    assert!(
        err.to_string()
            .contains("duplicates process reference id 0 within message transition 0")
    );
}

#[test]
fn validate_rejects_send_before_process_ref_spawn_with_transition_context() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions.reverse();

    let err = artifact
        .validate()
        .expect_err("send before process reference spawn should fail");

    assert!(err.to_string().contains(
        "process Main sends through unbound process reference id 0 within message transition 0"
    ));
}

#[test]
fn validate_rejects_unknown_spawn_target() {
    let mut artifact = valid_artifact();
    artifact.processes[0].process_refs[0].target = ProcessId::new(99);

    let err = artifact
        .validate()
        .expect_err("unknown spawn target should fail");

    assert!(
        err.to_string()
            .contains("process reference worker targets undefined process id 99")
    );
}

#[test]
fn validate_rejects_unknown_output_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::Emit {
        output: OutputId::new(99),
    }];

    let err = artifact
        .validate()
        .expect_err("unknown output id should fail");

    assert!(err.to_string().contains("emits undefined output id 99"));
}

#[test]
fn validate_rejects_unknown_entry_process_id() {
    let mut artifact = valid_artifact();
    artifact.entry_process = ProcessId::new(99);

    let err = artifact
        .validate()
        .expect_err("unknown entry process id should fail");

    assert!(
        err.to_string()
            .contains("entry process id 99 is not defined")
    );
}

#[test]
fn validate_rejects_payload_bearing_entry_message() {
    let mut artifact = valid_artifact();
    artifact.processes[0].message_variants[0] =
        ArtifactMessageVariant::payload("Start", MAIN_PAYLOAD);

    let err = artifact
        .validate()
        .expect_err("entry payload message should fail");

    assert!(
        err.to_string()
            .contains("entry message id 0 must not require a payload")
    );
}

#[test]
fn validate_rejects_unknown_next_state_value_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].next_state = NextState::Value(StateId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown next state value should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition next_state id 99 is not a valid state value")
    );
}

#[test]
fn validate_rejects_static_next_state_template_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: "Missing".to_string(),
        });

    let err = artifact
        .validate()
        .expect_err("static next-state template outside state table should fail");

    assert!(err.to_string().contains(
        "process Worker transition 0 next_state_template produced value Missing not admitted by state table"
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
        "process Worker transition 0 next_state_template.payload process reference template must be a direct message payload"
    ));
}

#[test]
fn validate_rejects_state_value_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].state_values[1] =
        ArtifactStateValue::with_label(OTHER_JOB, "HandledIdentity", "HandledLabel");

    let err = artifact
        .validate()
        .expect_err("state value type mismatch should fail");

    assert!(err.to_string().contains(
        "process Worker state value HandledIdentity (label HandledLabel) has type id 5, expected 2"
    ));
}

#[test]
fn validate_rejects_next_state_template_when_label_matches_but_identity_does_not() {
    let mut artifact = valid_artifact();
    artifact.processes[1].state_values[1] =
        ArtifactStateValue::with_label(WORKER_STATE, "Spoofed", "Handled");
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: "Handled".to_string(),
        });

    let err = artifact
        .validate()
        .expect_err("state labels must not admit mismatched typed values");

    assert!(err.to_string().contains(
        "process Worker transition 0 next_state_template produced value Handled not admitted by state table"
    ));
}

#[test]
fn validate_rejects_current_state_payload_template_outside_state_table() {
    let mut artifact = valid_artifact();
    let mut working = ArtifactStateValue::with_label(
        WORKER_STATE,
        "Working(Job{phase:Ready})",
        "Working(Job{phase:Ready})",
    );
    working.payload = Some(ArtifactPayload {
        ty: JOB,
        value: "Job{phase:Ready}".to_string(),
        process_ref: None,
    });
    artifact.processes[1].state_values =
        vec![ArtifactStateValue::new(WORKER_STATE, "Idle"), working];
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: Some(StateId::new(0)),
            message: MessageId::new(0),
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(1)),
            message: MessageId::new(0),
            step_result: StepResult::Stop,
            next_state: NextState::Template(ArtifactValueTemplate::EnumVariant {
                ty: WORKER_STATE,
                variant: "Done".to_string(),
                payload: Box::new(ArtifactValueTemplate::CurrentStatePayload { ty: JOB }),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    let err = artifact
        .validate()
        .expect_err("unadmitted current-state-derived next state should fail");

    assert!(err.to_string().contains(
        "process Worker transition 0 next_state_template produced value Done(Job{phase:Ready}) not admitted by state table"
    ));
}

#[test]
fn validate_rejects_missing_transition_for_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));

    let err = artifact
        .validate()
        .expect_err("missing transition should fail");

    assert!(
        err.to_string()
            .contains("process Worker has no transition for message id 1")
    );
}

#[test]
fn validate_rejects_duplicate_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));
    let duplicate = artifact.processes[1].transitions[0].clone();
    artifact.processes[1].transitions.push(duplicate);

    let err = artifact
        .validate()
        .expect_err("duplicate transition should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate transition for message id 0")
    );
}

#[test]
fn validate_rejects_unknown_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].message = MessageId::new(1);

    let err = artifact
        .validate()
        .expect_err("unknown transition message should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition message id 1 is not accepted")
    );
}

#[test]
fn validate_rejects_transition_current_state_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].current_state = Some(StateId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown transition current state should fail");

    assert!(
        err.to_string()
            .contains("process Worker message id 0 current_state id 99 is not a valid state value")
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

#[test]
fn write_artifact_rejects_invalid_artifacts_before_writing() {
    let dir = unique_test_dir("invalid-artifact-write");
    let path = dir.join("bad.mta");
    let mut artifact = valid_artifact();
    artifact.format = "invalid-format".to_string();

    let err = write_artifact(&path, &artifact).expect_err("invalid artifact should fail");

    assert!(err.to_string().contains("unsupported artifact format"));
    assert!(!path.exists(), "invalid artifact must not be written");
    assert!(
        !dir.exists(),
        "invalid artifact must not create parent dirs"
    );
}

#[test]
fn write_artifact_accepts_current_directory_output_path() {
    let path = unique_current_dir_artifact_path("artifact-current-dir");
    let artifact = valid_artifact();

    write_artifact(&path, &artifact).expect("current-directory artifact write should succeed");

    let decoded = read_artifact(&path).expect("written artifact should decode");
    assert_eq!(decoded, artifact);

    fs::remove_file(path).expect("test artifact should be removed");
}

#[test]
fn write_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");
    let artifact = valid_artifact();

    let err =
        write_artifact(&path, &artifact).expect_err("directory artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(unix)]
#[test]
fn write_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-write-fifo");
    create_fifo(&path);
    let artifact = valid_artifact();

    let err = write_artifact(&path, &artifact).expect_err("fifo artifact output path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[test]
fn read_artifact_rejects_oversized_file() {
    let path = unique_current_dir_artifact_path("artifact-too-large");
    fs::write(&path, vec![b'a'; MAX_ARTIFACT_BYTES + 1])
        .expect("oversized test file should be written");

    let err = read_artifact(&path).expect_err("oversized artifact file should fail");

    assert!(err.to_string().contains("is too large"));

    fs::remove_file(path).expect("test artifact should be removed");
}

#[test]
fn read_artifact_rejects_directory_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-directory");
    fs::create_dir_all(&path).expect("test artifact dir should be created");

    let err = read_artifact(&path).expect_err("directory artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_dir(path).expect("test artifact dir should be removed");
}

#[cfg(unix)]
#[test]
fn read_artifact_rejects_fifo_path_before_opening() {
    let path = unique_current_dir_artifact_path("artifact-fifo");
    create_fifo(&path);

    let err = read_artifact(&path).expect_err("fifo artifact path should fail");

    assert!(err.to_string().contains("is not a regular file"));

    fs::remove_file(path).expect("test fifo should be removed");
}

#[cfg(unix)]
fn create_fifo(path: &Path) {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
}

fn valid_artifact() -> MantleArtifact {
    MantleArtifact {
        format: ARTIFACT_FORMAT.to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: TEST_SOURCE_LANGUAGE.to_string(),
        module: "actor_ping".to_string(),
        entry_process: ProcessId::new(0),
        entry_message: MessageId::new(0),
        types: vec![
            ArtifactType::value("MainState"),
            ArtifactType::value("MainMsg"),
            ArtifactType::value("WorkerState"),
            ArtifactType::value("WorkerMsg"),
            ArtifactType::value("Job"),
            ArtifactType::value("OtherJob"),
            ArtifactType::value("Box"),
            ArtifactType::value("MainPayload"),
            ArtifactType::process_ref("ProcessRef_Main", ProcessId::new(0)),
            ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
        ],
        outputs: vec!["worker handled Ping".to_string()],
        processes: vec![
            ArtifactProcess {
                debug_name: "Main".to_string(),
                state_type: MAIN_STATE,
                state_values: state_values(MAIN_STATE, &["MainState"]),
                message_type: MAIN_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Start")],
                process_refs: vec![ArtifactProcessRef {
                    debug_name: "worker".to_string(),
                    target: ProcessId::new(1),
                }],
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Current,
                    effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                    actions: vec![
                        ArtifactAction::Spawn {
                            target: ProcessId::new(1),
                            process_ref: ProcessRefId::new(0),
                        },
                        ArtifactAction::Send {
                            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                            message: MessageId::new(0),
                            payload: None,
                        },
                    ],
                }],
            },
            ArtifactProcess {
                debug_name: "Worker".to_string(),
                state_type: WORKER_STATE,
                state_values: state_values(WORKER_STATE, &["Idle", "Handled"]),
                message_type: WORKER_MSG,
                message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                process_refs: Vec::new(),
                mailbox_bound: 1,
                init_state: StateId::new(0),
                transitions: vec![ArtifactTransition {
                    current_state: None,
                    message: MessageId::new(0),
                    step_result: StepResult::Stop,
                    next_state: NextState::Value(StateId::new(1)),
                    effects: vec![ArtifactEffect::Emit],
                    actions: vec![ArtifactAction::Emit {
                        output: OutputId::new(0),
                    }],
                }],
            },
        ],
        source_hash_fnv1a64: "0000000000000000".to_string(),
    }
}

fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
    values
        .iter()
        .map(|value| ArtifactStateValue::new(ty, *value))
        .collect()
}

fn emit_actions(count: usize) -> Vec<ArtifactAction> {
    vec![
        ArtifactAction::Emit {
            output: OutputId::new(0)
        };
        count
    ]
}

fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique_artifact_name(name))
}

fn unique_current_dir_artifact_path(name: &str) -> PathBuf {
    PathBuf::from(unique_artifact_name(name))
}

fn unique_artifact_name(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    format!("mantle-{name}-{}-{nanos}.mta", std::process::id())
}
