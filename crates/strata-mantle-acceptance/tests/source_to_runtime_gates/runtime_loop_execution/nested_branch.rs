use super::*;

const STEM: &str = "runtime_for_each_nested_if_actions";
const SOURCE: &str = "examples/runtime_for_each_nested_if_actions.str";
const ARTIFACT: &str = "target/strata/runtime_for_each_nested_if_actions.mta";

#[test]
fn runtime_for_each_nested_if_actions_branch_through_typed_loop_fields() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("batch outer true inner true"));
    assert!(stdout.contains("batch outer true inner false"));
    assert!(stdout.contains("batch outer false"));
    assert!(stdout.contains("worker reported true"));
    assert!(stdout.contains("worker reported false"));

    let artifact = gate.read_artifact(ARTIFACT);
    assert_nested_loop_branch_shape(&artifact);
    assert_no_executable_source_aliases(&artifact);

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":0"#,
            r#""element":"CheckFlags{outer_flag:True,inner_flag:True}""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""loop_index":0"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"else""#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"else""#,
            r#""loop_index":2"#,
            r#""condition":"False""#,
        ],
    );

    let first_iteration = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":0"#,
        ],
    );
    let outer_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch_path":[1,12288]"#,
            r#""branch":"then""#,
            r#""loop_index":0"#,
        ],
    );
    let nested_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch_path":[1,12288,4096]"#,
            r#""branch":"then""#,
            r#""loop_index":0"#,
        ],
    );
    let first_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":0,"text":"batch outer true inner true""#,
    );
    let second_iteration = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":1"#,
        ],
    );
    let second_outer_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch_path":[1,12288]"#,
            r#""branch":"then""#,
            r#""loop_index":1"#,
        ],
    );
    let nested_else = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch_path":[1,12288,4096]"#,
            r#""branch":"else""#,
            r#""loop_index":1"#,
        ],
    );
    let second_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":1,"text":"batch outer true inner false""#,
    );
    let third_iteration = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":2"#,
        ],
    );
    let outer_else = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch_path":[1,12288]"#,
            r#""branch":"else""#,
            r#""loop_index":2"#,
        ],
    );
    let third_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":2,"text":"batch outer false""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );

    assert!(first_iteration < outer_then);
    assert!(outer_then < nested_then);
    assert!(nested_then < first_output);
    assert!(first_output < second_iteration);
    assert!(second_iteration < second_outer_then);
    assert!(second_outer_then < nested_else);
    assert!(nested_else < second_output);
    assert!(second_output < third_iteration);
    assert!(third_iteration < outer_else);
    assert!(outer_else < third_output);
    assert!(third_output < loop_complete);
}

#[test]
fn runtime_for_each_nested_if_actions_rejects_deeper_branch_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_for_each_nested_if_actions_deep_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_for_each_nested_if_actions_deep.mta";
    let invalid_trace_stem = "runtime_for_each_nested_if_actions_deep";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    add_deeper_nested_loop_branch(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("runtime if action nesting exceeds maximum depth"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn assert_nested_loop_branch_shape(artifact: &MantleArtifact) {
    let flags_type = value_type_id(artifact, "CheckFlags");
    let bool_type = value_type_id(artifact, "Bool");
    let batch_worker = artifact_process(artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");

    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 3,
                body,
            },
        ] if element.ty == flags_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    condition,
                    then_actions,
                    else_actions,
                }] if matches_loop_record_bool_condition(
                        condition,
                        flags_type,
                        bool_type,
                        element.id,
                        0,
                    )
                    && matches!(
                        then_actions.as_slice(),
                        [ArtifactAction::IfElse {
                            condition,
                            then_actions,
                            else_actions,
                        }] if matches_loop_record_bool_condition(
                                condition,
                                flags_type,
                                bool_type,
                                element.id,
                                1,
                            )
                            && matches_send_loop_record_field(
                                then_actions.as_slice(),
                                flags_type,
                                bool_type,
                                element.id,
                                1,
                            )
                            && matches_send_loop_record_field(
                                else_actions.as_slice(),
                                flags_type,
                                bool_type,
                                element.id,
                                1,
                            )
                    )
                    && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
            )
    ));
}

fn matches_loop_record_bool_condition(
    condition: &ArtifactValueTemplate,
    flags_type: TypeId,
    bool_type: TypeId,
    element: mantle_artifact::LoopElementId,
    field_id: u32,
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
            && matches_loop_record_field(left, flags_type, bool_type, element, field_id)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn matches_send_loop_record_field(
    actions: &[ArtifactAction],
    flags_type: TypeId,
    bool_type: TypeId,
    element: mantle_artifact::LoopElementId,
    field_id: u32,
) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                payload: Some(payload),
                ..
            },
        ] if matches_loop_record_field(payload, flags_type, bool_type, element, field_id)
    )
}

fn matches_loop_record_field(
    template: &ArtifactValueTemplate,
    flags_type: TypeId,
    bool_type: TypeId,
    element: mantle_artifact::LoopElementId,
    field_id: u32,
) -> bool {
    matches!(
        template,
        ArtifactValueTemplate::RecordField { ty, record, field }
            if *ty == bool_type
                && field.as_u32() == field_id
                && matches!(
                    record.as_ref(),
                    ArtifactValueTemplate::LoopElement {
                        ty,
                        element: field_element,
                    } if *ty == flags_type && *field_element == element
                )
    )
}

fn assert_no_executable_source_aliases(artifact: &MantleArtifact) {
    assert!(
        !artifact.encode().lines().any(|line| {
            line.ends_with("=outer")
                || line.ends_with("=inner")
                || line.contains("debug_name=outer")
                || line.contains("debug_name=inner")
        }),
        "artifact must not dispatch through source pattern aliases"
    );
}

fn add_deeper_nested_loop_branch(artifact: &mut MantleArtifact) {
    let batch_worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact process should exist");
    let transition = batch_worker
        .transitions
        .first_mut()
        .expect("BatchWorker transition should exist");
    let ArtifactAction::ForEach { body, .. } = transition
        .actions
        .get_mut(1)
        .expect("BatchWorker should have a loop action")
    else {
        panic!("BatchWorker second action should be for_each");
    };
    let ArtifactAction::IfElse { then_actions, .. } = body
        .first_mut()
        .expect("loop body should have an outer if action")
    else {
        panic!("loop body action should be if_else");
    };
    let ArtifactAction::IfElse {
        condition,
        then_actions,
        ..
    } = then_actions
        .first_mut()
        .expect("outer then branch should have nested if action")
    else {
        panic!("outer then action should be nested if_else");
    };
    then_actions.insert(
        0,
        ArtifactAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![ArtifactAction::Emit {
                output: mantle_artifact::OutputId::new(0),
            }],
            else_actions: vec![ArtifactAction::Emit {
                output: mantle_artifact::OutputId::new(1),
            }],
        },
    );
}
