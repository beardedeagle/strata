use super::super::support::*;

#[test]
fn runtime_guarded_ref_loop_routes_received_ref_inside_guarded_loop() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_guarded_ref_loop");
    let run = gate.check_build_run(
        "examples/runtime_guarded_ref_loop.str",
        "target/strata/runtime_guarded_ref_loop.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    let artifact = gate.read_artifact("target/strata/runtime_guarded_ref_loop.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let worker_ref_type = process_ref_type_id(&artifact, worker_process_id);
    let expected_route = format!(
        "mantle: delivered Route(type{}#4) to BatchWorker",
        worker_ref_type.as_u32()
    );
    assert!(stdout.contains(&expected_route));
    assert!(stdout.contains("mantle: delivered Branch(True) to Worker"));
    assert!(stdout.contains("received ref guarded loop selected true"));
    assert!(stdout.contains("worker handled routed true"));
    assert!(!stdout.contains("mantle: delivered Branch(False) to Worker"));
    assert!(!stdout.contains("worker handled routed false"));

    let (batch_worker_index, batch_worker) = artifact
        .processes
        .iter()
        .enumerate()
        .find(|(_, process)| process.debug_name == "BatchWorker")
        .expect("artifact process BatchWorker should exist");
    assert!(
        batch_worker.process_refs.is_empty(),
        "BatchWorker must not acquire local process refs for received-ref routing"
    );
    assert!(
        batch_worker.transitions.iter().any(|transition| {
            transition.message == MessageId::new(1)
                && matches!(
                    transition.actions.as_slice(),
                    [ArtifactAction::IfElse {
                        condition: ArtifactValueTemplate::Equality { .. },
                        then_actions,
                        else_actions,
                    }] if matches!(
                        then_actions.as_slice(),
                        [ArtifactAction::ForEach {
                            collection: ArtifactValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            },
                            max_items: 2,
                            body,
                            ..
                        }] if field == "items"
                            && matches!(
                                record.as_ref(),
                                ArtifactValueTemplate::CurrentStatePayload { .. }
                            )
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
                                            target: ArtifactSendTarget::ReceivedPayload {
                                                ty,
                                                target_process,
                                            },
                                            payload: Some(ArtifactValueTemplate::LoopElement { .. }),
                                            ..
                                        },
                                    ] if *ty == worker_ref_type
                                        && *target_process == worker_process_id
                                ) && else_actions.is_empty()
                            )
                    ) && else_actions.is_empty()
                )
        }),
        "BatchWorker Route transition should send through an admitted received process-ref target"
    );
    let encoded = artifact.encode();
    let batch_worker_prefix = format!("process.{batch_worker_index}.");
    assert!(
        !encoded.lines().any(|line| {
            line.starts_with(&batch_worker_prefix)
                && (line.contains("target_process_ref")
                    || line.contains("debug_name=worker")
                    || line.ends_with("=worker"))
        }),
        "BatchWorker artifact must not dispatch through source names or local process-ref ids"
    );

    let trace = gate.read_trace("runtime_guarded_ref_loop");
    let route_payload_type_field = format!(r#""payload_type_id":{}"#, worker_ref_type.as_u32());
    let worker_process_id_field = format!(r#""payload_process_id":{}"#, worker_process_id.as_u32());
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":2"#,
            r#""message":"Route""#,
            route_payload_type_field.as_str(),
            worker_process_id_field.as_str(),
            r#""payload_pid":4"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"BatchWorker""#,
            r#""message":"Route""#,
            r#""branch":"then""#,
            r#""branch_path":[0]"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"BatchWorker""#,
            r#""message":"Route""#,
            r#""branch":"else""#,
            r#""branch_path":[0]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"Branch""#,
            r#""payload":"True""#,
            r#""sender_pid":2"#,
        ],
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""event":"message_accepted""#)
                && line.contains(r#""pid":4"#)
                && line.contains(r#""process":"Worker""#)
                && line.contains(r#""message":"Branch""#)
                && line.contains(r#""payload":"False""#)
        }),
        "received-ref guarded loop must not send the false item"
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""pid":3"#)
                && (line.contains(r#""event":"loop_started""#)
                    || line.contains(r#""event":"loop_iteration""#)
                    || line.contains(r#""event":"loop_completed""#))
        }),
        "disabled received-ref branch must not emit loop events"
    );

    let batch_worker_process_id =
        ProcessId::from_index(batch_worker_index).expect("artifact process index should fit");
    let outer_true = trace_line_index(
        &trace,
        &format!(
            r#""event":"branch_selected","pid":2,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","branch":"then""#,
            batch_worker_process_id.as_u32()
        ),
    );
    let loop_start = trace_line_index(
        &trace,
        &format!(
            r#""event":"loop_started","pid":2,"process_id":{},"process":"BatchWorker""#,
            batch_worker_process_id.as_u32()
        ),
    );
    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let true_send = trace_line_index(
        &trace,
        &format!(
            r#""event":"message_accepted","pid":4,"process_id":{},"process":"Worker","message_id":0,"message":"Branch","payload_type_id":{},"payload":"True""#,
            worker_process_id.as_u32(),
            bool_type.as_u32()
        ),
    );
    let loop_complete = trace_line_index(
        &trace,
        &format!(
            r#""event":"loop_completed","pid":2,"process_id":{},"process":"BatchWorker""#,
            batch_worker_process_id.as_u32()
        ),
    );
    let outer_false = trace_line_index(
        &trace,
        &format!(
            r#""event":"branch_selected","pid":3,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","branch":"else""#,
            batch_worker_process_id.as_u32()
        ),
    );
    assert!(outer_true < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < true_send);
    assert!(true_send < loop_complete);
    assert!(loop_complete < outer_false);
}
