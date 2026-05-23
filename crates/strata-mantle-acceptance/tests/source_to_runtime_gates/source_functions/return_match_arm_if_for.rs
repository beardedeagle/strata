use crate::support::*;

const STEM: &str = "process_return_match_arm_if_for_prefix";
const SOURCE: &str = "examples/process_return_match_arm_if_for_prefix.str";
const ARTIFACT: &str = "target/strata/process_return_match_arm_if_for_prefix.mta";

#[test]
fn process_return_match_arm_if_for_prefix_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout.matches("return-match if-for uniform prefix").count(),
        2
    );
    assert_eq!(
        stdout.matches("return-match ready if-for prefix").count(),
        1
    );
    assert_eq!(stdout.matches("return-match done if-for prefix").count(), 1);
    assert_eq!(
        stdout.matches("return-match ready enabled branch").count(),
        1
    );
    assert_eq!(
        stdout.matches("return-match ready disabled branch").count(),
        0
    );
    assert_eq!(
        stdout.matches("return-match done enabled branch").count(),
        0
    );
    assert_eq!(
        stdout.matches("return-match done disabled branch").count(),
        1
    );
    assert_eq!(
        stdout
            .matches("return-match ready branch loop item")
            .count(),
        2
    );
    assert_eq!(
        stdout.matches("return-match done branch loop item").count(),
        2
    );
    assert_eq!(
        stdout.matches("sink received ready if-for notice").count(),
        2
    );
    assert_eq!(
        stdout.matches("sink received done if-for notice").count(),
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
            is_selected_arm_if_for_transition(transition),
            "Worker transition should lower selected arm runtime-if branch loops through typed artifact actions: {transition:?}"
        );
    }
    let encoded = artifact.encode();
    assert!(
        !encoded.lines().any(|line| line.contains("job_phase")),
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
            r#""text":"return-match if-for uniform prefix""#,
        ],
    );
    let ready_prefix = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready if-for prefix""#,
        ],
    );
    let selected_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
        ],
    );
    let branch_prefix = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready enabled branch""#,
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
    let loop_output = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready branch loop item""#,
        ],
    );

    assert!(uniform < ready_prefix);
    assert!(ready_prefix < selected_branch);
    assert!(selected_branch < branch_prefix);
    assert!(branch_prefix < loop_start);
    assert!(loop_start < first_iteration);
    assert!(first_iteration < loop_output);
}

fn is_selected_arm_if_for_transition(transition: &ArtifactTransition) -> bool {
    matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::Emit { .. },
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            },
        ] if (branch_sends_loop_projection(then_actions)
            && matches!(else_actions.as_slice(), [ArtifactAction::Emit { .. }]))
            || (matches!(then_actions.as_slice(), [ArtifactAction::Emit { .. }])
                && branch_sends_loop_projection(else_actions))
    )
}

fn branch_sends_loop_projection(actions: &[ArtifactAction]) -> bool {
    matches!(
        actions,
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::ForEach { body, .. },
        ] if matches!(
            body.as_slice(),
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
    )
}
