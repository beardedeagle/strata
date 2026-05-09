use super::support::*;

#[test]
fn static_validation_rejects_action_without_declared_effect() {
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
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: vec![CheckedAction::Emit {
                output: CheckedOutputId::from_index(0).expect("valid checked output id"),
            }],
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("missing checked transition effect should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 uses effect emit but does not declare it")
    );
}

#[test]
fn static_validation_rejects_declared_effect_without_action() {
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
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Emit],
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("unused checked transition effect should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 declares effect emit but no action uses it")
    );
}

#[test]
fn static_validation_rejects_duplicate_transition_effect() {
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
            next_state: CheckedNextState::Current,
            effects: vec![Effect::Emit, Effect::Emit],
            actions: Vec::new(),
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("duplicate checked transition effect should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 declares duplicate effect emit")
    );
}
