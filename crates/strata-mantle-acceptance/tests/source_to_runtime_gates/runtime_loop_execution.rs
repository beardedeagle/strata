use super::support::*;

#[path = "runtime_loop_execution/guarded_ref_loop.rs"]
mod guarded_ref_loop;
#[path = "runtime_loop_execution/guarded_ref_loop_jobs.rs"]
mod guarded_ref_loop_jobs;
#[path = "runtime_loop_execution/loop_element_projection.rs"]
mod loop_element_projection;

#[test]
fn runtime_for_each_iterates_over_payload_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_for_each");
    let run = gate.check_build_run(
        "examples/runtime_for_each.str",
        "target/strata/runtime_for_each.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Branch(True) to Worker"));
    assert!(stdout.contains("mantle: delivered Branch(False) to Worker"));
    assert!(stdout.contains("worker handled true"));
    assert!(stdout.contains("worker handled false"));

    let artifact = gate.read_artifact("target/strata/runtime_for_each.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    let batch_payload_type = batch_worker.message_variants[0]
        .payload_type
        .expect("Batch message should carry a list payload");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { ty },
                max_items: 2,
                body,
            },
        ] if element.ty == bool_type
            && *ty == batch_payload_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::Send {
                    payload: Some(ArtifactValueTemplate::LoopElement {
                        ty,
                        element: payload_element,
                    }),
                    ..
                }] if *ty == bool_type && *payload_element == element.id
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop element dispatch must not rely on the source binding name"
    );

    let trace = gate.read_trace("runtime_for_each");
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"BatchWorker""#,
            r#""element_id":0"#,
            r#""max_items":2"#,
            r#""item_count":2"#,
        ],
    );
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
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":1"#,
            r#""element":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""process":"BatchWorker""#,
            r#""iteration_count":2"#,
        ],
    );

    let loop_start = trace_line_index(
        &trace,
        r#""event":"loop_started","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let first_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let second_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":0,"text":"worker handled true""#,
    );
    let false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":1,"text":"worker handled false""#,
    );

    assert!(loop_start < first_iteration);
    assert!(first_iteration < first_send);
    assert!(first_send < second_iteration);
    assert!(second_iteration < second_send);
    assert!(second_send < loop_complete);
    assert!(loop_complete < true_output);
    assert!(true_output < false_output);
}

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

#[test]
fn runtime_for_each_if_noop_branch_traces_inside_loop_body() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_for_each_if_noop";
    const ARTIFACT: &str = "target/strata/runtime_for_each_if_noop.mta";
    let source = include_str!("../../../../examples/runtime_for_each_if.str")
        .replace(
            "module runtime_for_each_if;",
            "module runtime_for_each_if_noop;",
        )
        .replace(
            "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            } else {\n                emit \"batch selected false\";\n                send worker Branch(item);\n            }",
            "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            }",
        );
    let source = gate.write_target_source(STEM, &source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    let run = gate.check_build_run(source, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("batch selected true"));
    assert!(stdout.contains("worker handled true"));
    assert!(!stdout.contains("batch selected false"));
    assert!(!stdout.contains("worker handled false"));

    let artifact = gate.read_artifact(ARTIFACT);
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
                body,
                ..
            },
        ] if element.ty == bool_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    then_actions,
                    else_actions,
                    ..
                }] if matches!(
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
                    ) && else_actions.is_empty()
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop no-op branch artifact must not dispatch through the source loop binding name"
    );

    let trace = gate.read_trace(STEM);
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
    assert!(
        !trace.contains(
            r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#
        ),
        "selected no-op loop branch must not send the false item"
    );

    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let then_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action""#,
    );
    let true_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let else_noop_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let worker_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":1,"text":"worker handled true""#,
    );

    assert!(first_iteration < then_branch);
    assert!(then_branch < true_send);
    assert!(true_send < second_iteration);
    assert!(second_iteration < else_noop_branch);
    assert!(else_noop_branch < loop_complete);
    assert!(loop_complete < worker_true_output);
}

#[test]
fn runtime_guarded_for_each_branches_loop_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_guarded_for_each");
    let run = gate.check_build_run(
        "examples/runtime_guarded_for_each.str",
        "target/strata/runtime_guarded_for_each.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains(
        "mantle: delivered Batch(BatchRequest{enabled:True,items:List[True,False]}) to BatchWorker"
    ));
    assert!(stdout.contains(
        "mantle: delivered Batch(BatchRequest{enabled:False,items:List[True,False]}) to BatchWorker"
    ));
    assert!(stdout.contains("mantle: delivered Branch(True) to Worker"));
    assert!(stdout.contains("guarded loop selected true"));
    assert!(stdout.contains("worker handled true"));
    assert!(!stdout.contains("mantle: delivered Branch(False) to Worker"));
    assert!(!stdout.contains("worker handled false"));

    let artifact = gate.read_artifact("target/strata/runtime_guarded_for_each.mta");
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
            ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::Equality { .. },
                then_actions,
                else_actions,
            },
        ] if matches!(
            then_actions.as_slice(),
            [ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::RecordField { .. },
                max_items: 2,
                body,
            }] if element.ty == bool_type
                && matches!(
                    body.as_slice(),
                    [ArtifactAction::IfElse {
                        condition: ArtifactValueTemplate::Equality { .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
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
                    ) && else_actions.is_empty()
                )
        ) && else_actions.is_empty()
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "guarded loop artifact must not dispatch through the source loop binding name"
    );

    let trace = gate.read_trace("runtime_guarded_for_each");
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""branch_path":[1]"#,
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
            r#""branch_path":[1]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"BatchWorker""#,
            r#""element_id":0"#,
            r#""max_items":2"#,
            r#""item_count":2"#,
        ],
    );
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
            r#""event":"loop_iteration""#,
            r#""process":"BatchWorker""#,
            r#""index":1"#,
            r#""element":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""branch_path":[1,4096,12288]"#,
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
            r#""branch_path":[1,4096,12288]"#,
            r#""loop_element_id":0"#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""process":"BatchWorker""#,
            r#""iteration_count":2"#,
        ],
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.contains(r#""event":"loop_started""#))
            .count(),
        1,
        "disabled branch must not emit loop_started"
    );
    assert!(
        !trace.contains(
            r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#
        ),
        "selected loop-body no-op branch must not send the false item"
    );

    let outer_true = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action","branch_path":[1]"#,
    );
    let loop_start = trace_line_index(
        &trace,
        r#""event":"loop_started","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let inner_then = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action","branch_path":[1,4096,12288]"#,
    );
    let batch_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"BatchWorker","stream":"stdout","output_id":0,"text":"guarded loop selected true""#,
    );
    let true_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"True""#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let inner_else = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action","branch_path":[1,4096,12288]"#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let outer_false = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action","branch_path":[1]"#,
    );

    assert!(outer_true < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < inner_then);
    assert!(inner_then < batch_output);
    assert!(batch_output < true_send);
    assert!(true_send < second_iteration);
    assert!(second_iteration < inner_else);
    assert!(inner_else < loop_complete);
    assert!(loop_complete < outer_false);
}
