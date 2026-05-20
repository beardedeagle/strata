use super::super::support::*;

const STEM: &str = "runtime_payload_projection_next_state";
const SOURCE: &str = "examples/runtime_payload_projection_next_state.str";
const ARTIFACT: &str = "target/strata/runtime_payload_projection_next_state.mta";

#[test]
fn runtime_payload_projection_next_state_branches_on_received_record_field() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    gate.check_build_run(SOURCE, ARTIFACT);

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let phase_type = value_type_id(&artifact, "Phase");
    let job_type = value_type_id(&artifact, "Job");
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have an Assign transition");
    let assign_message_id = transition.message;
    let ready_seen = state_id(worker, "ReadySeen");
    let done_seen = state_id(worker, "DoneSeen");

    assert!(transition.actions.is_empty());
    assert!(transition.effects.is_empty());
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition:
                ArtifactValueTemplate::Equality {
                    ty,
                    operand_ty,
                    operator: ArtifactValueEqualityOperator::Equal,
                    left,
                    right,
                },
            then_state,
            else_state,
        } if *ty == bool_type
            && *operand_ty == phase_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::RecordField {
                    ty,
                    record,
                    field,
                } if *ty == phase_type
                    && field == "phase"
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::ReceivedPayload { ty } if *ty == job_type
                    )
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == phase_type && value == &artifact_value("Ready")
            )
            && matches!(
                then_state.as_ref(),
                NextState::Value(state) if *state == ready_seen
            )
            && matches!(
                else_state.as_ref(),
                NextState::Value(state) if *state == done_seen
            )
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains("field_name=phase"));
    assert!(
        !encoded.contains("assigned_phase"),
        "artifact must not dispatch through the source payload alias"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );
    let worker_process = format!(r#""process_id":{}"#, worker_process_id.as_u32());
    let assign_message = format!(r#""message_id":{}"#, assign_message_id.as_u32());
    let job_payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let ready_seen_state = format!(r#""state_id":{}"#, ready_seen.as_u32());
    let done_seen_state = format!(r#""state_id":{}"#, done_seen.as_u32());
    let ready_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            worker_process.as_str(),
            r#""process":"Worker""#,
            assign_message.as_str(),
            r#""message":"Assign""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
        ],
    );
    let ready_step = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""pid":2"#,
            worker_process.as_str(),
            r#""process":"Worker""#,
            assign_message.as_str(),
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Ready}""#,
            r#""result":"Continue""#,
            ready_seen_state.as_str(),
            r#""state":"ReadySeen""#,
        ],
    );
    let done_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":3"#,
            worker_process.as_str(),
            r#""process":"Worker""#,
            assign_message.as_str(),
            r#""message":"Assign""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
        ],
    );
    let done_step = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""pid":3"#,
            worker_process.as_str(),
            r#""process":"Worker""#,
            assign_message.as_str(),
            r#""message":"Assign""#,
            job_payload_type.as_str(),
            r#""payload":"Job{phase:Done}""#,
            r#""result":"Continue""#,
            done_seen_state.as_str(),
            r#""state":"DoneSeen""#,
        ],
    );
    assert!(ready_branch < ready_step);
    assert!(done_branch < done_step);
}

#[test]
fn runtime_payload_projection_next_state_rejects_unknown_projected_field_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path =
        "target/strata/runtime_payload_projection_next_state_bad_field_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_payload_projection_next_state_bad_field.mta";
    let invalid_trace_stem = "runtime_payload_projection_next_state_bad_field";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let ArtifactValueTemplate::RecordField { field, .. } =
        next_state_projection_condition_left_mut(&mut artifact)
    else {
        panic!("next-state condition should project the received payload field");
    };
    *field = "missing_phase".to_owned();
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("field_name missing_phase is not declared by type id"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn next_state_projection_condition_left_mut(
    artifact: &mut MantleArtifact,
) -> &mut ArtifactValueTemplate {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let transition = worker
        .transitions
        .first_mut()
        .expect("Worker should have an Assign transition");
    let NextState::IfElse { condition, .. } = &mut transition.next_state else {
        panic!("Worker Assign transition should contain next-state branch control");
    };
    let ArtifactValueTemplate::Equality { left, .. } = condition else {
        panic!("next-state branch condition should be equality");
    };
    left
}
