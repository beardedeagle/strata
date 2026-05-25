use super::support::*;

#[test]
fn checks_step_return_match_arm_for_each_runtime_if_prefixes_are_selected_and_typed() {
    let checked = check_source(PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX)
        .expect("arm-local for-if prefixes should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink should be checked");

    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(
        sink.transitions().len(),
        2,
        "selected arm-local loop branch sends should discover Sink payload cases"
    );

    for transition in worker.transitions() {
        assert_eq!(
            transition.effects(),
            &[Effect::Emit, Effect::Spawn, Effect::Send]
        );
        let [
            CheckedAction::Emit { .. },
            CheckedAction::Spawn { .. },
            CheckedAction::Emit { .. },
            CheckedAction::ForEach {
                element,
                collection,
                body,
                max_items,
            },
        ] = transition.actions()
        else {
            panic!(
                "selected return-match arm should lower uniform actions before one typed for-if action: {:?}",
                transition.actions()
            );
        };
        assert_eq!(*max_items, 2);
        assert!(
            matches!(
                collection,
                CheckedValueTemplate::RecordField { field, .. } if field.as_str() == "jobs"
            ),
            "return-match arm for collection should lower through the typed jobs payload template"
        );
        let [
            CheckedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            },
        ] = body.as_slice()
        else {
            panic!("arm-local for body should lower to one typed runtime if: {body:?}");
        };
        assert!(
            matches!(
                condition,
                CheckedValueTemplate::Equality {
                    operator: CheckedValueEqualityOperator::Equal,
                    left,
                    right,
                    ..
                } if matches!(
                    left.as_ref(),
                    CheckedValueTemplate::RecordField { field, record, .. }
                        if field.as_str() == "urgent"
                            && matches!(
                                record.as_ref(),
                                CheckedValueTemplate::LoopElement { element: loop_element, .. }
                                    if *loop_element == element.id()
                            )
                ) && matches!(
                    right.as_ref(),
                    CheckedValueTemplate::Literal(value) if value.label() == "True"
                )
            ),
            "loop-body runtime-if condition should lower through the typed loop element"
        );
        assert!(
            branch_sends_loop_phase(then_actions, element.id())
                && branch_sends_loop_phase(else_actions, element.id()),
            "both loop-body runtime-if branches should send a typed phase projection from the loop element"
        );
    }

    let artifact = lower_to_artifact(&checked, PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX)
        .expect("arm-local for-if prefixes should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    assert!(
        worker_artifact.transitions.iter().all(|transition| {
            matches!(
                transition.actions.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Spawn { .. },
                    ArtifactAction::Emit { .. },
                    ArtifactAction::ForEach { body, .. },
                ] if matches!(body.as_slice(), [ArtifactAction::IfElse { .. }])
            )
        }),
        "artifact should encode typed for_each actions containing typed runtime branches"
    );
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .any(|line| { line.contains("job_phase") || line.contains("job_urgent") }),
        "artifact execution must not dispatch through source loop aliases"
    );
}

#[test]
fn checks_step_return_match_arm_for_each_body_multiple_runtime_if_prefixes() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX.replace(
        "                    }\n                }\n                return Continue(SawReady);",
        "                    }\n                    if (job_urgent == True) {\n                        emit \"return-match ready second loop branch\";\n                    } else {\n                        emit \"return-match ready second loop fallback\";\n                    }\n                }\n                return Continue(SawReady);",
    );

    let checked = check_source(&source).expect("second loop-body runtime if should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert!(
        worker.transitions().iter().any(|transition| {
            matches!(
                transition.actions(),
                [
                    CheckedAction::Emit { .. },
                    CheckedAction::Spawn { .. },
                    CheckedAction::Emit { .. },
                    CheckedAction::ForEach { body, .. },
                ] if matches!(
                    body.as_slice(),
                    [CheckedAction::IfElse { .. }, CheckedAction::IfElse { .. }]
                )
            )
        }),
        "selected arm loop body should preserve both runtime-if actions in source order"
    );
}

#[test]
fn checks_step_return_match_arm_for_each_body_nested_runtime_if() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX.replace(
        "emit \"return-match ready urgent loop branch\";",
        "if (job_urgent == True) {\n                            emit \"return-match ready nested loop branch\";\n                        } else {\n                            emit \"return-match ready nested loop fallback\";\n                        }",
    );

    let checked = check_source(&source).expect("nested loop-body runtime if should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("nested loop-body runtime if should lower");

    assert!(
        artifact.processes.iter().any(|process| {
            process.debug_name == "Worker"
                && process.transitions.iter().any(|transition| {
                    matches!(
                        transition.actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Spawn { .. },
                            ArtifactAction::Emit { .. },
                            ArtifactAction::ForEach { body, .. },
                        ] if matches!(
                            body.as_slice(),
                            [ArtifactAction::IfElse { then_actions, .. }]
                                if matches!(
                                    then_actions.as_slice(),
                                    [ArtifactAction::IfElse { .. }, ArtifactAction::Send { .. }]
                                )
                        )
                    )
                })
        }),
        "selected arm loop-body nested runtime-if should lower as typed branch actions"
    );
}

#[test]
fn rejects_step_return_match_arm_for_each_body_runtime_if_nested_for_each() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX.replace(
        "emit \"return-match ready urgent loop branch\";",
        "for Job { phase: nested_phase, urgent: nested_urgent } in jobs {\n                            emit \"return-match ready nested loop item\";\n                            send sink Notice(nested_phase);\n                        }",
    );

    let err =
        check_source(&source).expect_err("nested loop inside loop-body runtime if should fail");

    assert!(
        err.to_string()
            .contains("nested for loops are not supported"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_for_each_body_runtime_if_missing_send_authority() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX.replace(
        "ProcResult<WorkerState> ! [emit, spawn, send]",
        "ProcResult<WorkerState> ! [emit, spawn]",
    );

    let err =
        check_source(&source).expect_err("loop-body branch send authority should be required");

    assert!(
        err.to_string()
            .contains("step uses effect send but does not declare it"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_for_each_runtime_if_invalid_send_payload_template() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_IF_PREFIX
        .replace(
            "        send worker Envelope(Assign(Assignment {\n            phase: Done,\n            jobs: List<Job,2>[\n                Job { phase: Done, urgent: True },\n                Job { phase: Ready, urgent: False },\n            ],\n        }));\n",
            "",
        )
        .replace(
            "                        emit \"return-match done urgent loop branch\";\n                        send sink Notice(job_phase);",
            "                        emit \"return-match done urgent loop branch\";\n                        send sink Notice(Assignment { phase: Done, jobs: jobs });",
        );

    let err = check_source(&source)
        .expect_err("unselected loop-body branch send payload should be statically validated");

    assert!(
        err.to_string()
            .contains("type Phase is not declared as a record"),
        "unexpected error: {err}"
    );
}

fn branch_sends_loop_phase(
    actions: &[CheckedAction],
    expected_element: crate::language::checked::CheckedLoopElementId,
) -> bool {
    matches!(
        actions,
        [
            CheckedAction::Emit { .. },
            CheckedAction::Send {
                payload: Some(payload),
                ..
            },
        ] if matches!(
            payload.as_ref(),
            CheckedValueTemplate::RecordField { field, record, .. }
                if field.as_str() == "phase"
                    && matches!(
                        record.as_ref(),
                        CheckedValueTemplate::LoopElement { element, .. }
                            if *element == expected_element
                    )
        )
    )
}
