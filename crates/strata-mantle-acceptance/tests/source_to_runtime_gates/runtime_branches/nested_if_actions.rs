use super::super::support::*;
use mantle_artifact::OutputId;

const STEM: &str = "runtime_nested_if_actions";
const SOURCE: &str = "examples/runtime_nested_if_actions.str";
const ARTIFACT: &str = "target/strata/runtime_nested_if_actions.mta";

#[test]
fn runtime_nested_if_actions_branch_through_typed_action_paths() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    gate.check_build_run(SOURCE, ARTIFACT);

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let flags_type = value_type_id(&artifact, "CheckFlags");
    let worker = artifact_process(&artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");
    assert_nested_action_branch(transition, bool_type, flags_type);
    assert_no_executable_source_aliases(&artifact);

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0]"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0,4096]"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0]"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[0,4096]"#,
            r#""condition":"False""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected","pid":4,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[0]"#,
            r#""condition":"False""#,
        ],
    );

    let both_outer = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0]"#,
    );
    let both_inner = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0,4096]"#,
    );
    let both_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"worker saw both true""#,
    );
    assert!(both_outer < both_inner);
    assert!(both_inner < both_output);

    let outer_only_outer = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"then","scope":"action","branch_path":[0]"#,
    );
    let outer_only_inner = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":3,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[0,4096]"#,
    );
    let outer_only_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":1,"process":"Worker","stream":"stdout","output_id":1,"text":"worker saw outer true only""#,
    );
    assert!(outer_only_outer < outer_only_inner);
    assert!(outer_only_inner < outer_only_output);

    let outer_false_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":4,"process_id":1,"process":"Worker","message_id":0,"message":"Check","branch":"else","scope":"action","branch_path":[0]"#,
    );
    let outer_false_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":4,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"worker saw outer false""#,
    );
    assert!(outer_false_branch < outer_false_output);
}

#[test]
fn runtime_nested_if_actions_rejects_action_nesting_above_limit_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_nested_if_actions_too_deep_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_nested_if_actions_too_deep.mta";
    let invalid_trace_stem = "runtime_nested_if_actions_too_deep";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    add_over_limit_nested_action(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("runtime if action nesting exceeds maximum depth of 2"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn assert_nested_action_branch(
    transition: &ArtifactTransition,
    bool_type: TypeId,
    flags_type: TypeId,
) {
    assert_eq!(transition.effects, [ArtifactEffect::Emit]);
    assert!(matches!(transition.next_state, NextState::Current));
    assert!(matches!(
        transition.actions.as_slice(),
        [ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        }] if condition_is_bool_field_check(condition, bool_type, flags_type, 0)
            && matches!(
                then_actions.as_slice(),
                [ArtifactAction::IfElse {
                    condition,
                    then_actions,
                    else_actions,
                }] if condition_is_bool_field_check(condition, bool_type, flags_type, 1)
                    && matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
                    && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
            )
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }])
    ));
}

fn condition_is_bool_field_check(
    condition: &ArtifactValueTemplate,
    bool_type: TypeId,
    flags_type: TypeId,
    expected_field: u32,
) -> bool {
    matches!(
        condition,
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
                ArtifactValueTemplate::RecordField {
                    ty,
                    record,
                    field,
                } if *ty == bool_type
                    && field.as_u32() == expected_field
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::ReceivedPayload { ty } if *ty == flags_type
                    )
            )
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn assert_no_executable_source_aliases(artifact: &MantleArtifact) {
    let encoded = artifact.encode();
    assert!(
        !encoded.lines().any(|line| {
            line.ends_with("=primary")
                || line.ends_with("=secondary")
                || line.contains("debug_name=primary")
                || line.contains("debug_name=secondary")
        }),
        "nested branch artifact must not dispatch through source aliases"
    );
}

fn add_over_limit_nested_action(artifact: &mut MantleArtifact) {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let transition = worker
        .transitions
        .first_mut()
        .expect("Worker should have one Check transition");
    let [ArtifactAction::IfElse { then_actions, .. }] = transition.actions.as_mut_slice() else {
        panic!("outer branch action should be present");
    };
    let [
        ArtifactAction::IfElse {
            condition,
            then_actions,
            ..
        },
    ] = then_actions.as_mut_slice()
    else {
        panic!("nested branch action should be present");
    };
    let extra_nested_action = ArtifactAction::IfElse {
        condition: condition.clone(),
        then_actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
        else_actions: Vec::new(),
    };
    then_actions.insert(0, extra_nested_action);
}
