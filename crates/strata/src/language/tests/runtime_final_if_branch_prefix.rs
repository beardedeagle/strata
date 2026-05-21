use super::support::*;

#[test]
fn runtime_final_if_nested_if_actions_check_and_lower_to_typed_prefix() {
    let checked = check_source(RUNTIME_FINAL_IF_NESTED_IF_ACTIONS)
        .expect("final-position nested if source should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let transition = only_transition(worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Continue);
    assert!(matches!(
        transition.next_state(),
        CheckedNextState::IfElse {
            then_state,
            else_state,
            ..
        } if matches!(then_state.as_ref(), CheckedNextState::Current)
            && matches!(else_state.as_ref(), CheckedNextState::Current)
    ));
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if matches!(then_actions.as_slice(), [CheckedAction::IfElse { .. }])
            && matches!(else_actions.as_slice(), [CheckedAction::IfElse { .. }])
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_FINAL_IF_NESTED_IF_ACTIONS)
        .expect("final-position nested if source should lower");
    assert_final_if_nested_action_shape(&artifact);
    assert_no_executable_source_aliases(&artifact);
}

#[test]
fn runtime_final_if_nested_if_actions_rejects_deeper_direct_branch() {
    let source = RUNTIME_FINAL_IF_NESTED_IF_ACTIONS.replace(
        "                emit \"worker final outer true inner true\";",
        "                if (inner == True) {\n                    emit \"too deep\";\n                } else {\n                    emit \"still too deep\";\n                }",
    );
    let error = check_source(&source).expect_err("third-level runtime if action must fail");
    assert!(
        error
            .to_string()
            .contains("statement-level if action nesting exceeds maximum depth of 2"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_if_actions_rejects_branch_local_process_ref() {
    let source = RUNTIME_FINAL_IF_NESTED_IF_ACTIONS.replace(
        "                emit \"worker final outer true inner true\";",
        "                let extra: ProcessRef<Reporter> = spawn Reporter;",
    );
    let error = check_source(&source).expect_err("nested branch process ref must fail");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches cannot bind process references"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_if_actions_rejects_statement_branch_return() {
    let source = RUNTIME_FINAL_IF_NESTED_IF_ACTIONS.replace(
        "                emit \"worker final outer true inner true\";",
        "                return Continue(state);",
    );
    let error = check_source(&source).expect_err("statement branch return must fail");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches must not return"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_if_actions_requires_declared_effect_authority() {
    let source = RUNTIME_FINAL_IF_NESTED_IF_ACTIONS.replace(
        "ProcResult<WorkerState> ! [spawn, emit, send]",
        "ProcResult<WorkerState> ! [spawn, send]",
    );
    let error = check_source(&source).expect_err("branch emit authority must fail");
    assert!(
        error
            .to_string()
            .contains("step uses effect emit but does not declare it"),
        "{error}"
    );
}

fn assert_final_if_nested_action_shape(artifact: &MantleArtifact) {
    let bool_type = artifact_type_id(artifact, "Bool");
    let flags_type = artifact_type_id(artifact, "CheckFlags");
    let reporter_process = artifact_process_id(artifact, "Reporter");
    let worker = artifact_process(artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");

    assert!(transition.effects.contains(&ArtifactEffect::Spawn));
    assert!(transition.effects.contains(&ArtifactEffect::Emit));
    assert!(transition.effects.contains(&ArtifactEffect::Send));
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "outer_flag")
            && matches!(then_state.as_ref(), NextState::Current)
            && matches!(else_state.as_ref(), NextState::Current)
    ));

    let [
        ArtifactAction::Spawn {
            target,
            process_ref,
        },
        ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        },
    ] = transition.actions.as_slice()
    else {
        panic!(
            "Worker transition should spawn Reporter before final-position branch action prefix"
        );
    };
    assert_eq!(*target, reporter_process);
    assert!(condition_is_bool_field_check(
        condition,
        bool_type,
        flags_type,
        "outer_flag"
    ));
    assert_final_branch_contains_nested_if(then_actions, bool_type, flags_type, *process_ref);
    assert_final_branch_contains_nested_if(else_actions, bool_type, flags_type, *process_ref);
}

fn assert_final_branch_contains_nested_if(
    actions: &[ArtifactAction],
    bool_type: TypeId,
    flags_type: TypeId,
    process_ref: ProcessRefId,
) {
    assert!(matches!(
        actions,
        [ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        }] if condition_is_bool_field_check(condition, bool_type, flags_type, "inner_flag")
            && selected_inner_branch_actions_use_typed_payload(
                then_actions,
                bool_type,
                flags_type,
                process_ref
            )
            && selected_inner_branch_actions_use_typed_payload(
                else_actions,
                bool_type,
                flags_type,
                process_ref
            )
    ));
}

fn selected_inner_branch_actions_use_typed_payload(
    actions: &[ArtifactAction],
    bool_type: TypeId,
    flags_type: TypeId,
    process_ref: ProcessRefId,
) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(target_ref),
                message,
                payload: Some(payload),
            },
        ] if *target_ref == process_ref
            && *message == MessageId::new(0)
            && payload_is_bool_field(payload, bool_type, flags_type, "inner_flag")
    )
}

fn condition_is_bool_field_check(
    condition: &ArtifactValueTemplate,
    bool_type: TypeId,
    flags_type: TypeId,
    expected_field: &str,
) -> bool {
    matches!(
        condition,
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator: ArtifactValueEqualityOperator::Equal,
            left,
            right,
        } if *ty == bool_type
            && *operand_ty == bool_type
            && payload_is_bool_field(left, bool_type, flags_type, expected_field)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn payload_is_bool_field(
    template: &ArtifactValueTemplate,
    bool_type: TypeId,
    flags_type: TypeId,
    expected_field: &str,
) -> bool {
    matches!(
        template,
        ArtifactValueTemplate::RecordField {
            ty,
            record,
            field,
        } if *ty == bool_type
            && field == expected_field
            && matches!(
                record.as_ref(),
                ArtifactValueTemplate::ReceivedPayload { ty } if *ty == flags_type
            )
    )
}

fn artifact_process_id(artifact: &MantleArtifact, process: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}

fn artifact_process<'a>(
    artifact: &'a MantleArtifact,
    process: &str,
) -> &'a mantle_artifact::ArtifactProcess {
    artifact
        .processes
        .iter()
        .find(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"))
}

fn assert_no_executable_source_aliases(artifact: &MantleArtifact) {
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .filter(|line| line.starts_with("process.1.transition."))
            .any(|line| { line.ends_with("=outer") || line.ends_with("=inner") }),
        "final-position nested branch artifact must not dispatch through source aliases"
    );
}
