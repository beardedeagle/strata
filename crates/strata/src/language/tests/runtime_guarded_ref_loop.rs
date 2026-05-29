use super::support::*;

#[test]
fn guarded_runtime_for_each_received_ref_target_checks_and_lowers() {
    let checked = check_source(RUNTIME_GUARDED_REF_LOOP)
        .expect("guarded runtime loop with received process ref should check");
    let worker_checked_id = checked
        .processes()
        .iter()
        .enumerate()
        .find_map(|(index, process)| {
            (process.debug_name().as_str() == "Worker").then(|| checked_process_id(index))
        })
        .expect("Worker should be checked");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    assert!(
        batch_worker.process_refs().is_empty(),
        "BatchWorker must route through the received process ref without local spawn authority"
    );
    let route_transition = batch_worker
        .transitions()
        .iter()
        .find(|transition| {
            transition.message() == checked_message_id(1)
                && matches!(
                    transition.actions(),
                    [CheckedAction::IfElse {
                        condition: CheckedValueTemplate::Equality { .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
                        then_actions.as_slice(),
                        [CheckedAction::ForEach {
                            collection: CheckedValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            },
                            max_items: 2,
                            body,
                            ..
                        }] if field.as_str() == "items"
                            && matches!(
                                record.as_ref(),
                                CheckedValueTemplate::CurrentStatePayload { .. }
                            )
                            && matches!(
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
                                            target: CheckedSendTarget::ReceivedPayload {
                                                target,
                                                ..
                                            },
                                            payload: Some(payload),
                                            ..
                                        },
                                    ] if *target == worker_checked_id
                                        && matches!(
                                            payload.as_ref(),
                                            CheckedValueTemplate::LoopElement { .. }
                                        )
                                ) && else_actions.is_empty()
                            )
                    ) && else_actions.is_empty()
                )
        })
        .expect("Route transition should guard a loop that sends through received ProcessRef");
    assert_eq!(route_transition.step_result(), CheckedStepResult::Continue);

    let artifact = lower_to_artifact(&checked, RUNTIME_GUARDED_REF_LOOP)
        .expect("guarded runtime received process ref loop should lower");
    let (worker_artifact_index, _) = artifact
        .processes
        .iter()
        .enumerate()
        .find(|(_, process)| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let worker_process_id =
        ProcessId::from_index(worker_artifact_index).expect("artifact process index should fit");
    let worker_ref_type = artifact_process_ref_type_id(&artifact, worker_process_id);
    let (batch_worker_artifact_index, batch_worker_artifact) = artifact
        .processes
        .iter()
        .enumerate()
        .find(|(_, process)| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    assert!(
        batch_worker_artifact.process_refs.is_empty(),
        "BatchWorker artifact must not acquire local process refs"
    );
    assert!(
        batch_worker_artifact.transitions.iter().any(|transition| {
            transition.message == MessageId::new(1)
                && matches!(
                    transition.actions.as_slice(),
                    [ArtifactAction::IfElse {
                        condition: ArtifactValueTemplate::Equality { .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
                        then_actions.as_slice(),
                        [ArtifactAction::ForEach {
                            collection: ArtifactValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            },
                            max_items: 2,
                            body,
                            ..
                        }] if field.as_u32() == 1
                            && matches!(
                                record.as_ref(),
                                ArtifactValueTemplate::CurrentStatePayload { .. }
                            )
                            && matches!(
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
                                            target: ArtifactSendTarget::ReceivedPayload {
                                                ty,
                                                target_process,
                                            },
                                            payload: Some(ArtifactValueTemplate::LoopElement { .. }),
                                            ..
                                        },
                                    ] if *ty == worker_ref_type
                                        && *target_process == worker_process_id
                                ) && else_actions.is_empty()
                            )
                    ) && else_actions.is_empty()
                )
        }),
        "Route artifact transition should retain a typed received-payload send target inside the guarded loop"
    );
    let encoded = artifact.encode();
    let batch_worker_prefix = format!("process.{batch_worker_artifact_index}.");
    assert!(
        !encoded.lines().any(|line| {
            line.starts_with(&batch_worker_prefix)
                && (line.contains("target_process_ref")
                    || line.contains("debug_name=worker")
                    || line.ends_with("=worker"))
        }),
        "BatchWorker must not dispatch through source names or local process-ref ids"
    );
}
