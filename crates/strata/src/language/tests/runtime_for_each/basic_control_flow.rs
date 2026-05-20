use super::*;

#[test]
fn runtime_for_each_checks_and_lowers_to_mantle_loop_control_flow() {
    let checked = check_source(RUNTIME_FOR_EACH).expect("runtime for source should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::ForEach {
                collection: CheckedValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
                ..
            },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::Send {
                payload: Some(payload),
                ..
            }] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
        )
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_FOR_EACH).expect("runtime for should lower");
    let batch_worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let artifact_transition = batch_worker_artifact
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
            },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::Send {
                payload: Some(ArtifactValueTemplate::LoopElement { element: payload_element, .. }),
                ..
            }] if *payload_element == element.id
        )
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=for_each"));
    assert!(encoded.contains(".kind=loop_element"));
    assert!(
        !encoded
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop element dispatch must not lower the source binding name"
    );
}

#[test]
fn runtime_for_each_if_checks_and_lowers_to_mantle_loop_branch_control_flow() {
    let checked = check_source(RUNTIME_FOR_EACH_IF).expect("runtime for-if source should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::ForEach {
                collection: CheckedValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
                ..
            },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::IfElse {
                condition: CheckedValueTemplate::BooleanBinary {
                    operator: CheckedValueBooleanOperator::And,
                    left,
                    right,
                    ..
                },
                then_actions,
                else_actions,
            }] if matches!(left.as_ref(), CheckedValueTemplate::Equality { .. })
                && matches!(right.as_ref(), CheckedValueTemplate::BooleanNot { .. })
                && matches!(
                then_actions.as_slice(),
                [
                    CheckedAction::Emit { .. },
                    CheckedAction::Send {
                        payload: Some(payload),
                        ..
                    },
                ] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
            ) && matches!(
                else_actions.as_slice(),
                [
                    CheckedAction::Emit { .. },
                    CheckedAction::Send {
                        payload: Some(payload),
                        ..
                    },
                ] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
            )
        )
    ));

    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert!(matches!(
        only_transition(worker).actions(),
        [CheckedAction::IfElse {
            condition: CheckedValueTemplate::Equality {
                operator: CheckedValueEqualityOperator::Equal,
                left,
                right,
                ..
            },
            then_actions,
            else_actions,
        }] if matches!(left.as_ref(), CheckedValueTemplate::ReceivedPayload { .. })
            && matches!(
                right.as_ref(),
                CheckedValueTemplate::Literal(value) if value.label() == "True"
            )
            && matches!(then_actions.as_slice(), [CheckedAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [CheckedAction::Emit { .. }])
    ));

    let artifact =
        lower_to_artifact(&checked, RUNTIME_FOR_EACH_IF).expect("runtime for-if should lower");
    let batch_worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let artifact_transition = batch_worker_artifact
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
            },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::BooleanBinary {
                    ty,
                    operator: ArtifactValueBooleanOperator::And,
                    left,
                    right,
                },
                then_actions,
                else_actions,
            }] if *ty == element.ty
                && matches!(
                    left.as_ref(),
                    ArtifactValueTemplate::Equality {
                        ty,
                        operand_ty,
                        operator: ArtifactValueEqualityOperator::NotEqual,
                        left,
                        right,
                    } if *ty == element.ty
                        && *operand_ty == element.ty
                        && matches!(
                            left.as_ref(),
                            ArtifactValueTemplate::LoopElement {
                                element: condition_element,
                                ..
                            } if *condition_element == element.id
                        )
                        && matches!(
                            right.as_ref(),
                            ArtifactValueTemplate::Literal { ty, value } if *ty == element.ty && value == &artifact_value("False")
                        )
                )
                && matches!(
                    right.as_ref(),
                    ArtifactValueTemplate::BooleanNot { ty, operand }
                        if *ty == element.ty && matches!(operand.as_ref(), ArtifactValueTemplate::Equality { .. })
                )
                && matches!(
                    then_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::Send {
                            payload: Some(ArtifactValueTemplate::LoopElement {
                                element: payload_element,
                                ..
                            }),
                            ..
                        },
                    ] if *payload_element == element.id
                )
                && matches!(
                    else_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::Send {
                            payload: Some(ArtifactValueTemplate::LoopElement {
                                element: payload_element,
                                ..
                            }),
                            ..
                        },
                    ] if *payload_element == element.id
                )
        )
    ));
    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=if_else"));
    assert!(encoded.contains(".kind=boolean_binary"));
    assert!(encoded.contains(".kind=boolean_not"));
    assert!(encoded.contains(".kind=equality"));
    assert!(encoded.contains(".kind=loop_element"));
    assert!(
        !encoded
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop branch dispatch must not lower the source binding name"
    );
}

#[test]
fn runtime_for_each_if_accepts_guard_branch_with_omitted_else() {
    let source = RUNTIME_FOR_EACH_IF.replace(
        "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            } else {\n                emit \"batch selected false\";\n                send worker Branch(item);\n            }",
        "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            }",
    );
    let checked = check_source(&source).expect("runtime loop guard branch should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::ForEach { body, .. },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            }] if matches!(
                    then_actions.as_slice(),
                    [
                        CheckedAction::Emit { .. },
                        CheckedAction::Send {
                            payload: Some(payload),
                            ..
                        },
                    ] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
                ) && else_actions.is_empty()
        )
    ));

    let artifact = lower_to_artifact(&checked, &source).expect("loop guard branch should lower");
    let batch_worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach { body, .. },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            }] if matches!(
                    then_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::Send {
                            payload: Some(ArtifactValueTemplate::LoopElement { .. }),
                            ..
                        },
                    ]
                ) && else_actions.is_empty()
        )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop guard branch artifact must not dispatch through the source loop binding name"
    );
}
