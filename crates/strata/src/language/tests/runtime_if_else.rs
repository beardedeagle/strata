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
            condition: CheckedValueTemplate::Equality {
                operator: CheckedValueEqualityOperator::Equal,
                left,
                right,
                ..
            },
            ..
        } if matches!(left.as_ref(), CheckedValueTemplate::ReceivedPayload { .. })
            && matches!(
                right.as_ref(),
                CheckedValueTemplate::Literal(value) if value.label() == "True"
            )
    ));
    assert!(matches!(
        transition.actions(),
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
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::Equal,
                left,
                right,
            },
            ..
        } if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("True")
            )
    ));
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty,
                operand_ty,
                operator: ArtifactValueEqualityOperator::Equal,
                left,
                right,
            },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("True")
            )
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));
}

#[test]
fn runtime_if_else_without_branch_effects_keeps_action_prefix_empty() {
    let source = r#"
module runtime_if_else_no_effects;

record MainState;
enum Bool {
    False,
    True,
}
enum MainMsg {
    Start,
}
enum WorkerState {
    Idle,
    WarmReady,
    ColdReady,
}
enum WorkerMsg {
    Branch(Bool),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let warm: ProcessRef<Worker> = spawn Worker;
        send warm Branch(True);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Branch(flag: Bool)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        if (flag == True) {
            return Stop(WarmReady);
        } else {
            return Stop(ColdReady);
        }
    }
}
"#;

    let checked = check_source(source).expect("runtime if without branch effects should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let transition = only_transition(worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(transition.actions().is_empty());
    assert!(matches!(
        transition.next_state(),
        CheckedNextState::IfElse {
            condition: CheckedValueTemplate::Equality { .. },
            ..
        }
    ));

    let artifact = lower_to_artifact(&checked, source).expect("runtime if should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let transition = worker
        .transitions
        .first()
        .expect("Worker artifact transition should exist");
    assert!(transition.actions.is_empty());
    assert!(matches!(
        transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::Equality { .. },
            ..
        }
    ));
}

#[test]
fn runtime_if_else_accepts_not_equal_payload_predicate() {
    let source = RUNTIME_IF_ELSE.replace("if (flag == True)", "if (flag != False)");
    let checked = check_source(&source).expect("runtime != predicate source should check");
    let artifact = lower_to_artifact(&checked, &source).expect("runtime != predicate should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let condition = match &worker.transitions[0].next_state {
        NextState::IfElse { condition, .. } => condition,
        other => panic!("expected if/else next state, got {other:?}"),
    };
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        condition,
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator: ArtifactValueEqualityOperator::NotEqual,
            left,
            right,
        } if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("False")
            )
    ));
}

#[test]
fn runtime_if_else_accepts_composed_payload_predicate() {
    let source = RUNTIME_IF_ELSE.replace(
        "if (flag == True)",
        "if (((flag == True) && !(flag == False)) || (flag != False))",
    );
    let checked = check_source(&source).expect("composed runtime predicate should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let transition = only_transition(worker);
    assert!(matches!(
        transition.next_state(),
        CheckedNextState::IfElse {
            condition: CheckedValueTemplate::BooleanBinary {
                operator: CheckedValueBooleanOperator::Or,
                left,
                right,
                ..
            },
            ..
        } if matches!(
                left.as_ref(),
                CheckedValueTemplate::BooleanBinary {
                    operator: CheckedValueBooleanOperator::And,
                    ..
                }
            )
            && matches!(
                right.as_ref(),
                CheckedValueTemplate::Equality {
                    operator: CheckedValueEqualityOperator::NotEqual,
                    ..
                }
            )
    ));

    let artifact =
        lower_to_artifact(&checked, &source).expect("composed runtime predicate should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let condition = match &worker.transitions[0].next_state {
        NextState::IfElse { condition, .. } => condition,
        other => panic!("expected if/else next state, got {other:?}"),
    };
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        condition,
        ArtifactValueTemplate::BooleanBinary {
            ty,
            operator: ArtifactValueBooleanOperator::Or,
            left,
            right,
        } if *ty == bool_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::BooleanBinary {
                    operator: ArtifactValueBooleanOperator::And,
                    ..
                }
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Equality {
                    operator: ArtifactValueEqualityOperator::NotEqual,
                    ..
                }
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=flag") || line.contains("debug_name=flag")),
        "composed predicate artifact must not dispatch through the source binding name"
    );
}

#[test]
fn runtime_if_else_accepts_direct_bool_payload_composition() {
    let source = RUNTIME_IF_ELSE.replace("if (flag == True)", "if (flag && !(flag == False))");
    let checked = check_source(&source).expect("direct Bool runtime predicate should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("direct Bool runtime predicate should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let condition = match &worker.transitions[0].next_state {
        NextState::IfElse { condition, .. } => condition,
        other => panic!("expected if/else next state, got {other:?}"),
    };
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        condition,
        ArtifactValueTemplate::BooleanBinary {
            ty,
            operator: ArtifactValueBooleanOperator::And,
            left,
            right,
        } if *ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::BooleanNot { ty, operand }
                    if *ty == bool_type
                        && matches!(
                            operand.as_ref(),
                            ArtifactValueTemplate::Equality {
                                ty,
                                operand_ty,
                                operator: ArtifactValueEqualityOperator::Equal,
                                left,
                                right,
                            } if *ty == bool_type
                                && *operand_ty == bool_type
                                && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
                                && matches!(
                                    right.as_ref(),
                                    ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("False")
                                )
                        )
            )
    ));
}

#[test]
fn statement_runtime_if_accepts_noop_branch_shapes() {
    let checked = check_source(RUNTIME_GUARD_NOOP).expect("guard no-op source should check");
    assert_eq!(
        checked.outputs(),
        ["guard saw true", "guard enabled", "guard saw false"]
    );
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let transition = only_transition(worker);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::IfElse {
                then_actions: first_then,
                else_actions: first_else,
                ..
            },
            CheckedAction::IfElse {
                then_actions: second_then,
                else_actions: second_else,
                ..
            },
            CheckedAction::IfElse {
                then_actions: third_then,
                else_actions: third_else,
                ..
            },
        ] if matches!(first_then.as_slice(), [CheckedAction::Emit { .. }])
            && first_else.is_empty()
            && matches!(second_then.as_slice(), [CheckedAction::Emit { .. }])
            && second_else.is_empty()
            && third_then.is_empty()
            && matches!(third_else.as_slice(), [CheckedAction::Emit { .. }])
    ));

    let artifact =
        lower_to_artifact(&checked, RUNTIME_GUARD_NOOP).expect("guard no-op source should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let transition = worker
        .transitions
        .first()
        .expect("Worker artifact transition should exist");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::IfElse {
                then_actions: first_then,
                else_actions: first_else,
                ..
            },
            ArtifactAction::IfElse {
                then_actions: second_then,
                else_actions: second_else,
                ..
            },
            ArtifactAction::IfElse {
                then_actions: third_then,
                else_actions: third_else,
                ..
            },
        ] if matches!(first_then.as_slice(), [ArtifactAction::Emit { .. }])
            && first_else.is_empty()
            && matches!(second_then.as_slice(), [ArtifactAction::Emit { .. }])
            && second_else.is_empty()
            && third_then.is_empty()
            && matches!(third_else.as_slice(), [ArtifactAction::Emit { .. }])
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=flag") || line.contains("debug_name=flag")),
        "no-op branch artifact must not dispatch through the source binding name"
    );
}

#[test]
fn statement_runtime_if_rejects_omitted_else_when_then_branch_is_empty() {
    let source = RUNTIME_GUARD_NOOP.replace(
        "        if (flag == True) {\n            emit \"guard saw true\";\n        }",
        "        if (flag == True) {\n        }",
    );
    let error = check_source(&source).expect_err("omitted else with empty then must fail closed");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches cannot both be empty"),
        "{error}"
    );
}

#[test]
fn statement_runtime_if_rejects_explicit_both_empty_branches() {
    let source = RUNTIME_GUARD_NOOP.replace(
        "        if (flag == True) {\n            emit \"guard saw true\";\n        }",
        "        if (flag == True) {\n        } else {\n        }",
    );
    let error = check_source(&source).expect_err("both empty branches must fail closed");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches cannot both be empty"),
        "{error}"
    );
}

#[test]
fn statement_runtime_if_rejects_action_nesting_above_limit() {
    let source = RUNTIME_GUARD_NOOP.replace(
        "        if (flag == True) {\n            emit \"guard saw true\";\n        }",
        "        if (flag == True) {\n            if (flag == True) {\n                if (flag == True) {\n                    emit \"too deep\";\n                }\n            }\n        }",
    );
    let error = check_source(&source).expect_err("over-limit nested branch must fail");
    assert!(
        error
            .to_string()
            .contains("statement-level if action nesting exceeds maximum depth of 2"),
        "{error}"
    );
}

#[test]
fn runtime_if_else_disambiguates_fieldless_variant_from_payload_type() {
    let source = RUNTIME_IF_ELSE.replace(
        "enum MainMsg {\n    Start,\n}",
        "enum OtherBool {\n    True,\n}\nenum MainMsg {\n    Start,\n}",
    );
    let checked = check_source(&source).expect("typed payload should disambiguate True");
    let artifact =
        lower_to_artifact(&checked, &source).expect("disambiguated runtime equality should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    let condition = match &worker.transitions[0].next_state {
        NextState::IfElse { condition, .. } => condition,
        other => panic!("expected if/else next state, got {other:?}"),
    };
    let bool_type = artifact_type_id(&artifact, "Bool");
    assert!(matches!(
        condition,
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator: ArtifactValueEqualityOperator::Equal,
            left,
            right,
        } if *ty == bool_type
            && *operand_ty == bool_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ReceivedPayload { ty } if *ty == bool_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("True")
            )
    ));
}

#[test]
fn runtime_if_else_rejects_non_bool_condition() {
    let source = RUNTIME_IF_ELSE.replace("if (flag == True)", "if (state)");
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
fn runtime_if_else_accepts_direct_statement_branch_inside_final_branch() {
    let source = RUNTIME_IF_ELSE.replace(
        "emit \"worker took warm branch\";",
        "if (flag) { emit \"nested warm\"; } else { emit \"nested cold\"; }",
    );
    check_source(&source).expect("direct nested branch in final-position branch should check");
}
