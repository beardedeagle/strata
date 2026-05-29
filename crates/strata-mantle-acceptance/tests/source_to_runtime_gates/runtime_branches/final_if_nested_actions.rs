use super::super::support::*;
use mantle_artifact::{OutputId, ProcessRefId};

const STEM: &str = "runtime_final_if_nested_if_actions";
const SOURCE: &str = "examples/runtime_final_if_nested_if_actions.str";
const ARTIFACT: &str = "target/strata/runtime_final_if_nested_if_actions.mta";

#[test]
fn runtime_final_if_nested_if_actions_prefix_selected_next_state_branch() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker final outer true inner true"));
    assert!(stdout.contains("worker final outer true inner false"));
    assert!(stdout.contains("worker final outer false inner true"));
    assert!(stdout.contains("worker final outer false inner false"));
    assert_eq!(
        stdout.matches("reporter saw true").count(),
        2,
        "two selected inner paths should send true reports\n{stdout}"
    );
    assert_eq!(
        stdout.matches("reporter saw false").count(),
        2,
        "two selected inner paths should send false reports\n{stdout}"
    );

    let artifact = gate.read_artifact(ARTIFACT);
    assert_final_if_nested_action_shape(&artifact);
    assert_no_executable_source_aliases(&artifact);

    let trace = gate.read_trace(STEM);
    assert_worker_branch_trace(
        &trace,
        2,
        r#""branch":"then""#,
        r#""branch_path":[1]"#,
        r#""condition":"True""#,
    );
    assert_worker_branch_trace(
        &trace,
        2,
        r#""branch":"then""#,
        r#""branch_path":[1,4096]"#,
        r#""condition":"True""#,
    );
    assert_worker_branch_trace(
        &trace,
        3,
        r#""branch":"then""#,
        r#""branch_path":[1]"#,
        r#""condition":"True""#,
    );
    assert_worker_branch_trace(
        &trace,
        3,
        r#""branch":"else""#,
        r#""branch_path":[1,4096]"#,
        r#""condition":"False""#,
    );
    assert_worker_branch_trace(
        &trace,
        4,
        r#""branch":"else""#,
        r#""branch_path":[1]"#,
        r#""condition":"False""#,
    );
    assert_worker_branch_trace(
        &trace,
        4,
        r#""branch":"then""#,
        r#""branch_path":[1,8192]"#,
        r#""condition":"True""#,
    );
    assert_worker_branch_trace(
        &trace,
        5,
        r#""branch":"else""#,
        r#""branch_path":[1]"#,
        r#""condition":"False""#,
    );
    assert_worker_branch_trace(
        &trace,
        5,
        r#""branch":"else""#,
        r#""branch_path":[1,8192]"#,
        r#""condition":"False""#,
    );

    let next_state_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch":"then""#,
            r#""scope":"next_state""#,
        ],
    );
    let outer_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch_path":[1]"#,
            r#""scope":"action""#,
        ],
    );
    let nested_then = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected","pid":2"#,
            r#""branch_path":[1,4096]"#,
            r#""scope":"action""#,
        ],
    );
    let worker_output = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output","pid":2"#,
            r#""text":"worker final outer true inner true""#,
        ],
    );
    assert!(next_state_then < outer_then);
    assert!(outer_then < nested_then);
    assert!(nested_then < worker_output);
}

#[test]
fn runtime_final_if_nested_if_actions_rejects_deeper_branch_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_final_if_nested_if_actions_deep_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_final_if_nested_if_actions_deep.mta";
    let invalid_trace_stem = "runtime_final_if_nested_if_actions_deep";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    add_deeper_final_if_nested_action(&mut artifact);
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

fn assert_worker_branch_trace(
    trace: &str,
    pid: u64,
    branch: &str,
    branch_path: &str,
    condition: &str,
) {
    assert_trace_event(
        trace,
        &[
            &format!(r#""event":"branch_selected","pid":{pid}"#),
            r#""process":"Worker""#,
            r#""scope":"action""#,
            branch,
            branch_path,
            condition,
        ],
    );
}

fn assert_final_if_nested_action_shape(artifact: &MantleArtifact) {
    let bool_type = value_type_id(artifact, "Bool");
    let flags_type = value_type_id(artifact, "CheckFlags");
    let reporter_process = artifact_process_id(artifact, "Reporter");
    let worker = artifact_process(artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");
    assert!(transition.effects.contains(&ArtifactEffect::Spawn));
    assert!(transition.effects.contains(&ArtifactEffect::Emit));
    assert!(transition.effects.contains(&ArtifactEffect::Send));
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, 0)
            && matches!(then_state.as_ref(), NextState::Current)
            && matches!(else_state.as_ref(), NextState::Current)
    ));

    let [
        ArtifactAction::Spawn {
            target,
            process_ref,
            ..
        },
        ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        },
    ] = transition.actions.as_slice()
    else {
        panic!(
            "Worker transition should spawn Reporter before final-position branch action prefix"
        );
    };
    assert_eq!(*target, reporter_process);
    assert!(condition_is_bool_field_check(
        condition, bool_type, flags_type, 0
    ));
    assert_final_branch_contains_nested_if(then_actions, bool_type, flags_type, *process_ref);
    assert_final_branch_contains_nested_if(else_actions, bool_type, flags_type, *process_ref);
}

fn assert_final_branch_contains_nested_if(
    actions: &[ArtifactAction],
    bool_type: TypeId,
    flags_type: TypeId,
    process_ref: ProcessRefId,
) {
    assert!(matches!(
        actions,
        [ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        }] if condition_is_bool_field_check(condition, bool_type, flags_type, 1)
            && selected_inner_branch_actions_use_typed_payload(
                then_actions,
                bool_type,
                flags_type,
                process_ref
            )
            && selected_inner_branch_actions_use_typed_payload(
                else_actions,
                bool_type,
                flags_type,
                process_ref
            )
    ));
}

fn selected_inner_branch_actions_use_typed_payload(
    actions: &[ArtifactAction],
    bool_type: TypeId,
    flags_type: TypeId,
    process_ref: ProcessRefId,
) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(target_ref),
                message,
                payload: Some(payload),
                ..
            },
        ] if *target_ref == process_ref
            && *message == MessageId::new(0)
            && payload_is_bool_field(payload, bool_type, flags_type, 1)
    )
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
            && payload_is_bool_field(left, bool_type, flags_type, expected_field)
            && matches!(
                right.as_ref(),
                ArtifactValueTemplate::Literal { ty, value }
                    if *ty == bool_type && value == &artifact_value("True")
            )
    )
}

fn payload_is_bool_field(
    template: &ArtifactValueTemplate,
    bool_type: TypeId,
    flags_type: TypeId,
    expected_field: u32,
) -> bool {
    matches!(
        template,
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
}

fn assert_no_executable_source_aliases(artifact: &MantleArtifact) {
    let worker_process = artifact_process_id(artifact, "Worker");
    let transition_prefix = format!("process.{}.transition.", worker_process.as_u32());
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .filter(|line| line.starts_with(&transition_prefix))
            .any(|line| { line.ends_with("=outer") || line.ends_with("=inner") }),
        "final-position nested branch artifact must not dispatch through source aliases"
    );
}

fn add_deeper_final_if_nested_action(artifact: &mut MantleArtifact) {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let transition = worker
        .transitions
        .first_mut()
        .expect("Worker should have one Check transition");
    let [
        ArtifactAction::Spawn { .. },
        ArtifactAction::IfElse { then_actions, .. },
    ] = transition.actions.as_mut_slice()
    else {
        panic!("Worker transition should contain spawn plus final-position branch action prefix");
    };
    let [
        ArtifactAction::IfElse {
            condition,
            then_actions,
            ..
        },
    ] = then_actions.as_mut_slice()
    else {
        panic!("outer then branch should contain direct nested branch action");
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
