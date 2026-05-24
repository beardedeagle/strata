use super::support::*;

#[test]
fn checks_step_return_match_arm_runtime_if_for_prefixes_are_selected_and_typed() {
    let checked = check_source(PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX)
        .expect("arm-local if-for prefixes should check");
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
        "selected branch-local loop sends should discover Sink payload cases"
    );

    for transition in worker.transitions() {
        let payload = transition
            .payload_guard()
            .map(|payload| payload.label())
            .expect("Worker transitions should carry concrete payload guards");
        assert_eq!(
            transition.effects(),
            &[Effect::Emit, Effect::Spawn, Effect::Send]
        );
        let [
            CheckedAction::Emit { .. },
            CheckedAction::Spawn { .. },
            CheckedAction::Emit { .. },
            CheckedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            },
        ] = transition.actions()
        else {
            panic!(
                "selected return-match arm should lower uniform actions before one typed if-for action: {:?}",
                transition.actions()
            );
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
                    CheckedValueTemplate::RecordField { field, .. }
                        if field.as_str() == "enabled"
                ) && matches!(
                    right.as_ref(),
                    CheckedValueTemplate::Literal(value) if value.label() == "True"
                )
            ),
            "return-match arm runtime-if condition should lower through the typed enabled payload template"
        );

        if payload.contains("Assign(Assignment{phase:Ready,") {
            assert_eq!(transition.step_result(), CheckedStepResult::Continue);
            assert_eq!(
                transition.next_state(),
                CheckedNextState::Value(checked_state_id(1))
            );
            assert!(
                branch_has_loop_send(then_actions)
                    && matches!(else_actions.as_slice(), [CheckedAction::Emit { .. }]),
                "Ready arm should loop only from the selected true branch"
            );
        } else {
            assert!(
                payload.contains("Assign(Assignment{phase:Done,"),
                "unexpected payload: {payload}"
            );
            assert_eq!(transition.step_result(), CheckedStepResult::Stop);
            assert_eq!(
                transition.next_state(),
                CheckedNextState::Value(checked_state_id(2))
            );
            assert!(
                matches!(then_actions.as_slice(), [CheckedAction::Emit { .. }])
                    && branch_has_loop_send(else_actions),
                "Done arm should loop only from the selected false branch"
            );
        }
    }

    let artifact = lower_to_artifact(&checked, PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX)
        .expect("arm-local if-for prefixes should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    assert!(
        worker_artifact
            .transitions
            .iter()
            .all(|transition| is_typed_if_for_transition(&transition.actions)),
        "artifact should encode typed if_else actions containing typed for_each branch actions"
    );
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.contains("job_phase")),
        "artifact execution must not dispatch through source loop aliases"
    );
}

#[test]
fn preserves_step_return_match_arm_runtime_if_branch_for_each_body_runtime_if() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                        emit \"return-match ready branch loop item\";\n                        send sink Notice(job_phase);",
        "                        if (job_phase == Ready) {\n                            emit \"return-match ready nested loop branch\";\n                            send sink Notice(job_phase);\n                        } else {\n                            emit \"return-match ready nested loop fallback\";\n                            send sink Notice(job_phase);\n                        }",
    );

    let checked = check_source(&source)
        .expect("branch-local for should preserve one direct loop-body runtime if");
    let artifact = lower_to_artifact(&checked, &source).expect("branch-local for-if should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    assert!(
        worker.transitions.iter().any(|transition| {
            matches!(
                transition.actions.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Spawn { .. },
                    ArtifactAction::Emit { .. },
                    ArtifactAction::IfElse { then_actions, .. },
                ] if matches!(
                    then_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::ForEach { body, .. },
                    ] if matches!(body.as_slice(), [ArtifactAction::IfElse { .. }])
                )
            )
        }),
        "branch-local for body should lower the admitted direct runtime-if action"
    );
}

#[test]
fn preserves_step_return_match_arm_runtime_if_branch_for_each_body_nested_runtime_if() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                        emit \"return-match ready branch loop item\";\n                        send sink Notice(job_phase);",
        "                        if (job_phase == Ready) {\n                            if (enabled == True) {\n                                emit \"return-match ready nested loop inner branch\";\n                                send sink Notice(job_phase);\n                            } else {\n                                emit \"return-match ready nested loop inner fallback\";\n                                send sink Notice(job_phase);\n                            }\n                        } else {\n                            emit \"return-match ready nested loop outer fallback\";\n                            send sink Notice(job_phase);\n                        }",
    );

    let checked = check_source(&source)
        .expect("branch-local for should reset loop-body runtime-if nesting depth");
    let artifact =
        lower_to_artifact(&checked, &source).expect("nested branch-local loop if should lower");

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
                            ArtifactAction::IfElse { then_actions, .. },
                        ] if matches!(
                            then_actions.as_slice(),
                            [
                                ArtifactAction::Emit { .. },
                                ArtifactAction::ForEach { body, .. },
                            ] if loop_body_has_nested_branch(body)
                        )
                    )
                })
        }),
        "loop body reached through a selected arm runtime-if branch should preserve nested typed branch actions"
    );
}

#[test]
fn checks_step_return_match_arm_runtime_if_branch_for_each_body_multiple_runtime_if_prefixes() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                        emit \"return-match ready branch loop item\";\n                        send sink Notice(job_phase);",
        "                        if (job_phase == Ready) {\n                            emit \"return-match ready first nested loop branch\";\n                            send sink Notice(job_phase);\n                        } else {\n                            emit \"return-match ready first nested loop fallback\";\n                            send sink Notice(job_phase);\n                        }\n                        if (job_phase == Done) {\n                            emit \"return-match ready second nested loop branch\";\n                            send sink Notice(job_phase);\n                        } else {\n                            emit \"return-match ready second nested loop fallback\";\n                            send sink Notice(job_phase);\n                        }",
    )
    .replace("proc Sink mailbox bounded(2)", "proc Sink mailbox bounded(4)");

    let checked =
        check_source(&source).expect("second branch-local loop-body runtime if should check");
    let artifact = lower_to_artifact(&checked, &source)
        .expect("second branch-local loop-body runtime if should lower");

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
                            ArtifactAction::IfElse { then_actions, .. },
                        ] if matches!(
                            then_actions.as_slice(),
                            [
                                ArtifactAction::Emit { .. },
                                ArtifactAction::ForEach { body, .. },
                            ] if matches!(
                                body.as_slice(),
                                [ArtifactAction::IfElse { .. }, ArtifactAction::IfElse { .. }]
                            )
                        )
                    )
                })
        }),
        "selected branch-local loop body should preserve both runtime-if actions"
    );
}

#[test]
fn checks_step_return_match_arm_runtime_if_branch_multiple_for_each_prefixes() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                    }\n                } else {\n                    emit \"return-match ready disabled branch\";",
        "                    }\n                    for Job { phase: job_phase } in jobs {\n                        emit \"return-match ready second branch loop item\";\n                        send sink Notice(job_phase);\n                    }\n                } else {\n                    emit \"return-match ready disabled branch\";",
    )
    .replace("proc Sink mailbox bounded(2)", "proc Sink mailbox bounded(4)");

    let checked = check_source(&source).expect("second branch-local for should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("second branch-local for should lower");

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
                            ArtifactAction::IfElse { then_actions, .. },
                        ] if matches!(
                            then_actions.as_slice(),
                            [
                                ArtifactAction::Emit { .. },
                                ArtifactAction::ForEach { .. },
                                ArtifactAction::ForEach { .. },
                            ]
                        )
                    )
                })
        }),
        "selected runtime-if branch should preserve both bounded for actions"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_branch_for_each_body_excessive_nested_runtime_if() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                        emit \"return-match ready branch loop item\";\n                        send sink Notice(job_phase);",
        "                        if (job_phase == Ready) {\n                            if (enabled == True) {\n                                if (job_phase == Ready) {\n                                    emit \"return-match ready excessive nested loop branch\";\n                                } else {\n                                    emit \"return-match ready excessive nested loop fallback\";\n                                }\n                            } else {\n                                emit \"return-match ready nested loop inner fallback\";\n                            }\n                        } else {\n                            emit \"return-match ready nested loop outer fallback\";\n                        }",
    );

    let err = check_source(&source)
        .expect_err("third loop-body runtime-if nesting level should fail closed");

    assert!(
        err.to_string()
            .contains("statement-level if action nesting exceeds maximum depth"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_branch_for_each_nested_for_each_body() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "                        emit \"return-match ready branch loop item\";\n                        send sink Notice(job_phase);",
        "                        for nested in jobs {\n                            emit \"return-match ready nested loop item\";\n                        }",
    );

    let err = check_source(&source).expect_err("nested branch-local for body should fail");

    assert!(
        err.to_string()
            .contains("nested for loops are not supported in this source slice"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_branch_for_each_missing_send_authority() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "ProcResult<WorkerState> ! [emit, spawn, send]",
        "ProcResult<WorkerState> ! [emit, spawn]",
    );

    let err = check_source(&source).expect_err("branch-local loop send should require authority");

    assert!(
        err.to_string()
            .contains("step uses effect send but does not declare it"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_arm_if_branch_loop_invalid_send_payload_template() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX
        .replace(
            "        send worker Envelope(Assign(Assignment {\n            phase: Done,\n            enabled: False,\n            jobs: List<Job,2>[\n                Job { phase: Done },\n                Job { phase: Ready },\n            ],\n        }));\n",
            "",
        )
        .replace(
            "                        emit \"return-match done branch loop item\";\n                        send sink Notice(job_phase);",
            "                        emit \"return-match done branch loop item\";\n                        send sink Notice(Assignment { phase: Done, enabled: False, jobs: jobs });",
        );

    let err = check_source(&source)
        .expect_err("unselected branch-local loop send payload should be statically validated");

    assert!(
        err.to_string()
            .contains("type Phase is not declared as a record"),
        "unexpected error: {err}"
    );
}

#[test]
fn checks_step_return_match_arm_runtime_if_direct_nested_if() {
    let source = PROCESS_RETURN_MATCH_ARM_IF_FOR_PREFIX.replace(
        "emit \"return-match ready enabled branch\";",
        "if (enabled == True) {\n                        emit \"return-match ready nested branch\";\n                    } else {\n                        emit \"return-match ready nested fallback\";\n                    }",
    );

    let checked = check_source(&source).expect("direct nested arm-local if should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("direct nested arm-local if should lower");

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
                            ArtifactAction::IfElse { then_actions, .. },
                        ] if matches!(
                            then_actions.as_slice(),
                            [
                                ArtifactAction::IfElse { .. },
                                ArtifactAction::ForEach { .. },
                            ]
                        )
                    )
                })
        }),
        "selected arm nested runtime-if should lower as typed branch actions"
    );
}

fn branch_has_loop_send(actions: &[CheckedAction]) -> bool {
    matches!(
        actions,
        [
            CheckedAction::Emit { .. },
            CheckedAction::ForEach {
                element,
                collection,
                body,
                max_items,
            },
        ] if *max_items == 2
            && matches!(
                collection,
                CheckedValueTemplate::RecordField { field, .. }
                    if field.as_str() == "jobs"
            )
            && loop_body_sends_phase(body, element.id())
    )
}

fn loop_body_sends_phase(
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

fn is_typed_if_for_transition(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::Emit { .. },
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if (branch_has_artifact_loop_send(then_actions)
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }]))
            || (matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
                && branch_has_artifact_loop_send(else_actions))
    )
}

fn branch_has_artifact_loop_send(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::ForEach { body, .. },
        ] if matches!(
            body.as_slice(),
            [
                ArtifactAction::Emit { .. },
                ArtifactAction::Send {
                    payload: Some(ArtifactValueTemplate::RecordField { record, field, .. }),
                    ..
                },
            ] if field == "phase"
                && matches!(
                    record.as_ref(),
                    ArtifactValueTemplate::LoopElement { .. }
                )
        )
    )
}

fn loop_body_has_nested_branch(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [ArtifactAction::IfElse { then_actions, .. }]
            if then_actions
                .iter()
                .any(|action| matches!(action, ArtifactAction::IfElse { .. }))
    )
}
