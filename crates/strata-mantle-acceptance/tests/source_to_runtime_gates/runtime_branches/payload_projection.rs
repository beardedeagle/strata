use super::super::support::*;

const STEM: &str = "runtime_payload_projection_if";
const SOURCE: &str = "examples/runtime_payload_projection_if.str";
const ARTIFACT: &str = "target/strata/runtime_payload_projection_if.mta";

#[test]
fn runtime_payload_projection_if_branches_on_received_record_field() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(stdout.matches("worker accepted ready payload").count(), 1);

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let phase_type = value_type_id(&artifact, "Phase");
    let job_type = value_type_id(&artifact, "Job");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have an Assign transition");

    assert!(matches!(transition.next_state, NextState::Current));
    assert!(matches!(
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
        }] if *ty == bool_type
            && *operand_ty == phase_type
            && matches!(
                left.as_ref(),
                ArtifactValueTemplate::RecordField {
                    ty,
                    record,
                    field,
                } if *ty == phase_type
                    && field.as_u32() == 0
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
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && else_actions.is_empty()
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains("field_id=0"));
    assert!(!encoded.contains("field_name="));
    assert!(
        !encoded.contains("assigned_phase"),
        "artifact must not dispatch through the source payload alias"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
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
            r#""message":"Assign""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""condition":"False""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker accepted ready payload""#
    ));
    assert_eq!(trace.matches(r#""event":"program_output""#).count(), 1);
}

#[test]
fn runtime_payload_projection_if_rejects_unknown_projected_field_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_payload_projection_if_bad_field_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_payload_projection_if_bad_field.mta";
    let invalid_trace_stem = "runtime_payload_projection_if_bad_field";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    let ArtifactValueTemplate::RecordField { field, .. } =
        payload_projection_condition_left_mut(&mut artifact)
    else {
        panic!("runtime branch condition should project the received payload field");
    };
    *field = RecordFieldId::new(1);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("field_id 1 is not declared by type id"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn payload_projection_condition_left_mut(
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
    let [ArtifactAction::IfElse { condition, .. }] = transition.actions.as_mut_slice() else {
        panic!("Worker Assign transition should contain one runtime branch action");
    };
    let ArtifactValueTemplate::Equality { left, .. } = condition else {
        panic!("runtime branch condition should be equality");
    };
    left
}
