use super::*;

#[test]
fn runtime_for_each_if_branches_inside_loop_body_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each_if");
    let run = gate.check_build_run(
        "examples/runtime_for_each_if.str",
        "target/strata/runtime_for_each_if.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("batch selected true"));
    assert!(stdout.contains("batch selected false"));
    assert!(stdout.contains("worker handled true"));
    assert!(stdout.contains("worker handled false"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each_if.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
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
                max_items: 2,
                body,
            },
        ] if element.ty == bool_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    condition: ArtifactValueTemplate::BooleanBinary {
                        ty,
                        operator: ArtifactValueBooleanOperator::And,
                        left,
                        right,
                    },
                    then_actions,
                    else_actions,
                }] if *ty == bool_type
                    && matches!(
                        left.as_ref(),
                        ArtifactValueTemplate::Equality {
                            ty,
                            operand_ty,
                            operator: ArtifactValueEqualityOperator::NotEqual,
                            left,
                            right,
                        } if *ty == bool_type
                            && *operand_ty == bool_type
                            && matches!(
                                left.as_ref(),
                                ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: condition_element,
                                } if *ty == bool_type && *condition_element == element.id
                            )
                            && matches!(
                                right.as_ref(),
                                ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("False")
                            )
                    )
                    && matches!(
                        right.as_ref(),
                        ArtifactValueTemplate::BooleanNot {
                            ty,
                            operand,
                        } if *ty == bool_type
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
                                    && matches!(
                                        left.as_ref(),
                                        ArtifactValueTemplate::LoopElement {
                                            ty,
                                            element: condition_element,
                                        } if *ty == bool_type && *condition_element == element.id
                                    )
                                    && matches!(
                                        right.as_ref(),
                                        ArtifactValueTemplate::Literal { ty, value } if *ty == bool_type && value == &artifact_value("False")
                                    )
                            )
                    )
                    && matches!(
                        then_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    )
                    && matches!(
                        else_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    )
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop branch artifact must not dispatch through the source loop binding name"
    );

    let trace = gate.read_trace("runtime_for_each_if");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":0"#,
            r#""element":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""branch_path":[1,12288]"#,
            r#""loop_element_id":0"#,
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
            r#""scope":"action""#,
            r#""branch_path":[1,12288]"#,
            r#""loop_element_id":0"#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );

    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let batch_true_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action""#,
    );
    let batch_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":0,"text":"batch selected true""#,
    );
    let true_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let batch_false_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action""#,
    );
    let batch_false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":1,"text":"batch selected false""#,
    );
    let false_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let worker_true_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","branch":"then","scope":"action""#,
    );
    let worker_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":2,"text":"worker handled true""#,
    );
    let worker_false_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","branch":"else","scope":"action""#,
    );
    let worker_false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":3,"text":"worker handled false""#,
    );

    assert!(first_iteration < batch_true_branch);
    assert!(batch_true_branch < batch_true_output);
    assert!(batch_true_output < true_send);
    assert!(true_send < second_iteration);
    assert!(second_iteration < batch_false_branch);
    assert!(batch_false_branch < batch_false_output);
    assert!(batch_false_output < false_send);
    assert!(false_send < loop_complete);
    assert!(loop_complete < worker_true_branch);
    assert!(worker_true_branch < worker_true_output);
    assert!(worker_true_output < worker_false_branch);
    assert!(worker_false_branch < worker_false_output);
}
