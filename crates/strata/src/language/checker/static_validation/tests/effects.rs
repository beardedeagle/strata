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

#[test]
fn static_validation_rejects_runtime_if_literal_that_is_not_bool_atom() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let process = process_with_runtime_if_condition(CheckedValueTemplate::Literal(
        CheckedPayloadValue::new(bool_ty, artifact_value("Maybe")),
    ));

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("invalid checked Bool literal should fail static validation");

    assert!(
        err.to_string()
            .contains("if condition must evaluate to unit Bool value False or True"),
        "{err}"
    );
}

#[test]
fn static_validation_rejects_runtime_if_dynamic_non_unit_bool_shape() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let process = process_with_runtime_if_payload_condition(CheckedValueTemplate::Record {
        ty: bool_ty.clone(),
        fields: vec![CheckedValueTemplateField::new(
            ident("value"),
            CheckedValueTemplate::ReceivedPayload { ty: bool_ty },
        )],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("non-unit checked Bool condition shape should fail static validation");

    assert!(
        err.to_string()
            .contains("if condition must evaluate to unit Bool value False or True"),
        "{err}"
    );
}

#[test]
fn static_validation_accepts_one_nested_runtime_if_branch_action() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let condition =
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(bool_ty, artifact_value("True")));
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
            actions: vec![CheckedAction::IfElse {
                condition: condition.clone(),
                then_actions: vec![CheckedAction::IfElse {
                    condition,
                    then_actions: vec![CheckedAction::Emit {
                        output: checked_output_id(0),
                    }],
                    else_actions: vec![CheckedAction::Emit {
                        output: checked_output_id(0),
                    }],
                }],
                else_actions: vec![CheckedAction::Emit {
                    output: checked_output_id(0),
                }],
            }],
        })],
    });

    validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
        .expect("one nested checked runtime if action should pass static validation");
}

#[test]
fn static_validation_rejects_runtime_if_action_nesting_above_limit() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let condition =
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(bool_ty, artifact_value("True")));
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
            actions: vec![CheckedAction::IfElse {
                condition: condition.clone(),
                then_actions: vec![CheckedAction::IfElse {
                    condition: condition.clone(),
                    then_actions: vec![CheckedAction::IfElse {
                        condition,
                        then_actions: vec![CheckedAction::Emit {
                            output: checked_output_id(0),
                        }],
                        else_actions: Vec::new(),
                    }],
                    else_actions: Vec::new(),
                }],
                else_actions: Vec::new(),
            }],
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("over-limit checked runtime if action nesting should fail");
    assert!(
        err.to_string()
            .contains("runtime if action nesting exceeds maximum depth of 2"),
        "{err}"
    );
}

fn process_with_runtime_if_condition(condition: CheckedValueTemplate) -> CheckedProcess {
    process_with_runtime_if(condition, None)
}

fn process_with_runtime_if_payload_condition(condition: CheckedValueTemplate) -> CheckedProcess {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    process_with_runtime_if(condition, Some(bool_ty))
}

fn process_with_runtime_if(
    condition: CheckedValueTemplate,
    payload_type: Option<CheckedTypeRef>,
) -> CheckedProcess {
    CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                payload_type,
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
            actions: vec![CheckedAction::IfElse {
                condition,
                then_actions: Vec::new(),
                else_actions: Vec::new(),
            }],
        })],
    })
}

#[test]
fn static_validation_rejects_runtime_if_branch_process_ref_binding() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let condition =
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(bool_ty, artifact_value("True")));
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
            effects: vec![Effect::Spawn],
            actions: vec![CheckedAction::IfElse {
                condition,
                then_actions: vec![CheckedAction::Spawn {
                    target: checked_process_id(1),
                    process_ref: checked_process_ref_id(0),
                }],
                else_actions: Vec::new(),
            }],
        })],
    });

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("runtime if branch process ref binding should fail static validation");

    assert!(
        err.to_string()
            .contains("runtime if branch cannot bind process references"),
        "{err}"
    );
}

#[test]
fn static_validation_accepts_runtime_if_branch_for_each_action() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let condition =
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(bool_ty, artifact_value("True")));
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
            actions: vec![CheckedAction::IfElse {
                condition,
                then_actions: vec![CheckedAction::ForEach {
                    element: CheckedLoopElement::new(
                        CheckedLoopElementId::from_index(0).expect("valid loop element id"),
                        value_type("Job"),
                    ),
                    collection: CheckedValueTemplate::Literal(CheckedPayloadValue::new(
                        value_type("JobList"),
                        artifact_value("List[Ready]"),
                    )),
                    max_items: 1,
                    body: Vec::new(),
                }],
                else_actions: Vec::new(),
            }],
        })],
    });

    validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
        .expect("runtime if branch for loop action should pass static validation");
}
