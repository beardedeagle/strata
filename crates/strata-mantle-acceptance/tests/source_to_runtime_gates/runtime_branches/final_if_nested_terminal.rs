use super::super::support::*;

const STEM: &str = "runtime_final_if_nested_terminal_if";
const SOURCE: &str = "examples/runtime_final_if_nested_terminal_if.str";
const ARTIFACT: &str = "target/strata/runtime_final_if_nested_terminal_if.mta";

#[test]
fn runtime_final_if_nested_terminal_if_selects_nested_next_state_branch() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_output_once(&stdout, "worker terminal outer true inner true");
    assert_output_once(&stdout, "worker terminal outer true inner false");
    assert_output_once(&stdout, "worker terminal outer false inner true");
    assert_output_once(&stdout, "worker terminal outer false inner false");

    let artifact = gate.read_artifact(ARTIFACT);
    assert_nested_terminal_next_state_shape(&artifact);
    assert_no_executable_source_aliases(&artifact);

    let trace = gate.read_trace(STEM);
    assert_trace_event_count(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""scope":"next_state""#,
        ],
        8,
    );
    assert_trace_event_count(
        &trace,
        &[r#""event":"program_output""#, r#""process":"Worker""#],
        4,
    );
    assert_worker_next_state_branch(
        &trace,
        2,
        r#""branch":"then""#,
        r#""branch_path":[]"#,
        r#""condition":"True""#,
    );
    assert_worker_next_state_branch(
        &trace,
        2,
        r#""branch":"then""#,
        r#""branch_path":[16384]"#,
        r#""condition":"True""#,
    );
    assert_worker_next_state_branch(
        &trace,
        3,
        r#""branch":"then""#,
        r#""branch_path":[]"#,
        r#""condition":"True""#,
    );
    assert_worker_next_state_branch(
        &trace,
        3,
        r#""branch":"else""#,
        r#""branch_path":[16384]"#,
        r#""condition":"False""#,
    );
    assert_worker_next_state_branch(
        &trace,
        4,
        r#""branch":"else""#,
        r#""branch_path":[]"#,
        r#""condition":"False""#,
    );
    assert_worker_next_state_branch(
        &trace,
        4,
        r#""branch":"then""#,
        r#""branch_path":[16385]"#,
        r#""condition":"True""#,
    );
    assert_worker_next_state_branch(
        &trace,
        5,
        r#""branch":"else""#,
        r#""branch_path":[]"#,
        r#""condition":"False""#,
    );
    assert_worker_next_state_branch(
        &trace,
        5,
        r#""branch":"else""#,
        r#""branch_path":[16385]"#,
        r#""condition":"False""#,
    );

    assert_selected_nested_path(
        &trace,
        2,
        r#""branch":"then""#,
        r#""branch_path":[16384]"#,
        "worker terminal outer true inner true",
        "OuterTrueInnerTrue",
    );
    assert_selected_nested_path(
        &trace,
        3,
        r#""branch":"else""#,
        r#""branch_path":[16384]"#,
        "worker terminal outer true inner false",
        "OuterTrueInnerFalse",
    );
    assert_selected_nested_path(
        &trace,
        4,
        r#""branch":"then""#,
        r#""branch_path":[16385]"#,
        "worker terminal outer false inner true",
        "OuterFalseInnerTrue",
    );
    assert_selected_nested_path(
        &trace,
        5,
        r#""branch":"else""#,
        r#""branch_path":[16385]"#,
        "worker terminal outer false inner false",
        "OuterFalseInnerFalse",
    );
}

#[test]
fn runtime_final_if_nested_terminal_if_rejects_deeper_next_state_before_runtime() {
    let gate = GateHarness::new();
    let seed_artifact_path = "target/strata/runtime_final_if_nested_terminal_if_deep_seed.mta";
    let invalid_artifact_path = "target/strata/runtime_final_if_nested_terminal_if_deep.mta";
    let invalid_trace_stem = "runtime_final_if_nested_terminal_if_deep";

    gate.check(SOURCE);
    gate.build(SOURCE, seed_artifact_path);
    gate.remove_artifact(invalid_artifact_path);
    gate.remove_trace(invalid_trace_stem);

    let mut artifact = gate.read_artifact(seed_artifact_path);
    add_deeper_terminal_next_state(&mut artifact);
    gate.write_unvalidated_encoded_artifact(invalid_artifact_path, &artifact.encode());

    let run = gate.run_mantle_failure(invalid_artifact_path);

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("next_state runtime if nesting exceeds maximum depth of 2"),
        "unexpected diagnostic\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stdout.contains("mantle: loaded"));
    assert!(!gate.trace_exists(invalid_trace_stem));
}

fn assert_output_once(stdout: &str, text: &str) {
    assert_eq!(
        stdout.matches(text).count(),
        1,
        "expected exactly one stdout line containing {text:?}\n{stdout}"
    );
}

fn assert_trace_event_count(trace: &str, fields: &[&str], expected: usize) {
    let count = trace
        .lines()
        .filter(|line| fields.iter().all(|field| line.contains(field)))
        .count();
    assert_eq!(
        count, expected,
        "expected {expected} trace events containing fields {fields:?}, got {count}\n{trace}"
    );
}

fn assert_worker_next_state_branch(
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
            r#""scope":"next_state""#,
            branch,
            branch_path,
            condition,
        ],
    );
}

fn assert_selected_nested_path(
    trace: &str,
    pid: u64,
    branch: &str,
    branch_path: &str,
    output: &str,
    state: &str,
) {
    let nested_next_state = trace_line_index_with_fields(
        trace,
        &[
            &format!(r#""event":"branch_selected","pid":{pid}"#),
            r#""scope":"next_state""#,
            branch,
            branch_path,
        ],
    );
    let selected_output = trace_line_index_with_fields(
        trace,
        &[
            &format!(r#""event":"program_output","pid":{pid}"#),
            &format!(r#""text":"{output}""#),
        ],
    );
    let selected_state = trace_line_index_with_fields(
        trace,
        &[
            &format!(r#""event":"state_updated","pid":{pid}"#),
            &format!(r#""to":"{state}""#),
        ],
    );
    assert!(
        nested_next_state < selected_output,
        "pid {pid} should select nested next_state before output"
    );
    assert!(
        selected_output < selected_state,
        "pid {pid} should emit selected output before whole-value state update"
    );
}

fn assert_nested_terminal_next_state_shape(artifact: &MantleArtifact) {
    let bool_type = value_type_id(artifact, "Bool");
    let flags_type = value_type_id(artifact, "CheckFlags");
    let worker = artifact_process(artifact, "Worker");
    let transition = worker
        .transitions
        .first()
        .expect("Worker should have one Check transition");
    assert!(transition.effects.contains(&ArtifactEffect::Emit));
    assert!(matches!(
        &transition.next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "outer_flag")
            && nested_terminal_branch_returns_values(then_state.as_ref(), bool_type, flags_type)
            && nested_terminal_branch_returns_values(else_state.as_ref(), bool_type, flags_type)
    ));
}

fn nested_terminal_branch_returns_values(
    next_state: &NextState,
    bool_type: TypeId,
    flags_type: TypeId,
) -> bool {
    matches!(
        next_state,
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } if condition_is_bool_field_check(condition, bool_type, flags_type, "inner_flag")
            && matches!(then_state.as_ref(), NextState::Value(_))
            && matches!(else_state.as_ref(), NextState::Value(_))
    )
}

fn condition_is_bool_field_check(
    condition: &ArtifactValueTemplate,
    bool_type: TypeId,
    flags_type: TypeId,
    expected_field: &str,
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
    expected_field: &str,
) -> bool {
    matches!(
        template,
        ArtifactValueTemplate::RecordField {
            ty,
            record,
            field,
        } if *ty == bool_type
            && field == expected_field
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
            .any(|line| line.ends_with("=outer") || line.ends_with("=inner")),
        "nested terminal runtime if artifact must not dispatch through source aliases"
    );
}

fn add_deeper_terminal_next_state(artifact: &mut MantleArtifact) {
    let worker = artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    let transition = worker
        .transitions
        .first_mut()
        .expect("Worker should have one Check transition");
    let NextState::IfElse { then_state, .. } = &mut transition.next_state else {
        panic!("Worker transition should contain final-position outer next_state branch");
    };
    let NextState::IfElse {
        condition,
        then_state,
        ..
    } = then_state.as_mut()
    else {
        panic!("outer then branch should contain direct nested terminal next_state branch");
    };
    **then_state = NextState::IfElse {
        condition: condition.clone(),
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Value(StateId::new(2))),
    };
}
