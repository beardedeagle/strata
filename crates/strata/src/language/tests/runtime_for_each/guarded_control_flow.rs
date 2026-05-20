use super::*;

#[test]
fn guarded_runtime_for_each_checks_and_lowers_branch_contained_loop() {
    let checked =
        check_source(RUNTIME_GUARDED_FOR_EACH).expect("guarded runtime for source should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Continue);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::IfElse {
                condition: CheckedValueTemplate::Equality { .. },
                then_actions,
                else_actions,
            },
        ] if matches!(
            then_actions.as_slice(),
            [CheckedAction::ForEach {
                collection: CheckedValueTemplate::RecordField { .. },
                max_items: 2,
                body,
                ..
            }] if matches!(
                body.as_slice(),
                [CheckedAction::IfElse {
                    condition: CheckedValueTemplate::Equality { .. },
                    then_actions,
                    else_actions,
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
        ) && else_actions.is_empty()
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_GUARDED_FOR_EACH)
        .expect("guarded runtime for should lower");
    let batch_worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let artifact_transition = batch_worker_artifact
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::Equality { .. },
                then_actions,
                else_actions,
            },
        ] if matches!(
            then_actions.as_slice(),
            [ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::RecordField { .. },
                max_items: 2,
                body,
            }] if element.ty == bool_type
                && matches!(
                    body.as_slice(),
                    [ArtifactAction::IfElse {
                        condition: ArtifactValueTemplate::Equality { .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
                        then_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    ) && else_actions.is_empty()
                )
        ) && else_actions.is_empty()
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=if_else"));
    assert!(encoded.contains(".kind=for_each"));
    assert!(encoded.contains(".kind=loop_element"));
    assert!(
        !encoded
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "guarded loop artifact must not dispatch through the source loop binding name"
    );
}

#[test]
fn guarded_runtime_for_each_accepts_omitted_else() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "        } else {\n        }\n        return Continue(state);",
        "        }\n        return Continue(state);",
    );
    let checked = check_source(&source).expect("guarded runtime for omitted else should check");
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
            CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if matches!(then_actions.as_slice(), [CheckedAction::ForEach { .. }])
            && else_actions.is_empty()
    ));

    let artifact = lower_to_artifact(&checked, &source)
        .expect("guarded runtime for omitted else should lower");
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
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if matches!(then_actions.as_slice(), [ArtifactAction::ForEach { .. }])
            && else_actions.is_empty()
    ));
}

#[test]
fn guarded_runtime_for_each_accepts_loop_in_each_branch() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "        } else {\n        }\n        return Continue(state);",
        "        } else {\n            for item in items {\n                emit \"guarded loop selected false\";\n            }\n        }\n        return Continue(state);",
    );
    let checked = check_source(&source).expect("guarded runtime for both branches should check");
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
            CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if matches!(then_actions.as_slice(), [CheckedAction::ForEach { .. }])
            && matches!(else_actions.as_slice(), [CheckedAction::ForEach { .. }])
    ));

    let artifact = lower_to_artifact(&checked, &source)
        .expect("guarded runtime for both branches should lower");
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
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if matches!(then_actions.as_slice(), [ArtifactAction::ForEach { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::ForEach { .. }])
    ));
}
