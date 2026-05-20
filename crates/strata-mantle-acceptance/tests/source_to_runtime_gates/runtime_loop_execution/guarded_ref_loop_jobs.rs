use mantle_artifact::{ArtifactTransition, ArtifactValue, ArtifactValueShape};

use super::super::support::*;

#[test]
fn runtime_guarded_ref_loop_jobs_routes_payload_values_through_received_ref() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_guarded_ref_loop_jobs");
    let run = gate.check_build_run(
        "examples/runtime_guarded_ref_loop_jobs.str",
        "target/strata/runtime_guarded_ref_loop_jobs.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout
            .matches("mantle: delivered Assign(Job{phase:Ready}) to Worker")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("mantle: delivered Assign(Job{phase:Done}) to Worker")
            .count(),
        1
    );
    assert_eq!(stdout.matches("worker handled routed job").count(), 2);

    let artifact = gate.read_artifact("target/strata/runtime_guarded_ref_loop_jobs.mta");
    let batch_request_type = value_type_id(&artifact, "BatchRequest");
    let bool_type = value_type_id(&artifact, "Bool");
    let job_type = value_type_id(&artifact, "Job");
    let true_value = artifact_value("True");
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let worker_ref_type = process_ref_type_id(&artifact, worker_process_id);
    let expected_route = ExpectedRouteShape {
        batch_request_type,
        bool_type,
        job_type,
        worker_ref_type,
        worker_process_id,
        true_value: &true_value,
    };
    let (batch_worker_index, batch_worker) = artifact
        .processes
        .iter()
        .enumerate()
        .find(|(_, process)| process.debug_name == "BatchWorker")
        .expect("artifact process BatchWorker should exist");
    assert!(
        batch_worker.process_refs.is_empty(),
        "BatchWorker must route through the received process ref without local authority"
    );
    let mut route_transition_count = 0;
    for transition in batch_worker
        .transitions
        .iter()
        .filter(|transition| transition.message == MessageId::new(1))
    {
        route_transition_count += 1;
        assert!(
            is_received_ref_job_route_transition(&artifact, transition, &expected_route),
            "Route transition should guard a typed Job loop and send each element through a received process-ref target"
        );
    }
    assert!(
        route_transition_count > 0,
        "BatchWorker should have Route transitions"
    );
    let encoded = artifact.encode();
    let batch_worker_prefix = format!("process.{batch_worker_index}.");
    assert!(
        !encoded.lines().any(|line| {
            line.starts_with(&batch_worker_prefix)
                && (line.contains("target_process_ref")
                    || line.contains("debug_name=worker")
                    || line.contains("debug_name=job")
                    || line.ends_with("=worker")
                    || line.ends_with("=job"))
        }),
        "BatchWorker artifact must not dispatch through source names or local process-ref ids"
    );

    let trace = gate.read_trace("runtime_guarded_ref_loop_jobs");
    let route_payload_type = format!(r#""payload_type_id":{}"#, worker_ref_type.as_u32());
    let route_payload_process = format!(r#""payload_process_id":{}"#, worker_process_id.as_u32());
    let job_element_type = format!(r#""element_type_id":{}"#, job_type.as_u32());
    let job_payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":2"#,
            r#""message":"Route""#,
            route_payload_type.as_str(),
            route_payload_process.as_str(),
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
            r#""scope":"action""#,
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
            r#""scope":"action""#,
            r#""branch_path":[0]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""pid":2"#,
            r#""process":"BatchWorker""#,
            r#""message":"Route""#,
            r#""element_id":0"#,
            r#""max_items":2"#,
            r#""item_count":2"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""pid":2"#,
            r#""index":0"#,
            job_element_type.as_str(),
            r#""element":"Job{phase:Ready}""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""pid":2"#,
            r#""index":1"#,
            job_element_type.as_str(),
            r#""element":"Job{phase:Done}""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""pid":2"#,
            r#""process":"BatchWorker""#,
            r#""message":"Route""#,
            r#""element_id":0"#,
            r#""iteration_count":2"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Ready}""#,
            r#""sender_pid":2"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Done}""#,
            r#""sender_pid":2"#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Ready}""#,
            r#""result":"Continue""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Done}""#,
            r#""result":"Continue""#,
        ],
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""event":"message_accepted""#)
                && line.contains(r#""process":"Worker""#)
                && line.contains(r#""message":"Assign""#)
                && line.contains(r#""sender_pid":3"#)
        }),
        "disabled batch must not send Job payloads to Worker"
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""pid":3"#)
                && (line.contains(r#""event":"loop_started""#)
                    || line.contains(r#""event":"loop_iteration""#)
                    || line.contains(r#""event":"loop_completed""#))
        }),
        "disabled branch must not emit loop events"
    );

    let batch_worker_process_id =
        ProcessId::from_index(batch_worker_index).expect("artifact process index should fit");
    let enabled_route_accepted = trace_line_index(
        &trace,
        &format!(
            r#""event":"message_accepted","pid":2,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","payload_type_id":{}"#,
            batch_worker_process_id.as_u32(),
            worker_ref_type.as_u32()
        ),
    );
    let enabled_branch = trace_line_index(
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
    let first_iteration = trace_line_index(
        &trace,
        &format!(
            r#""event":"loop_iteration","pid":2,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","element_id":0,"index":0,"element_type_id":{},"element":"Job{{phase:Ready}}""#,
            batch_worker_process_id.as_u32(),
            job_type.as_u32()
        ),
    );
    let first_send = trace_line_index(
        &trace,
        &format!(
            r#""event":"message_accepted","pid":4,"process_id":{},"process":"Worker","message_id":0,"message":"Assign","payload_type_id":{},"payload":"Job{{phase:Ready}}""#,
            worker_process_id.as_u32(),
            job_type.as_u32()
        ),
    );
    let second_iteration = trace_line_index(
        &trace,
        &format!(
            r#""event":"loop_iteration","pid":2,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","element_id":0,"index":1,"element_type_id":{},"element":"Job{{phase:Done}}""#,
            batch_worker_process_id.as_u32(),
            job_type.as_u32()
        ),
    );
    let second_send = trace_line_index(
        &trace,
        &format!(
            r#""event":"message_accepted","pid":4,"process_id":{},"process":"Worker","message_id":0,"message":"Assign","payload_type_id":{},"payload":"Job{{phase:Done}}""#,
            worker_process_id.as_u32(),
            job_type.as_u32()
        ),
    );
    let loop_complete = trace_line_index(
        &trace,
        &format!(
            r#""event":"loop_completed","pid":2,"process_id":{},"process":"BatchWorker""#,
            batch_worker_process_id.as_u32()
        ),
    );
    let disabled_route_accepted = trace_line_index(
        &trace,
        &format!(
            r#""event":"message_accepted","pid":3,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","payload_type_id":{}"#,
            batch_worker_process_id.as_u32(),
            worker_ref_type.as_u32()
        ),
    );
    let disabled_branch = trace_line_index(
        &trace,
        &format!(
            r#""event":"branch_selected","pid":3,"process_id":{},"process":"BatchWorker","message_id":1,"message":"Route","branch":"else""#,
            batch_worker_process_id.as_u32()
        ),
    );
    assert!(enabled_route_accepted < enabled_branch);
    assert!(enabled_branch < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < first_send);
    assert!(first_send < second_iteration);
    assert!(second_iteration < second_send);
    assert!(second_send < loop_complete);
    assert!(disabled_route_accepted < disabled_branch);
    assert!(loop_complete < disabled_branch);
}

#[test]
fn runtime_guarded_ref_loop_jobs_rejects_loop_payload_type_mismatch_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_guarded_ref_loop_jobs_bad_payload_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_guarded_ref_loop_jobs_bad_payload.mta";
    let invalid_trace_stem = "runtime_guarded_ref_loop_jobs_bad_payload";

    gate.check("examples/runtime_guarded_ref_loop_jobs.str");
    gate.build(
        "examples/runtime_guarded_ref_loop_jobs.str",
        seed_artifact_path,
    );
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let job_type = value_type_id(&artifact, "Job");
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("artifact process BatchWorker should exist");
    let route_transition = batch_worker
        .transitions
        .iter_mut()
        .find(|transition| transition.message == MessageId::new(1))
        .expect("BatchWorker Route transition should exist");
    let [ArtifactAction::IfElse { then_actions, .. }] = route_transition.actions.as_mut_slice()
    else {
        panic!("Route transition should contain only the guarded branch");
    };
    let [ArtifactAction::ForEach { body, .. }] = then_actions.as_mut_slice() else {
        panic!("Route enabled branch should contain only the bounded loop");
    };
    let [
        ArtifactAction::Send {
            payload: Some(ArtifactValueTemplate::LoopElement { ty: payload_ty, .. }),
            ..
        },
    ] = body.as_mut_slice()
    else {
        panic!("Route loop body should send the loop element");
    };
    *payload_ty = bool_type;
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    let expected = format!(
        "send payload loop element id 0 has type id {}, expected {}",
        job_type.as_u32(),
        bool_type.as_u32()
    );
    assert!(
        stderr.contains(&expected),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

struct ExpectedRouteShape<'a> {
    batch_request_type: TypeId,
    bool_type: TypeId,
    job_type: TypeId,
    worker_ref_type: TypeId,
    worker_process_id: ProcessId,
    true_value: &'a ArtifactValue,
}

fn is_received_ref_job_route_transition(
    artifact: &MantleArtifact,
    transition: &ArtifactTransition,
    expected: &ExpectedRouteShape<'_>,
) -> bool {
    matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition:
                ArtifactValueTemplate::Equality {
                    ty,
                    operand_ty,
                    operator: ArtifactValueEqualityOperator::Equal,
                    left,
                    right,
                },
            then_actions,
            else_actions,
        }] if *ty == expected.bool_type
            && *operand_ty == expected.bool_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::RecordField {
                    ty,
                    record,
                    field,
                } if *ty == expected.bool_type
                    && field == "enabled"
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::CurrentStatePayload { ty }
                            if *ty == expected.batch_request_type
                    )
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == expected.bool_type && value == expected.true_value
            )
            && matches!(
                then_actions.as_slice(),
                [ArtifactAction::ForEach {
                    element,
                    collection:
                        ArtifactValueTemplate::RecordField {
                            ty,
                            record,
                            field,
                        },
                    max_items: 2,
                    body,
                }] if element.ty == expected.job_type
                    && is_list_type(artifact, *ty, expected.job_type, 2)
                    && field == "jobs"
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::CurrentStatePayload { ty }
                            if *ty == expected.batch_request_type
                    )
                    && matches!(
                        body.as_slice(),
                        [ArtifactAction::Send {
                            target:
                                ArtifactSendTarget::ReceivedPayload {
                                    ty,
                                    target_process,
                                },
                            message,
                            payload:
                                Some(ArtifactValueTemplate::LoopElement {
                                    ty: payload_ty,
                                    element: payload_element,
                                }),
                        }] if *ty == expected.worker_ref_type
                            && *target_process == expected.worker_process_id
                            && *message == MessageId::new(0)
                            && *payload_ty == expected.job_type
                            && *payload_element == element.id
                    )
            )
            && else_actions.is_empty()
    )
}

fn is_list_type(
    artifact: &MantleArtifact,
    ty: TypeId,
    expected_element: TypeId,
    expected_capacity: usize,
) -> bool {
    matches!(
        artifact.types.get(ty.index()).and_then(|ty| ty.shape.as_ref()),
        Some(ArtifactValueShape::List { element, capacity })
            if *element == expected_element && *capacity == expected_capacity
    )
}
