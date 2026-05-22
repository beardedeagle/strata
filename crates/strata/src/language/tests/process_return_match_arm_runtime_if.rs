use super::support::*;

#[test]
fn checks_step_return_match_arm_runtime_if_prefixes_are_selected_and_typed() {
    let checked = check_source(PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX)
        .expect("arm-local runtime if prefixes should check");
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
        "branch-local sends in selected return-match arms should discover Sink payload cases"
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
            CheckedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            },
        ] = transition.actions()
        else {
            panic!(
                "selected return-match arm should lower uniform actions before one typed runtime-if action: {:?}",
                transition.actions()
            );
        };
        assert!(
            matches!(
                condition,
                CheckedValueTemplate::Equality {
                    operator: CheckedValueEqualityOperator::Equal,
                    right,
                    ..
                } if matches!(
                        right.as_ref(),
                        CheckedValueTemplate::Literal(value) if value.label() == "True"
                    )
            ),
            "return-match arm runtime-if condition should lower as a typed payload template"
        );
        if payload.contains("phase:Ready") {
            assert_eq!(transition.step_result(), CheckedStepResult::Continue);
            assert_eq!(
                transition.next_state(),
                CheckedNextState::Value(checked_state_id(1))
            );
            assert!(
                matches!(
                    (then_actions.as_slice(), else_actions.as_slice()),
                    (
                        [CheckedAction::Emit { .. }, CheckedAction::Send { .. }],
                        [CheckedAction::Emit { .. }]
                    )
                ),
                "Ready arm should send only from the selected true branch"
            );
        } else {
            assert!(
                payload.contains("phase:Done"),
                "unexpected payload: {payload}"
            );
            assert_eq!(transition.step_result(), CheckedStepResult::Stop);
            assert_eq!(
                transition.next_state(),
                CheckedNextState::Value(checked_state_id(2))
            );
            assert!(
                matches!(
                    (then_actions.as_slice(), else_actions.as_slice()),
                    (
                        [CheckedAction::Emit { .. }],
                        [CheckedAction::Emit { .. }, CheckedAction::Send { .. }]
                    )
                ),
                "Done arm should send only from the selected false branch"
            );
        }
    }

    let artifact = lower_to_artifact(&checked, PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX)
        .expect("arm-local runtime if prefixes should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    assert!(worker_artifact.transitions.iter().all(|transition| {
        matches!(
            transition.actions.as_slice(),
            [
                ArtifactAction::Emit { .. },
                ArtifactAction::Spawn { .. },
                ArtifactAction::IfElse { .. },
            ]
        )
    }));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.contains("debug_name=flag")),
        "artifact execution must not dispatch through the source branch binding name"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_missing_effect_authority() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "fn step(state: WorkerState, Envelope(Assign(Assignment { phase: phase, flag: flag }))) -> ProcResult<WorkerState> ! [emit, spawn, send] ~ [] @det {",
        "fn step(state: WorkerState, Envelope(Assign(Assignment { phase: phase, flag: flag }))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {",
    );

    let err = check_source(&source).expect_err("branch emit authority should be required");

    assert!(
        err.to_string()
            .contains("step uses effect emit but does not declare it"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_runtime_if_invalid_send_payload() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX
        .replace(
            "        send worker Envelope(Assign(Assignment { phase: Done, flag: False }));\n",
            "",
        )
        .replace(
            "send sink Notice(Done);",
            "send sink Notice(Assignment { phase: Done, flag: False });",
        );

    let err = check_source(&source)
        .expect_err("unselected arm branch send payload should be statically validated");

    assert!(
        err.to_string()
            .contains("type Phase is not declared as a record"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_both_branches_empty() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "                if (flag == True) {\n                    emit \"return-match ready true branch\";\n                    send sink Notice(Ready);\n                } else {\n                    emit \"return-match ready false branch\";\n                }",
        "                if (flag == True) {\n                } else {\n                }",
    );

    let err = check_source(&source).expect_err("empty branch pair should fail closed");

    assert!(
        err.to_string()
            .contains("statement-level if branches cannot both be empty"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_nested_runtime_if_prefix() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "emit \"return-match ready true branch\";",
        "if (flag == True) {\n                        emit \"nested ready branch\";\n                    } else {\n                        emit \"nested ready fallback\";\n                    }",
    );

    let err = check_source(&source).expect_err("nested arm-local runtime if should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match arm cannot perform nested runtime if in this source slice"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_multiple_runtime_if_prefixes() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "                }\n                return Continue(SawReady);",
        "                }\n                if (flag == True) {\n                    emit \"second ready branch\";\n                } else {\n                    emit \"second ready fallback\";\n                }\n                return Continue(SawReady);",
    );

    let err = check_source(&source).expect_err("second direct arm-local runtime if should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match arm cannot perform more than one runtime if in this source slice"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_process_ref_binding() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "emit \"return-match ready true branch\";",
        "let other: ProcessRef<Sink> = spawn Sink;",
    );

    let err = check_source(&source).expect_err("branch-local process ref binding should fail");

    assert!(
        err.to_string()
            .contains("statement-level if branches cannot bind process references"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_runtime_if_branch_return() {
    let source = PROCESS_RETURN_MATCH_ARM_RUNTIME_IF_PREFIX.replace(
        "emit \"return-match ready true branch\";",
        "return Continue(SawReady);",
    );

    let err = check_source(&source).expect_err("branch-local return should fail");

    assert!(
        err.to_string()
            .contains("statement-level if branches must not return"),
        "unexpected error: {err}"
    );
}
