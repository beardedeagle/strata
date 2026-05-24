use super::support::*;

#[test]
fn runtime_final_if_nested_terminal_if_checks_and_lowers_to_typed_next_state() {
    let checked = check_source(RUNTIME_FINAL_IF_NESTED_TERMINAL_IF)
        .expect("nested terminal runtime if source should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let transition = only_transition(worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Continue);
    assert_nested_checked_next_state(&transition.next_state());

    let artifact = lower_to_artifact(&checked, RUNTIME_FINAL_IF_NESTED_TERMINAL_IF)
        .expect("nested terminal runtime if source should lower");
    assert_nested_artifact_next_state(&artifact);
    assert_no_executable_source_aliases(&artifact);
}

#[test]
fn runtime_final_if_nested_terminal_if_rejects_third_terminal_branch() {
    let source = RUNTIME_FINAL_IF_NESTED_TERMINAL_IF.replace(
        "                return Continue(OuterTrueInnerTrue);",
        "                if (inner == True) {\n                    return Continue(OuterTrueInnerTrue);\n                } else {\n                    return Continue(OuterTrueInnerFalse);\n                }",
    );
    let error = check_source(&source).expect_err("third-level terminal runtime if must fail");
    assert!(
        error
            .to_string()
            .contains("next_state runtime if nesting exceeds maximum depth of 2"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_terminal_if_rejects_divergent_leaf_results() {
    let source = RUNTIME_FINAL_IF_NESTED_TERMINAL_IF.replace(
        "return Continue(OuterTrueInnerFalse);",
        "return Stop(OuterTrueInnerFalse);",
    );
    let error = check_source(&source).expect_err("nested leaves must return compatible results");
    assert!(
        error
            .to_string()
            .contains("runtime if branches must return the same step result"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_terminal_if_rejects_missing_leaf_effect_authority() {
    let source = RUNTIME_FINAL_IF_NESTED_TERMINAL_IF.replace(
        "ProcResult<WorkerState> ! [emit]",
        "ProcResult<WorkerState> ! []",
    );
    let error = check_source(&source).expect_err("nested leaf effect must require authority");
    assert!(
        error
            .to_string()
            .contains("step uses effect emit but does not declare it"),
        "{error}"
    );
}

#[test]
fn runtime_final_if_nested_terminal_if_rejects_branch_local_process_ref() {
    let source = RUNTIME_FINAL_IF_NESTED_TERMINAL_IF
        .replace(
            "proc Worker mailbox bounded(1) {",
            "record PeerState;\n\nenum PeerMsg { Help }\n\nproc Peer mailbox bounded(1) {\n    type State = PeerState;\n    type Msg = PeerMsg;\n\n    fn init() -> PeerState ! [] ~ [] @det {\n        return PeerState;\n    }\n\n    fn step(state: PeerState, Help) -> ProcResult<PeerState> ! [] ~ [] @det {\n        return Continue(state);\n    }\n}\n\nproc Worker mailbox bounded(1) {",
        )
        .replace(
            "ProcResult<WorkerState> ! [emit]",
            "ProcResult<WorkerState> ! [spawn, emit]",
        )
        .replace(
            "                emit \"worker terminal outer true inner true\";",
            "                let extra: ProcessRef<Peer> = spawn Peer;",
        );
    let error = check_source(&source).expect_err("terminal branch process ref must fail");
    assert!(
        error
            .to_string()
            .contains("final-position runtime if branch cannot bind process reference extra"),
        "{error}"
    );
}

fn assert_nested_checked_next_state(next_state: &CheckedNextState) {
    assert!(matches!(
        next_state,
        CheckedNextState::IfElse {
            then_state,
            else_state,
            ..
        } if nested_checked_branch_returns_values(then_state.as_ref())
            && nested_checked_branch_returns_values(else_state.as_ref())
    ));
}

fn nested_checked_branch_returns_values(next_state: &CheckedNextState) -> bool {
    matches!(
        next_state,
        CheckedNextState::IfElse {
            then_state,
            else_state,
            ..
        } if matches!(then_state.as_ref(), CheckedNextState::Value(_))
            && matches!(else_state.as_ref(), CheckedNextState::Value(_))
    )
}

fn assert_nested_artifact_next_state(artifact: &MantleArtifact) {
    let bool_type = artifact_type_id(artifact, "Bool");
    let flags_type = artifact_type_id(artifact, "CheckFlags");
    let worker = artifact_process(artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");
    assert!(transition.effects.contains(&ArtifactEffect::Emit));
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "outer_flag")
            && nested_artifact_branch_returns_values(then_state.as_ref(), bool_type, flags_type)
            && nested_artifact_branch_returns_values(else_state.as_ref(), bool_type, flags_type)
    ));
}

fn nested_artifact_branch_returns_values(
    next_state: &NextState,
    bool_type: TypeId,
    flags_type: TypeId,
) -> bool {
    matches!(
        next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "inner_flag")
            && matches!(then_state.as_ref(), NextState::Value(_))
            && matches!(else_state.as_ref(), NextState::Value(_))
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
    let worker_index = artifact
        .processes
        .iter()
        .position(|candidate| candidate.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let worker_id = ProcessId::from_index(worker_index).expect("Worker process index should fit");
    let transition_prefix = format!("process.{}.transition.", worker_id.as_u32());
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .filter(|line| line.starts_with(&transition_prefix))
            .any(|line| line.ends_with("=outer") || line.ends_with("=inner")),
        "nested terminal runtime if artifact must not dispatch through source aliases"
    );
}
