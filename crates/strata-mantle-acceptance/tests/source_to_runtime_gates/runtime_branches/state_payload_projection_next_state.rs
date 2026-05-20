use super::super::support::*;

const STEM: &str = "runtime_state_payload_projection_next_state";
const SOURCE: &str = "examples/runtime_state_payload_projection_next_state.str";
const ARTIFACT: &str = "target/strata/runtime_state_payload_projection_next_state.mta";

struct ExpectedBranchIds {
    bool_type: TypeId,
    phase_type: TypeId,
    job_type: TypeId,
    ready_seen: StateId,
    done_seen: StateId,
}

#[test]
fn runtime_state_payload_projection_next_state_branches_on_current_record_field() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    gate.check_build_run(SOURCE, ARTIFACT);

    let artifact = gate.read_artifact(ARTIFACT);
    let worker_process_id = artifact_process_id(&artifact, "Worker");
    let worker = artifact_process(&artifact, "Worker");
    let decide_message_id = message_id(worker, "Decide");
    let ready_holding = state_id(worker, "Holding(Job{phase:Ready})");
    let done_holding = state_id(worker, "Holding(Job{phase:Done})");
    let expected = ExpectedBranchIds {
        bool_type: value_type_id(&artifact, "Bool"),
        phase_type: value_type_id(&artifact, "Phase"),
        job_type: value_type_id(&artifact, "Job"),
        ready_seen: state_id(worker, "ReadySeen"),
        done_seen: state_id(worker, "DoneSeen"),
    };

    assert_current_payload_phase_branch(
        transition_for_current_state(worker, decide_message_id, ready_holding),
        &expected,
    );
    assert_current_payload_phase_branch(
        transition_for_current_state(worker, decide_message_id, done_holding),
        &expected,
    );

    let encoded = artifact.encode();
    assert!(encoded.contains("field_name=phase"));
    assert!(
        !encoded.contains("held_phase"),
        "artifact must not dispatch through the source state-payload alias"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            r#""process":"Worker""#,
            r#""message":"Decide""#,
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
            r#""message":"Decide""#,
            r#""branch":"else""#,
            r#""scope":"next_state""#,
            r#""condition":"False""#,
        ],
    );

    let worker_process = format!(r#""process_id":{}"#, worker_process_id.as_u32());
    let decide_message = format!(r#""message_id":{}"#, decide_message_id.as_u32());
    let ready_seen_state = format!(r#""state_id":{}"#, expected.ready_seen.as_u32());
    let done_seen_state = format!(r#""state_id":{}"#, expected.done_seen.as_u32());
    let ready_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""pid":2"#,
            worker_process.as_str(),
            r#""process":"Worker""#,
            decide_message.as_str(),
            r#""message":"Decide""#,
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
            decide_message.as_str(),
            r#""message":"Decide""#,
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
            decide_message.as_str(),
            r#""message":"Decide""#,
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
            decide_message.as_str(),
            r#""message":"Decide""#,
            r#""result":"Continue""#,
            done_seen_state.as_str(),
            r#""state":"DoneSeen""#,
        ],
    );
    assert!(ready_branch < ready_step);
    assert!(done_branch < done_step);
}

#[test]
fn runtime_state_payload_projection_next_state_rejects_unknown_projected_field_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path =
        "target/strata/runtime_state_payload_projection_next_state_bad_field_seed.mta";
    let invalid_artifact_path =
        "target/strata/runtime_state_payload_projection_next_state_bad_field.mta";
    let invalid_trace_stem = "runtime_state_payload_projection_next_state_bad_field";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let ArtifactValueTemplate::RecordField { field, .. } =
        first_decide_next_state_condition_left_mut(&mut artifact)
    else {
        panic!("next-state condition should project the current state payload field");
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

fn assert_current_payload_phase_branch(
    transition: &ArtifactTransition,
    expected: &ExpectedBranchIds,
) {
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
        } if *ty == expected.bool_type
            && *operand_ty == expected.phase_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::RecordField {
                    ty,
                    record,
                    field,
                } if *ty == expected.phase_type
                    && field == "phase"
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::CurrentStatePayload { ty }
                            if *ty == expected.job_type
                    )
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == expected.phase_type && value == &artifact_value("Ready")
            )
            && matches!(
                then_state.as_ref(),
                NextState::Value(state) if *state == expected.ready_seen
            )
            && matches!(
                else_state.as_ref(),
                NextState::Value(state) if *state == expected.done_seen
            )
    ));
}

fn transition_for_current_state(
    worker: &ArtifactProcess,
    message: MessageId,
    current_state: StateId,
) -> &ArtifactTransition {
    worker
        .transitions
        .iter()
        .find(|transition| {
            transition.message == message && transition.current_state == Some(current_state)
        })
        .unwrap_or_else(|| {
            panic!(
                "Worker should have transition for message {} and state {}",
                message.as_u32(),
                current_state.as_u32()
            )
        })
}

fn first_decide_next_state_condition_left_mut(
    artifact: &mut MantleArtifact,
) -> &mut ArtifactValueTemplate {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let decide_message = MessageId::from_index(
        worker
            .message_variants
            .iter()
            .position(|message| message.label == "Decide")
            .expect("Decide message should exist"),
    )
    .expect("Decide message index should fit");
    let transition = worker
        .transitions
        .iter_mut()
        .find(|transition| {
            transition.message == decide_message
                && matches!(&transition.next_state, NextState::IfElse { .. })
        })
        .expect("Worker Decide transition should contain next-state branch control");
    let NextState::IfElse { condition, .. } = &mut transition.next_state else {
        panic!("Worker Decide transition should contain next-state branch control");
    };
    let ArtifactValueTemplate::Equality { left, .. } = condition else {
        panic!("next-state branch condition should be equality");
    };
    left
}
