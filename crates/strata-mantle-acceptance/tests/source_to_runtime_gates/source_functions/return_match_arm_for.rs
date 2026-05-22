use crate::support::*;

const STEM: &str = "process_return_match_arm_for_prefix";
const SOURCE: &str = "examples/process_return_match_arm_for_prefix.str";
const ARTIFACT: &str = "target/strata/process_return_match_arm_for_prefix.mta";

#[test]
fn process_return_match_arm_for_prefix_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    gate.remove_trace(STEM);
    let run = gate.check_build_run(SOURCE, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(stdout.matches("return-match for uniform prefix").count(), 2);
    assert_eq!(stdout.matches("return-match ready for prefix").count(), 1);
    assert_eq!(stdout.matches("return-match done for prefix").count(), 1);
    assert_eq!(stdout.matches("return-match ready loop item").count(), 2);
    assert_eq!(stdout.matches("return-match done loop item").count(), 2);
    assert_eq!(stdout.matches("sink received ready loop notice").count(), 2);
    assert_eq!(stdout.matches("sink received done loop notice").count(), 2);

    let artifact = gate.read_artifact(ARTIFACT);
    let phase_type = value_type_id(&artifact, "Phase");
    let job_type = value_type_id(&artifact, "Job");
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
            is_selected_arm_for_each_transition(transition, phase_type, job_type),
            "Worker transition should lower the selected arm loop through typed artifact actions: {transition:?}"
        );
    }

    let sink = artifact_process(&artifact, "Sink");
    let mut sink_payload_guards = sink
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .map(|payload| payload.value.label())
                .expect("Sink transition should have a payload guard")
        })
        .collect::<Vec<_>>();
    sink_payload_guards.sort();
    assert_eq!(sink_payload_guards, ["Done", "Ready"]);

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
            r#""payload":"Assign(Assignment{phase:Ready,jobs:List[Job{phase:Ready},Job{phase:Done}]})""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Assignment{phase:Done,jobs:List[Job{phase:Done},Job{phase:Ready}]})""#,
            r#""result":"Stop""#,
            r#""state":"SawDone""#,
        ],
    );
    let first_uniform = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match for uniform prefix""#,
        ],
    );
    let ready_prefix = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready for prefix""#,
        ],
    );
    let first_loop = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"loop_started""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
        ],
    );
    assert!(
        first_uniform < ready_prefix && ready_prefix < first_loop,
        "uniform prefix, selected arm prefix, and arm-local loop should execute in source order"
    );
}

fn is_selected_arm_for_each_transition(
    transition: &ArtifactTransition,
    phase_type: TypeId,
    job_type: TypeId,
) -> bool {
    matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Emit { .. },
            ArtifactAction::Spawn { .. },
            ArtifactAction::Emit { .. },
            ArtifactAction::ForEach {
                element,
                collection:
                    ArtifactValueTemplate::RecordField {
                        record,
                        field,
                        ..
                    },
                max_items: 2,
                body,
            },
        ] if element.ty == job_type
            && field == "jobs"
            && matches!(
                record.as_ref(),
                ArtifactValueTemplate::EnumPayload { value, .. }
                    if matches!(value.as_ref(), ArtifactValueTemplate::ReceivedPayload { .. })
            )
            && matches!(
                body.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Send {
                        payload:
                            Some(ArtifactValueTemplate::RecordField {
                                ty,
                                record,
                                field,
                            }),
                        ..
                    },
                ] if *ty == phase_type
                    && field == "phase"
                    && matches!(
                        record.as_ref(),
                        ArtifactValueTemplate::LoopElement {
                            ty,
                            element: payload_element,
                        } if *ty == job_type && *payload_element == element.id
                    )
            )
    )
}
