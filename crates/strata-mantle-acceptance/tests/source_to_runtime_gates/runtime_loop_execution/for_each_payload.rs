use super::*;

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
