use super::support::*;

#[test]
fn runtime_if_else_checks_and_lowers_to_mantle_control_flow() {
    let checked = check_source(RUNTIME_IF_ELSE).expect("runtime if source should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "WarmReady", "ColdReady"]
    );
    assert_eq!(
        checked.outputs(),
        ["worker took warm branch", "worker took cold branch"]
    );

    let transition = only_transition(worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(matches!(
        transition.next_state(),
        CheckedNextState::IfElse {
            condition: CheckedValueTemplate::ReceivedPayload { .. },
            ..
        }
    ));
    assert!(matches!(
        transition.actions(),
        [CheckedAction::IfElse {
            condition: CheckedValueTemplate::ReceivedPayload { .. },
            then_actions,
            else_actions,
        }] if matches!(then_actions.as_slice(), [CheckedAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [CheckedAction::Emit { .. }])
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_IF_ELSE).expect("runtime if should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let artifact_transition = worker_artifact
        .transitions
        .first()
        .expect("Worker artifact transition should exist");
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        &artifact_transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::ReceivedPayload { ty },
            ..
        } if *ty == bool_type
    ));
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::ReceivedPayload { ty },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));
}

#[test]
fn runtime_if_else_rejects_non_bool_condition() {
    let source = RUNTIME_IF_ELSE.replace("if (flag)", "if (state)");
    let error = check_source(&source).expect_err("runtime if condition must be Bool");
    assert!(
        error
            .to_string()
            .contains("if condition must have type Bool"),
        "{error}"
    );
}

#[test]
fn runtime_if_else_rejects_step_result_mismatch() {
    let source = RUNTIME_IF_ELSE.replace("return Stop(WarmReady);", "return Continue(WarmReady);");
    let error = check_source(&source).expect_err("runtime if branch results must match");
    assert!(
        error
            .to_string()
            .contains("runtime if branches must return the same step result"),
        "{error}"
    );
}

#[test]
fn runtime_if_else_rejects_missing_else_branch_return_with_precise_diagnostic() {
    let source = RUNTIME_IF_ELSE.replace("return Stop(ColdReady);", "emit \"missing return\";");

    let error = parse_source(&source).expect_err("runtime if else branch must return");

    assert!(
        error
            .to_string()
            .contains("runtime return if else branch must contain a top-level return"),
        "{error}"
    );
}

#[test]
fn runtime_if_else_rejects_branch_effect_without_declared_authority() {
    let source = RUNTIME_IF_ELSE.replace(
        "fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [emit] ~ [] @det",
        "fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [] ~ [] @det",
    );
    let error = check_source(&source).expect_err("branch effects must be declared");
    assert!(
        error
            .to_string()
            .contains("step uses effect emit but does not declare it"),
        "{error}"
    );
}

#[test]
fn runtime_if_else_rejects_nested_statement_branch_inside_final_branch() {
    let source = RUNTIME_IF_ELSE.replace(
        "emit \"worker took warm branch\";",
        "if (flag) { emit \"nested warm\"; } else { emit \"nested cold\"; }",
    );
    let error = check_source(&source).expect_err("nested runtime branch must be rejected");
    assert!(
        error
            .to_string()
            .contains("nested statement-level if branches are not supported"),
        "{error}"
    );
}
