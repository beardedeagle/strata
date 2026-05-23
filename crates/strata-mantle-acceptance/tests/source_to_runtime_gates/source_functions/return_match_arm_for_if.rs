use crate::support::*;

const STEM: &str = "process_return_match_arm_for_if_prefix";
const SOURCE: &str = "examples/process_return_match_arm_for_if_prefix.str";
const ARTIFACT: &str = "target/strata/process_return_match_arm_for_if_prefix.mta";

#[test]
fn process_return_match_arm_for_if_prefix_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout.matches("return-match for-if uniform prefix").count(),
        2
    );
    assert_eq!(
        stdout.matches("return-match ready for-if prefix").count(),
        1
    );
    assert_eq!(stdout.matches("return-match done for-if prefix").count(), 1);
    assert_eq!(
        stdout
            .matches("return-match ready urgent loop branch")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready normal loop branch")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match done urgent loop branch")
            .count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match done normal loop branch")
            .count(),
        1
    );
    assert_eq!(
        stdout.matches("sink received ready for-if notice").count(),
        2
    );
    assert_eq!(
        stdout.matches("sink received done for-if notice").count(),
        2
    );

    let artifact = gate.read_artifact(ARTIFACT);
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    for transition in &worker.transitions {
        assert_eq!(
            transition.effects,
            [
                ArtifactEffect::Emit,
                ArtifactEffect::Spawn,
                ArtifactEffect::Send
            ]
        );
        assert!(
            is_selected_arm_for_if_transition(transition),
            "Worker transition should lower the selected arm loop and runtime branch through typed artifact actions: {transition:?}"
        );
    }
    let encoded = artifact.encode();
    assert!(
        !encoded
            .lines()
            .any(|line| line.contains("job_phase") || line.contains("job_urgent")),
        "artifact must not dispatch through source loop aliases"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""result":"Stop""#,
            r#""state":"SawDone""#,
        ],
    );

    let uniform = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match for-if uniform prefix""#,
        ],
    );
    let ready_prefix = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready for-if prefix""#,
        ],
    );
    let loop_start = trace_line_index_with_fields(
        &trace,
        &[r#""event":"loop_started""#, r#""process":"Worker""#],
    );
    let first_iteration = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"Worker""#,
            r#""index":0"#,
        ],
    );
    let urgent_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
        ],
    );
    let urgent_output = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready urgent loop branch""#,
        ],
    );
    let second_iteration = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_iteration""#,
            r#""process":"Worker""#,
            r#""index":1"#,
        ],
    );
    let normal_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
        ],
    );
    let normal_output = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready normal loop branch""#,
        ],
    );

    assert!(uniform < ready_prefix);
    assert!(ready_prefix < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < urgent_branch);
    assert!(urgent_branch < urgent_output);
    assert!(urgent_output < second_iteration);
    assert!(second_iteration < normal_branch);
    assert!(normal_branch < normal_output);
}

fn is_selected_arm_for_if_transition(transition: &ArtifactTransition) -> bool {
    matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::Emit { .. },
            ArtifactAction::ForEach { body, .. },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            }] if branch_sends_loop_projection(then_actions)
                && branch_sends_loop_projection(else_actions)
        )
    )
}

fn branch_sends_loop_projection(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Send {
                payload:
                    Some(ArtifactValueTemplate::RecordField {
                        record,
                        field,
                        ..
                    }),
                ..
            },
        ] if field == "phase"
            && matches!(
                record.as_ref(),
                ArtifactValueTemplate::LoopElement { .. }
            )
    )
}
