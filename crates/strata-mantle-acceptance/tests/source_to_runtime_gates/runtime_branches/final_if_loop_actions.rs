use super::super::support::*;

const STEM: &str = "runtime_final_if_guarded_loop";
const SOURCE: &str = "examples/runtime_final_if_guarded_loop.str";
const ARTIFACT: &str = "target/strata/runtime_final_if_guarded_loop.mta";

#[test]
fn runtime_final_if_loop_actions_prefix_selected_next_state_branch() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout.matches("worker saw enabled item").count(),
        1,
        "only the enabled true item should emit\n{stdout}"
    );
    assert_eq!(
        stdout.matches("worker disabled").count(),
        1,
        "only the disabled worker should emit disabled output\n{stdout}"
    );

    let artifact = gate.read_artifact(ARTIFACT);
    assert_final_if_guarded_loop_shape(&artifact);
    assert_no_executable_source_aliases(&artifact);

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":4,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":4,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.contains(r#""event":"loop_started""#))
            .count(),
        2,
        "both enabled workers should execute the loop, and the disabled worker should not"
    );
    assert!(
        !trace.contains(r#""event":"loop_started","pid":4"#),
        "disabled final-position branch must not execute loop actions"
    );

    let outer_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch":"then""#,
            r#""scope":"action""#,
        ],
    );
    let loop_start = trace_line_index(&trace, r#""event":"loop_started","pid":2"#);
    let first_iteration = trace_line_index_with_fields(
        &trace,
        &[r#""event":"loop_iteration","pid":2"#, r#""index":0"#],
    );
    let inner_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch":"then""#,
            r#""loop_index":0"#,
            r#""condition":"True""#,
        ],
    );
    let enabled_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker saw enabled item""#,
    );
    let second_iteration = trace_line_index_with_fields(
        &trace,
        &[r#""event":"loop_iteration","pid":2"#, r#""index":1"#],
    );
    let inner_else = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch":"else""#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    let loop_complete = trace_line_index(&trace, r#""event":"loop_completed","pid":2"#);
    let next_state_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
        ],
    );

    assert!(next_state_then < outer_then);
    assert!(outer_then < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < inner_then);
    assert!(inner_then < enabled_output);
    assert!(enabled_output < second_iteration);
    assert!(second_iteration < inner_else);
    assert!(inner_else < loop_complete);
}

#[test]
fn runtime_final_if_loop_actions_rejects_nested_loop_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_final_if_guarded_loop_nested_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_final_if_guarded_loop_nested.mta";
    let invalid_trace_stem = "runtime_final_if_guarded_loop_nested";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    add_nested_loop_under_final_if_branch(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("nested for loops are not supported"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn assert_final_if_guarded_loop_shape(artifact: &MantleArtifact) {
    let bool_type = value_type_id(artifact, "Bool");
    let flags_type = value_type_id(artifact, "CheckFlags");
    let worker = artifact_process(artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");
    assert_eq!(transition.effects, [ArtifactEffect::Emit]);
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "enabled")
            && matches!(then_state.as_ref(), NextState::Current)
            && matches!(else_state.as_ref(), NextState::Current)
    ));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        }] if condition_is_bool_field_check(condition, bool_type, flags_type, "enabled")
            && matches!(
                then_actions.as_slice(),
                [ArtifactAction::ForEach {
                    element,
                    collection,
                    max_items: 2,
                    body,
                }] if element.ty == bool_type
                    && collection_is_flags_field(collection, flags_type, "items")
                    && matches!(
                        body.as_slice(),
                        [ArtifactAction::IfElse {
                            condition,
                            then_actions,
                            else_actions,
                        }] if condition_is_loop_bool_check(condition, bool_type, element.id)
                            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
                            && else_actions.is_empty()
                    )
            )
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));
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
            && matches!(
                left.as_ref(),
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
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn collection_is_flags_field(
    collection: &ArtifactValueTemplate,
    flags_type: TypeId,
    expected_field: &str,
) -> bool {
    matches!(
        collection,
        ArtifactValueTemplate::RecordField { record, field, .. }
            if field == expected_field
                && matches!(
                    record.as_ref(),
                    ArtifactValueTemplate::ReceivedPayload { ty } if *ty == flags_type
                )
    )
}

fn condition_is_loop_bool_check(
    condition: &ArtifactValueTemplate,
    bool_type: TypeId,
    element_id: mantle_artifact::LoopElementId,
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
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::LoopElement { ty, element }
                    if *ty == bool_type && *element == element_id
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn assert_no_executable_source_aliases(artifact: &MantleArtifact) {
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .any(|line| { line.ends_with("=item") || line.contains("debug_name=item") }),
        "final-position guarded loop artifact must not dispatch through source aliases"
    );
}

fn add_nested_loop_under_final_if_branch(artifact: &mut MantleArtifact) {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let transition = worker
        .transitions
        .first_mut()
        .expect("Worker should have one Check transition");
    let [ArtifactAction::IfElse { then_actions, .. }] = transition.actions.as_mut_slice() else {
        panic!("Worker transition should contain only the final-position branch action prefix");
    };
    let nested_loop = then_actions
        .first()
        .expect("then branch should contain the bounded loop")
        .clone();
    let [ArtifactAction::ForEach { body, .. }] = then_actions.as_mut_slice() else {
        panic!("enabled final-position branch should contain only the bounded loop");
    };
    body.push(nested_loop);
}
