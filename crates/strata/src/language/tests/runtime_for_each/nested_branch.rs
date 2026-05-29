use super::*;

#[test]
fn runtime_for_each_nested_if_actions_check_and_lower_to_typed_loop_branch_templates() {
    let checked = check_source(RUNTIME_FOR_EACH_NESTED_IF_ACTIONS)
        .expect("runtime for-each nested if action source should check");
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
                max_items: 3,
                body,
                ..
            },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::IfElse {
                condition: CheckedValueTemplate::Equality { left, .. },
                then_actions,
                else_actions,
            }] if matches!(
                    left.as_ref(),
                    CheckedValueTemplate::RecordField {
                        record,
                        field,
                        ..
                    } if field.as_str() == "outer_flag"
                        && matches!(record.as_ref(), CheckedValueTemplate::LoopElement { .. })
                )
                && matches!(
                    then_actions.as_slice(),
                    [CheckedAction::IfElse {
                        condition: CheckedValueTemplate::Equality { left, .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
                            left.as_ref(),
                            CheckedValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            } if field.as_str() == "inner_flag"
                                && matches!(record.as_ref(), CheckedValueTemplate::LoopElement { .. })
                        )
                        && matches!(
                            then_actions.as_slice(),
                            [
                                CheckedAction::Emit { .. },
                                CheckedAction::Send {
                                    payload: Some(payload),
                                    ..
                                },
                            ] if matches!(
                                payload.as_ref(),
                                CheckedValueTemplate::RecordField { field, .. }
                                    if field.as_str() == "inner_flag"
                            )
                        )
                        && matches!(
                            else_actions.as_slice(),
                            [
                                CheckedAction::Emit { .. },
                                CheckedAction::Send {
                                    payload: Some(payload),
                                    ..
                                },
                            ] if matches!(
                                payload.as_ref(),
                                CheckedValueTemplate::RecordField { field, .. }
                                    if field.as_str() == "inner_flag"
                            )
                        )
                )
                && matches!(else_actions.as_slice(), [CheckedAction::Emit { .. }])
        )
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_FOR_EACH_NESTED_IF_ACTIONS)
        .expect("runtime for-each nested if action source should lower");
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
                max_items: 3,
                body,
            },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::Equality { left, .. },
                then_actions,
                else_actions,
            }] if matches_loop_record_field(left, element.id, 0)
                && matches!(
                    then_actions.as_slice(),
                    [ArtifactAction::IfElse {
                        condition: ArtifactValueTemplate::Equality { left, .. },
                        then_actions,
                        else_actions,
                    }] if matches_loop_record_field(left, element.id, 1)
                        && matches_send_loop_record_field(
                            then_actions.as_slice(),
                            element.id,
                            1
                        )
                        && matches_send_loop_record_field(
                            else_actions.as_slice(),
                            element.id,
                            1
                        )
                )
                && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
        )
    ));

    assert!(
        !artifact.encode().lines().any(|line| {
            line.ends_with("=outer")
                || line.ends_with("=inner")
                || line.contains("debug_name=outer")
                || line.contains("debug_name=inner")
        }),
        "loop branch artifact must not dispatch through source pattern aliases"
    );
}

fn matches_loop_record_field(
    template: &ArtifactValueTemplate,
    element: mantle_artifact::LoopElementId,
    field_id: u32,
) -> bool {
    matches!(
        template,
        ArtifactValueTemplate::RecordField { record, field, .. }
            if field.as_u32() == field_id
                && matches!(
                    record.as_ref(),
                    ArtifactValueTemplate::LoopElement {
                        element: field_element,
                        ..
                    } if *field_element == element
                )
    )
}

fn matches_send_loop_record_field(
    actions: &[ArtifactAction],
    element: mantle_artifact::LoopElementId,
    field_id: u32,
) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                payload: Some(payload),
                ..
            },
        ] if matches_loop_record_field(payload, element, field_id)
    )
}
