use super::support::*;
use mantle_artifact::MAX_VALUE_TEMPLATE_FIELDS;

#[test]
fn static_validation_rejects_inactive_loop_element_payload() {
    let job = value_type("Job");
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
                    payload: Some(Box::new(CheckedValueTemplate::LoopElement {
                        ty: job.clone(),
                        element: checked_loop_element_id(0),
                    })),
                },
            ],
        })],
    });
    let worker = worker_process_with_payload(job);

    let err = validate_action_references(
        &[main, worker],
        &checked_process_id(0),
        &checked_message_id(0),
    )
    .expect_err("inactive checked loop element should fail before lowering");

    assert!(
        err.to_string()
            .contains("references inactive loop element id 0"),
        "{err}"
    );
}

#[test]
fn static_validation_rejects_nested_for_each_loop_body() {
    let process = process_with_single_loop_body(vec![CheckedAction::ForEach {
        element: CheckedLoopElement::new(checked_loop_element_id(1), value_type("Job")),
        collection: list_payload_template(),
        max_items: 1,
        body: Vec::new(),
    }]);

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("nested checked for loop should fail before lowering");

    assert!(
        err.to_string()
            .contains("nested for loops are not supported"),
        "{err}"
    );
}

#[test]
fn static_validation_accepts_runtime_if_inside_for_each_loop_body() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let process = process_with_single_loop_body_and_element(
        bool_ty.clone(),
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            bool_ty.clone(),
            artifact_value("List[True]"),
        )),
        vec![CheckedAction::IfElse {
            condition: CheckedValueTemplate::LoopElement {
                ty: bool_ty,
                element: checked_loop_element_id(0),
            },
            then_actions: vec![emit_action()],
            else_actions: vec![emit_action()],
        }],
        vec![Effect::Emit],
    );

    validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
        .expect("runtime if over active checked loop element should validate");
}

#[test]
fn static_validation_accepts_one_empty_runtime_if_branch_inside_for_each_loop_body() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let process = process_with_single_loop_body_and_element(
        bool_ty.clone(),
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            bool_ty.clone(),
            artifact_value("List[True]"),
        )),
        vec![CheckedAction::IfElse {
            condition: CheckedValueTemplate::LoopElement {
                ty: bool_ty,
                element: checked_loop_element_id(0),
            },
            then_actions: Vec::new(),
            else_actions: vec![emit_action()],
        }],
        vec![Effect::Emit],
    );

    validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
        .expect("one empty checked runtime if loop branch should validate");
}

#[test]
fn static_validation_rejects_both_empty_runtime_if_branches_inside_for_each_loop_body() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let process = process_with_single_loop_body_and_element(
        bool_ty.clone(),
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            bool_ty.clone(),
            artifact_value("List[True]"),
        )),
        vec![CheckedAction::IfElse {
            condition: CheckedValueTemplate::LoopElement {
                ty: bool_ty,
                element: checked_loop_element_id(0),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        }],
        Vec::new(),
    );

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("both empty checked runtime if branches should fail");
    assert!(
        err.to_string()
            .contains("runtime if action branches cannot both be empty"),
        "{err}"
    );
}

#[test]
fn static_validation_rejects_nested_runtime_if_inside_for_each_loop_branch() {
    let bool_ty = enum_value_type("Bool", &["False", "True"]);
    let condition = CheckedValueTemplate::LoopElement {
        ty: bool_ty.clone(),
        element: checked_loop_element_id(0),
    };
    let process = process_with_single_loop_body_and_element(
        bool_ty.clone(),
        CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            bool_ty,
            artifact_value("List[True]"),
        )),
        vec![CheckedAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![CheckedAction::IfElse {
                condition,
                then_actions: Vec::new(),
                else_actions: Vec::new(),
            }],
            else_actions: vec![emit_action()],
        }],
        vec![Effect::Emit],
    );

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("nested runtime if in checked for loop branch should fail");
    assert!(
        err.to_string()
            .contains("for loop branch cannot contain nested runtime if actions"),
        "{err}"
    );
}

#[test]
fn static_validation_rejects_loop_element_id_above_codec_bound() {
    let process =
        process_with_single_loop_element_id(checked_loop_element_id(MAX_VALUE_TEMPLATE_FIELDS));

    let err =
        validate_action_references(&[process], &checked_process_id(0), &checked_message_id(0))
            .expect_err("checked loop element ids above the codec bound should fail");

    assert!(
        err.to_string().contains(&format!(
            "for loop element id {MAX_VALUE_TEMPLATE_FIELDS} must be no greater than"
        )),
        "{err}"
    );
}

fn process_with_single_loop_body(body: Vec<CheckedAction>) -> CheckedProcess {
    process_with_single_loop(
        checked_loop_element_id(0),
        value_type("Job"),
        list_payload_template(),
        Vec::new(),
        body,
    )
}

fn process_with_single_loop_body_and_element(
    element_ty: CheckedTypeRef,
    collection: CheckedValueTemplate,
    body: Vec<CheckedAction>,
    effects: Vec<Effect>,
) -> CheckedProcess {
    process_with_single_loop(
        checked_loop_element_id(0),
        element_ty,
        collection,
        effects,
        body,
    )
}

fn process_with_single_loop_element_id(element_id: CheckedLoopElementId) -> CheckedProcess {
    process_with_single_loop(
        element_id,
        value_type("Job"),
        list_payload_template(),
        Vec::new(),
        Vec::new(),
    )
}

fn process_with_single_loop(
    element_id: CheckedLoopElementId,
    element_ty: CheckedTypeRef,
    collection: CheckedValueTemplate,
    effects: Vec<Effect>,
    body: Vec<CheckedAction>,
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
            effects,
            actions: vec![CheckedAction::ForEach {
                element: CheckedLoopElement::new(element_id, element_ty),
                collection,
                max_items: 1,
                body,
            }],
        })],
    })
}

fn worker_process_with_payload(payload_ty: CheckedTypeRef) -> CheckedProcess {
    CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Branch".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(payload_ty),
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
    })
}

fn list_payload_template() -> CheckedValueTemplate {
    CheckedValueTemplate::Literal(CheckedPayloadValue::new(
        value_type("JobList"),
        artifact_value("List[Ready]"),
    ))
}

fn checked_loop_element_id(index: usize) -> CheckedLoopElementId {
    CheckedLoopElementId::from_index(index).expect("valid checked loop element id")
}

fn emit_action() -> CheckedAction {
    CheckedAction::Emit {
        output: CheckedOutputId::from_index(0).expect("valid checked output id"),
    }
}
