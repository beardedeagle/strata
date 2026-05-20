use super::*;

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
