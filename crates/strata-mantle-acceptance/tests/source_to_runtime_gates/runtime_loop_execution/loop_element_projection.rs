use mantle_artifact::{ArtifactEnumVariant, ArtifactTransition, ArtifactType, ArtifactValueShape};

use super::super::support::*;

#[test]
fn runtime_loop_element_projection_routes_ready_phase_through_received_ref() {
    let gate = GateHarness::new();
    gate.remove_trace("runtime_loop_element_projection");
    let run = gate.check_build_run(
        "examples/runtime_loop_element_projection.str",
        "target/strata/runtime_loop_element_projection.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout
            .matches("mantle: delivered AssignPhase(Ready) to Worker")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("mantle: delivered AssignPhase(Done) to Worker")
            .count(),
        0
    );
    assert_eq!(stdout.matches("worker handled routed phase").count(), 1);

    let artifact = gate.read_artifact("target/strata/runtime_loop_element_projection.mta");
    let batch_request_type = value_type_id(&artifact, "BatchRequest");
    let bool_type = value_type_id(&artifact, "Bool");
    let phase_type = value_type_id(&artifact, "Phase");
    let job_type = value_type_id(&artifact, "Job");
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let worker_ref_type = process_ref_type_id(&artifact, worker_process_id);
    let batch_worker_index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact process should exist");
    let batch_worker_prefix = format!("process.{batch_worker_index}.");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    assert!(
        batch_worker.process_refs.is_empty(),
        "BatchWorker must use the received process ref without local authority"
    );
    assert!(
        batch_worker
            .transitions
            .iter()
            .filter(|transition| transition.message == MessageId::new(1))
            .any(|transition| {
                is_loop_element_projection_route(
                    &artifact,
                    transition,
                    ExpectedProjectionRoute {
                        batch_request_type,
                        bool_type,
                        phase_type,
                        job_type,
                        worker_ref_type,
                        worker_process_id,
                    },
                )
            }),
        "Route transition should project Phase from a typed loop element and send it through the received ProcessRef"
    );
    let encoded = artifact.encode();
    assert!(
        !encoded.lines().any(|line| {
            line.starts_with(&batch_worker_prefix)
                && (line.contains("debug_name=routed_phase")
                    || line.ends_with("=routed_phase")
                    || line.contains("target_process_ref"))
        }),
        "artifact must not dispatch through source binding aliases or local process refs"
    );

    let trace = gate.read_trace("runtime_loop_element_projection");
    let worker_ref_payload = format!(r#""payload_type_id":{}"#, worker_ref_type.as_u32());
    let worker_ref_process = format!(r#""payload_process_id":{}"#, worker_process_id.as_u32());
    let job_element_type = format!(r#""element_type_id":{}"#, job_type.as_u32());
    let phase_payload_type = format!(r#""payload_type_id":{}"#, phase_type.as_u32());
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":2"#,
            r#""message":"Route""#,
            worker_ref_payload.as_str(),
            worker_ref_process.as_str(),
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
            r#""condition":"False""#,
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
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""branch":"then""#,
            r#""loop_index":0"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_accepted""#,
            r#""pid":4"#,
            r#""process":"Worker""#,
            r#""message":"AssignPhase""#,
            phase_payload_type.as_str(),
            r#""payload":"Ready""#,
            r#""sender_pid":2"#,
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
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""branch":"else""#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"loop_completed""#,
            r#""pid":2"#,
            r#""iteration_count":2"#,
        ],
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""event":"message_accepted""#)
                && line.contains(r#""process":"Worker""#)
                && line.contains(r#""payload":"Done""#)
        }),
        "Done phase must not be sent"
    );
    assert!(
        !trace.lines().any(|line| {
            line.contains(r#""pid":3"#)
                && (line.contains(r#""event":"loop_started""#)
                    || line.contains(r#""event":"loop_iteration""#)
                    || line.contains(r#""event":"loop_completed""#))
        }),
        "disabled batch must not emit loop events"
    );
}

#[test]
fn runtime_loop_element_projection_rejects_wrong_projected_field_type_before_runtime() {
    let mut artifact =
        projection_seed_artifact("runtime_loop_element_projection_bad_field_type_seed.mta");
    let bool_type = value_type_id(&artifact, "Bool");
    let phase_type = value_type_id(&artifact, "Phase");
    let (worker_message_type, worker_message_variants) = {
        let worker = artifact_process_mut(&mut artifact, "Worker");
        worker.message_variants[0].payload_type = Some(bool_type);
        let variants = worker
            .message_variants
            .iter()
            .map(|variant| ArtifactEnumVariant {
                label: variant.label.clone(),
                payload_type: variant.payload_type,
            })
            .collect();
        (worker.message_type, variants)
    };
    let worker_message_label = artifact.types[worker_message_type.index()].label.clone();
    artifact.types[worker_message_type.index()] =
        ArtifactType::enum_value_with_payloads(worker_message_label, worker_message_variants);
    if let ArtifactValueTemplate::RecordField { ty, .. } = phase_send_projection_mut(&mut artifact)
    {
        *ty = bool_type;
    } else {
        panic!("send payload should be the phase record-field projection");
    }

    let gate = GateHarness::new();
    let invalid_artifact_path = "target/strata/runtime_loop_element_projection_bad_field_type.mta";
    let invalid_trace_stem = "runtime_loop_element_projection_bad_field_type";
    gate.remove_trace(invalid_trace_stem);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());
    let run = gate.run_mantle_failure(invalid_artifact_path);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(&format!(
            "expected record field type id {}",
            phase_type.as_u32()
        )),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_loop_element_projection_rejects_unknown_projected_field_before_runtime() {
    let mut artifact =
        projection_seed_artifact("runtime_loop_element_projection_bad_field_name_seed.mta");
    if let ArtifactValueTemplate::RecordField { field, .. } =
        phase_send_projection_mut(&mut artifact)
    {
        *field = RecordFieldId::new(1);
    } else {
        panic!("send payload should be the phase record-field projection");
    }

    let gate = GateHarness::new();
    let invalid_artifact_path = "target/strata/runtime_loop_element_projection_bad_field_name.mta";
    let invalid_trace_stem = "runtime_loop_element_projection_bad_field_name";
    gate.remove_trace(invalid_trace_stem);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());
    let run = gate.run_mantle_failure(invalid_artifact_path);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("field_id 1 is not declared"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

#[test]
fn runtime_loop_element_projection_rejects_non_record_projection_source_before_runtime() {
    let mut artifact =
        projection_seed_artifact("runtime_loop_element_projection_non_record_seed.mta");
    let phase_type = value_type_id(&artifact, "Phase");
    if let ArtifactValueTemplate::RecordField { record, .. } =
        phase_send_projection_mut(&mut artifact)
    {
        **record = ArtifactValueTemplate::Literal {
            ty: phase_type,
            value: artifact_value("Ready"),
        };
    } else {
        panic!("send payload should be the phase record-field projection");
    }

    let gate = GateHarness::new();
    let invalid_artifact_path = "target/strata/runtime_loop_element_projection_non_record.mta";
    let invalid_trace_stem = "runtime_loop_element_projection_non_record";
    gate.remove_trace(invalid_trace_stem);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());
    let run = gate.run_mantle_failure(invalid_artifact_path);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("send payload.record type id") && stderr.contains("must be a record type"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

struct ExpectedProjectionRoute {
    batch_request_type: TypeId,
    bool_type: TypeId,
    phase_type: TypeId,
    job_type: TypeId,
    worker_ref_type: TypeId,
    worker_process_id: ProcessId,
}

fn is_loop_element_projection_route(
    artifact: &MantleArtifact,
    transition: &ArtifactTransition,
    expected: ExpectedProjectionRoute,
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
                    && field.as_u32() == 0
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::CurrentStatePayload { ty }
                            if *ty == expected.batch_request_type
                    )
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == expected.bool_type && value == &artifact_value("True")
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
                    && field.as_u32() == 1
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::CurrentStatePayload { ty }
                            if *ty == expected.batch_request_type
                    )
                    && matches!(
                        body.as_slice(),
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
                            && *operand_ty == expected.phase_type
                            && matches!(
                                left.as_ref(),
                                ArtifactValueTemplate::RecordField {
                                    ty,
                                    record,
                                    field,
                                } if *ty == expected.phase_type
                                    && field.as_u32() == 0
                                    && matches!(
                                        record.as_ref(),
                                        ArtifactValueTemplate::LoopElement {
                                            ty,
                                            element: condition_element,
                                        } if *ty == expected.job_type
                                            && *condition_element == element.id
                                    )
                            )
                            && matches!(
                                right.as_ref(),
                                ArtifactValueTemplate::Literal { ty, value }
                                    if *ty == expected.phase_type && value == &artifact_value("Ready")
                            )
                            && matches!(
                                then_actions.as_slice(),
                                [ArtifactAction::Send {
                                    target:
                                        ArtifactSendTarget::ReceivedPayload {
                                            ty,
                                            target_process,
                                        },
                                    message,
                                    payload:
                                        Some(ArtifactValueTemplate::RecordField {
                                            ty: payload_ty,
                                            record,
                                            field,
                                        }),
                                    ..
                                }] if *ty == expected.worker_ref_type
                                    && *target_process == expected.worker_process_id
                                    && *message == MessageId::new(0)
                                    && *payload_ty == expected.phase_type
                                    && field.as_u32() == 0
                                    && matches!(
                                        record.as_ref(),
                                        ArtifactValueTemplate::LoopElement {
                                            ty,
                                            element: payload_element,
                                        } if *ty == expected.job_type
                                            && *payload_element == element.id
                                    )
                            )
                            && else_actions.is_empty()
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

fn projection_seed_artifact(seed_name: &str) -> MantleArtifact {
    let gate = GateHarness::new();
    let seed_artifact_path = format!("target/strata/{seed_name}");
    gate.check("examples/runtime_loop_element_projection.str");
    gate.build(
        "examples/runtime_loop_element_projection.str",
        &seed_artifact_path,
    );
    gate.read_artifact(&seed_artifact_path)
}

fn artifact_process_mut<'a>(
    artifact: &'a mut MantleArtifact,
    process: &str,
) -> &'a mut mantle_artifact::ArtifactProcess {
    artifact
        .processes
        .iter_mut()
        .find(|candidate| candidate.debug_name == process)
        .unwrap_or_else(|| panic!("artifact process {process} should exist"))
}

fn phase_send_projection_mut(artifact: &mut MantleArtifact) -> &mut ArtifactValueTemplate {
    let batch_worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact process should exist");
    let route_transition = batch_worker
        .transitions
        .iter_mut()
        .find(|transition| transition.message == MessageId::new(1))
        .expect("Route transition should exist");
    let [ArtifactAction::IfElse { then_actions, .. }] = route_transition.actions.as_mut_slice()
    else {
        panic!("Route transition should contain the enabled guard");
    };
    let [ArtifactAction::ForEach { body, .. }] = then_actions.as_mut_slice() else {
        panic!("enabled route branch should contain the loop");
    };
    let [ArtifactAction::IfElse { then_actions, .. }] = body.as_mut_slice() else {
        panic!("loop body should contain the phase guard");
    };
    let [
        ArtifactAction::Send {
            payload: Some(payload),
            ..
        },
    ] = then_actions.as_mut_slice()
    else {
        panic!("phase guard then branch should send a payload");
    };
    payload
}
