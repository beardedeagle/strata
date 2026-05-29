use super::super::support::*;

const STEM: &str = "runtime_scalar_priority";
const SOURCE: &str = "examples/runtime_scalar_priority.str";
const ARTIFACT: &str = "target/strata/runtime_scalar_priority.mta";

#[test]
fn runtime_scalar_priority_branches_on_typed_scalar_template_at_mantle_runtime() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("scalar priority high"));

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let priority_type = value_type_id(&artifact, "Priority");
    let u32_type = value_type_id(&artifact, "U32");
    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=if_else"));
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have an Assign transition");
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition: ArtifactValueTemplate::ScalarOrdering {
                ty,
                operand_ty,
                left,
                right,
                ..
            },
            then_state,
            ..
        } if *ty == bool_type
            && *operand_ty == u32_type
            && matches!(left.as_ref(), ArtifactValueTemplate::ScalarArithmetic { ty, .. } if *ty == u32_type)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value } if *ty == u32_type && value == &artifact_value("10_u32")
            )
            && matches!(
                then_state.as_ref(),
                NextState::Template(ArtifactValueTemplate::Record { fields, .. })
                    if fields.iter().any(|field| field.field.as_u32() == 1
                        && matches!(&field.value, ArtifactValueTemplate::ScalarArithmetic { ty, .. } if *ty == u32_type))
                    && fields.iter().any(|field| field.field.as_u32() == 0
                        && matches!(
                            &field.value,
                            ArtifactValueTemplate::IfElse {
                                ty,
                                condition,
                                then_value,
                                else_value,
                            } if *ty == priority_type
                                && matches!(condition.as_ref(), ArtifactValueTemplate::ScalarOrdering { ty, operand_ty, .. } if *ty == bool_type && *operand_ty == u32_type)
                                && matches!(then_value.as_ref(), ArtifactValueTemplate::Literal { ty, value } if *ty == priority_type && value == &artifact_value("High"))
                                && matches!(else_value.as_ref(), ArtifactValueTemplate::Literal { ty, value } if *ty == priority_type && value == &artifact_value("Normal"))
                        ))
            )
    ));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::ScalarOrdering { ty, operand_ty, .. },
            then_actions,
            else_actions,
        }] if *ty == bool_type
            && *operand_ty == u32_type
            && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));
    assert!(
        !encoded.contains("high_priority")
            && !encoded.contains("compute_level")
            && !encoded.contains("classify")
            && !encoded.contains("base")
            && !encoded.contains("adjusted")
            && !encoded.contains("urgent"),
        "scalar source names and local binding names must not lower as runtime dispatch meaning"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""message":"Assign""#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
            r#""condition":"True""#,
        ],
    );
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"scalar priority high""#
    ));
    assert!(trace.contains(r#""state":"WorkerState{selected:High,level:11_u32}""#));
}
