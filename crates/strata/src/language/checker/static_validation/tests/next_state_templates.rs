use super::support::*;

use mantle_artifact::MAX_NEXT_STATE_IF_ELSE_DEPTH;

#[test]
fn static_validation_rejects_next_state_if_else_above_terminal_limit() {
    let bool_type = enum_value_type("Bool", &["False", "True"]);
    let invalid_depth = MAX_NEXT_STATE_IF_ELSE_DEPTH + 1;
    let process = CheckedProcess::new(CheckedProcessParts {
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
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: nested_next_state_if_else(invalid_depth, &bool_type),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("checked next_state if above terminal depth limit must fail");

    let expected = format!(
        "next_state runtime if nesting exceeds maximum depth of {MAX_NEXT_STATE_IF_ELSE_DEPTH}"
    );
    assert!(err.to_string().contains(&expected), "{err}");
}

#[test]
fn static_validation_rejects_next_state_received_payload_template_for_unit_message() {
    let process = CheckedProcess::new(CheckedProcessParts {
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
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: CheckedStateId::from_index(0).expect("valid checked state id"),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Template(CheckedValueTemplate::ReceivedPayload {
                ty: value_type("MainState"),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("received payload template on unit message should fail");

    assert!(
        err.to_string()
            .contains("received payload template requires a payload-bearing message")
    );
}

fn nested_next_state_if_else(depth: usize, bool_type: &CheckedTypeRef) -> CheckedNextState {
    let mut next_state = CheckedNextState::Value(checked_state_id(0));
    let condition = CheckedValueTemplate::Literal(CheckedPayloadValue::new(
        bool_type.clone(),
        artifact_value("True"),
    ));
    for _ in 0..depth {
        next_state = CheckedNextState::IfElse {
            condition: condition.clone(),
            then_state: Box::new(next_state),
            else_state: Box::new(CheckedNextState::Value(checked_state_id(0))),
        };
    }
    next_state
}

#[test]
fn static_validation_rejects_static_next_state_template_outside_state_table() {
    let process = CheckedProcess::new(CheckedProcessParts {
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
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Template(CheckedValueTemplate::Literal(
                CheckedPayloadValue::new(
                    value_type("MainState"),
                    artifact_value("UnadmittedState"),
                ),
            )),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("unadmitted static template state should fail");

    assert!(err.to_string().contains(
        "process Main next_state template produced value UnadmittedState not admitted by state table"
    ));
}

#[test]
fn static_validation_rejects_process_ref_next_state_template() {
    let main = checked_process_with_spawn_to_worker(CheckedProcessParts {
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
            next_state: CheckedNextState::Template(CheckedValueTemplate::Record {
                ty: value_type("MainState"),
                fields: vec![CheckedValueTemplateField::new(
                    ident("reply_to"),
                    CheckedValueTemplate::ProcessRef {
                        ty: process_ref_type("Worker"),
                        target: checked_process_id(1),
                        process_ref: checked_process_ref_id(0),
                    },
                )],
            }),
            effects: Vec::new(),
            actions: Vec::new(),
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
    .expect_err("process ref next-state template should fail");

    assert!(
        err.to_string()
            .contains("process reference templates are not valid next-state values")
    );
}

#[test]
fn static_validation_rejects_process_ref_payload_enum_next_state_template() {
    let main = checked_process_with_spawn_to_worker(CheckedProcessParts {
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
    let worker_state = enum_value_type_with_payloads(
        "WorkerState",
        &[("Idle", None), ("Routed", Some(process_ref_type("Worker")))],
    );
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: worker_state.clone(),
        state_values: checked_state_values_for_type(worker_state.clone(), &["Idle"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Route".to_string(),
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
            next_state: CheckedNextState::Template(CheckedValueTemplate::EnumVariant {
                ty: worker_state,
                variant: checked_enum_variant_id(1),
                payload: Box::new(CheckedValueTemplate::ReceivedPayload {
                    ty: process_ref_type("Worker"),
                }),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("process ref payload enum next-state template should fail");

    assert!(
        err.to_string()
            .contains("process reference templates are not valid next-state values")
    );
}

#[test]
fn static_validation_rejects_payload_template_next_state_outside_state_table() {
    let main = checked_process_with_spawn_to_worker(CheckedProcessParts {
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
                    spawn_site: checked_spawn_site_id(0),
                },
                CheckedAction::Send {
                    target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                    message: checked_message_id(0),
                    payload: Some(Box::new(CheckedValueTemplate::Literal(
                        CheckedPayloadValue::new(
                            value_type("Job"),
                            artifact_value("Job{phase:Ready}"),
                        ),
                    ))),
                },
            ],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState{active:Job{phase:Done}}"]),
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
            next_state: CheckedNextState::Template(CheckedValueTemplate::Record {
                ty: value_type("WorkerState"),
                fields: vec![CheckedValueTemplateField::new(
                    ident("active"),
                    CheckedValueTemplate::ReceivedPayload {
                        ty: value_type("Job"),
                    },
                )],
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("unadmitted payload-derived template state should fail");

    assert!(err.to_string().contains(
        "process Worker next_state template produced value WorkerState{active:Job{phase:Ready}} not admitted by state table"
    ));
}

#[test]
fn static_validation_rejects_payload_enum_template_next_state_outside_state_table() {
    let main = checked_process_with_spawn_to_worker(CheckedProcessParts {
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
                    spawn_site: checked_spawn_site_id(0),
                },
                CheckedAction::Send {
                    target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                    message: checked_message_id(0),
                    payload: Some(Box::new(CheckedValueTemplate::Literal(
                        CheckedPayloadValue::new(
                            value_type("Job"),
                            artifact_value("Job{phase:Ready}"),
                        ),
                    ))),
                },
            ],
        })],
    });
    let worker_state = enum_value_type_with_payloads(
        "WorkerState",
        &[("Idle", None), ("Working", Some(value_type("Job")))],
    );
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: worker_state.clone(),
        state_values: checked_state_values_for_type(worker_state.clone(), &["Idle"]),
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
            next_state: CheckedNextState::Template(CheckedValueTemplate::EnumVariant {
                ty: worker_state,
                variant: checked_enum_variant_id(1),
                payload: Box::new(CheckedValueTemplate::ReceivedPayload {
                    ty: value_type("Job"),
                }),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("unadmitted payload-derived enum state should fail");

    assert!(err.to_string().contains(
        "process Worker next_state template produced value Working(Job{phase:Ready}) not admitted by state table"
    ));
}
