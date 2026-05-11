use super::support::*;

#[test]
fn static_validation_rejects_literal_send_payload_with_control_character_value() {
    let template = CheckedValueTemplate::Literal(CheckedPayloadValue::new(
        value_type("Job"),
        ArtifactValue::Atom("Job\n".to_string()),
    ));
    let err = validate_value_template_payload_labels(&template)
        .expect_err("invalid literal payload value should fail static validation");

    assert!(
        err.to_string()
            .contains("artifact field payload value must be an identifier"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_literal_send_payload_with_invalid_shape() {
    let template = CheckedValueTemplate::Literal(CheckedPayloadValue::new(
        value_type("Job"),
        ArtifactValue::List(vec![
            ArtifactValue::Atom("Ready".to_string());
            mantle_artifact::MAX_VALUE_TEMPLATE_FIELDS + 1
        ]),
    ));
    let err = validate_value_template_payload_labels(&template)
        .expect_err("invalid literal payload shape should fail static validation");

    assert!(
        err.to_string()
            .contains("payload value.item_count must be no greater than"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_invalid_checked_state_value_shape() {
    let process = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: vec![CheckedStateValue::new(
            value_type("MainState"),
            ArtifactValue::Atom("not-valid".to_string()),
        )],
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("invalid checked state value shape should fail static validation");

    assert!(
        err.to_string()
            .contains("artifact field state value must be an identifier"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_duplicate_map_projection_keys() {
    let template = CheckedValueTemplate::MapValue {
        ty: value_type("Phase"),
        map: Box::new(CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            value_type("PhaseMap"),
            artifact_value("Map[Done=>Ready,Ready=>Done]"),
        ))),
        key: artifact_value("Ready"),
        keys: vec![artifact_value("Ready"), artifact_value("Ready")],
        projection: mantle_artifact::MapProjectionMode::Exact,
    };
    let err = validate_value_template_payload_labels(&template)
        .expect_err("duplicate map projection keys should fail static validation");

    assert!(
        err.to_string()
            .contains("map projection duplicates expected map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_duplicate_record_template_fields() {
    let template = CheckedValueTemplate::Record {
        ty: value_type("Job"),
        fields: vec![
            CheckedValueTemplateField::new(
                ident("phase"),
                CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                    value_type("Phase"),
                    artifact_value("Ready"),
                )),
            ),
            CheckedValueTemplateField::new(
                ident("phase"),
                CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                    value_type("Phase"),
                    artifact_value("Done"),
                )),
            ),
        ],
    };
    let err = validate_value_template_payload_labels(&template)
        .expect_err("duplicate record template fields should fail static validation");

    assert!(
        err.to_string()
            .contains("record template duplicates field phase"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_dynamic_map_template_keys() {
    let template = CheckedValueTemplate::Map {
        ty: value_type("PhaseMap"),
        entries: vec![CheckedValueTemplateMapEntry::new(
            CheckedValueTemplate::ReceivedPayload {
                ty: value_type("Phase"),
            },
            CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                value_type("Phase"),
                artifact_value("Done"),
            )),
        )],
    };
    let err = validate_value_template_payload_labels(&template)
        .expect_err("dynamic map template keys should fail static validation");

    assert!(
        err.to_string()
            .contains("map template keys must be static source values"),
        "unexpected error: {err}"
    );
}

#[test]
fn static_validation_rejects_received_payload_send_target_with_non_process_ref_type() {
    let main = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(value_type("Job")),
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Send],
            actions: vec![CheckedAction::Send {
                target: CheckedSendTarget::ReceivedPayload {
                    ty: value_type("Job"),
                    target: checked_process_id(1),
                },
                message: checked_message_id(0),
                payload: None,
            }],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Done".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("non-process-ref received send target should fail");

    assert!(
        err.to_string()
            .contains("process reference payload type Job must be a process reference type")
    );
}

#[test]
fn static_validation_rejects_process_ref_template_with_non_process_ref_type() {
    let main = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: vec![CheckedProcessRef::new(
            ident("worker"),
            checked_process_id(1),
        )],
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Spawn, Effect::Send],
            actions: vec![
                CheckedAction::Spawn {
                    target: checked_process_id(1),
                    process_ref: checked_process_ref_id(0),
                },
                CheckedAction::Send {
                    target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                    message: checked_message_id(0),
                    payload: Some(CheckedValueTemplate::ProcessRef {
                        ty: value_type("Job"),
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    }),
                },
            ],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Assign".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(value_type("Job")),
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("non-process-ref process ref template should fail");

    assert!(
        err.to_string()
            .contains("process reference payload type Job must be a process reference type")
    );
}

#[test]
fn static_validation_formats_process_ref_type_diagnostics_without_internal_labels() {
    let main = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: vec![CheckedProcessRef::new(
            ident("worker"),
            checked_process_id(1),
        )],
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Spawn, Effect::Send],
            actions: vec![
                CheckedAction::Spawn {
                    target: checked_process_id(1),
                    process_ref: checked_process_ref_id(0),
                },
                CheckedAction::Send {
                    target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                    message: checked_message_id(0),
                    payload: Some(CheckedValueTemplate::ProcessRef {
                        ty: process_ref_type("Worker"),
                        target: checked_process_id(0),
                        process_ref: checked_process_ref_id(0),
                    }),
                },
            ],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Reply".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(process_ref_type("Worker")),
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("process-ref type target mismatch should fail");
    let message = err.to_string();

    assert!(message.contains(
        "process reference payload type ProcessRef<Worker> targets Worker (process id 1), expected Main (process id 0)"
    ));
    assert!(!message.contains("__strata_checked_process_ref_"));
}

#[test]
fn static_validation_rejects_nested_process_ref_payload_template() {
    let main = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: vec![CheckedProcessRef::new(
            ident("worker"),
            checked_process_id(1),
        )],
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Spawn, Effect::Send],
            actions: vec![
                CheckedAction::Spawn {
                    target: checked_process_id(1),
                    process_ref: checked_process_ref_id(0),
                },
                CheckedAction::Send {
                    target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                    message: checked_message_id(0),
                    payload: Some(CheckedValueTemplate::Record {
                        ty: value_type("Box"),
                        fields: vec![CheckedValueTemplateField::new(
                            ident("reply_to"),
                            CheckedValueTemplate::ProcessRef {
                                ty: process_ref_type("Worker"),
                                target: checked_process_id(1),
                                process_ref: checked_process_ref_id(0),
                            },
                        )],
                    }),
                },
            ],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Assign".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(value_type("Box")),
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("nested process ref template should fail");

    assert!(
        err.to_string()
            .contains("process reference payload templates must be direct message payloads")
    );
}
