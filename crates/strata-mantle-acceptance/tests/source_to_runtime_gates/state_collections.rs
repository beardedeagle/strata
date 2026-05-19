use super::support::*;

#[test]
fn state_payload_enum_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/state_payload_enum.str",
        "target/strata/state_payload_enum.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker entered payload state"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/state_payload_enum.mta");
    let worker = &artifact.processes[1];
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "Working(Job{phase:Ready})");
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: value_type_id(&artifact, "WorkerState"),
            variant: EnumVariantId::new(1),
            payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: value_type_id(&artifact, "Job"),
            }),
        })
    );

    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("state_payload_enum");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Idle","to_state_id":1,"to":"Working(Job{phase:Ready})""#
    ));
}

#[test]
fn collection_state_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/collection_state.str",
        "target/strata/collection_state.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("collection state replaced"));
    assert!(stdout.contains("collection map state replaced"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert!(stdout.contains("mantle: stopped MapWorker normally"));

    let artifact = gate.read_artifact("target/strata/collection_state.mta");
    let worker = &artifact.processes[1];
    let list_type = value_type_id(&artifact, "__strata_checked_4_List_1_1_5_Phase_1");
    let payload_list_type = value_type_id(&artifact, "__strata_checked_4_List_1_1_5_Phase_2");
    let phase_type = value_type_id(&artifact, "Phase");
    assert_eq!(worker.state_values[0].label, "List[Ready]");
    assert_eq!(worker.state_values[1].label, "List[Done]");
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::ListRest {
            ty: list_type,
            list: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: payload_list_type,
            }),
            prefix_len: 1,
        })
    );

    let map_worker = &artifact.processes[2];
    let map_type = value_type_id(&artifact, "__strata_checked_3_Map_2_1_5_Phase_5_Phase_2");
    assert_eq!(
        map_worker.state_values[0].label,
        "Map[Ready=>Ready,Done=>Ready]"
    );
    assert_eq!(
        map_worker.state_values[1].label,
        "Map[Ready=>Done,Done=>Ready]"
    );
    assert_eq!(
        map_worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Map {
            ty: map_type,
            entries: vec![
                ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Ready"),
                    },
                    value: ArtifactValueTemplate::MapValue {
                        ty: phase_type,
                        map: Box::new(ArtifactValueTemplate::ReceivedPayload { ty: map_type }),
                        key: artifact_value("Ready"),
                        keys: vec![artifact_value("Ready")],
                        projection: mantle_artifact::MapProjectionMode::Subset,
                    },
                },
                ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Done"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: phase_type,
                        value: artifact_value("Ready"),
                    },
                },
            ],
        })
    );

    let trace = gate.read_trace("collection_state");
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":0,"text":"collection state replaced""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"List[Ready]","to_state_id":1,"to":"List[Done]""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":3,"process_id":2,"process":"MapWorker","stream":"stdout","output_id":1,"text":"collection map state replaced""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":3,"process_id":2,"process":"MapWorker","from_state_id":0,"from":"Map[Ready=>Ready,Done=>Ready]","to_state_id":1,"to":"Map[Ready=>Done,Done=>Ready]""#
    ));
}

#[test]
fn state_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/state_payload_match.str",
        "target/strata/state_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("worker accepted job"));
    assert!(stdout.contains("worker completed job"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/state_payload_match.mta");
    let worker = &artifact.processes[1];
    let job_type = value_type_id(&artifact, "Job");
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "Working(Job{phase:Ready})");
    assert_eq!(worker.state_values[2].label, "Done(Job{phase:Ready})");
    assert_eq!(
        worker.state_values[1].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_type,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(
        worker.state_values[2].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_type,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(worker.transitions.len(), 4);
    assert_eq!(worker.transitions[0].current_state, None);
    assert_eq!(
        worker.transitions[1].current_state,
        Some(mantle_artifact::StateId::new(0))
    );
    assert_eq!(
        worker.transitions[2].current_state,
        Some(mantle_artifact::StateId::new(1))
    );
    assert_eq!(
        worker.transitions[3].current_state,
        Some(mantle_artifact::StateId::new(2))
    );
    assert_eq!(
        worker.transitions[2].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: value_type_id(&artifact, "WorkerState"),
            variant: EnumVariantId::new(2),
            payload: Box::new(ArtifactValueTemplate::CurrentStatePayload { ty: job_type }),
        })
    );

    let trace = gate.read_trace("state_payload_match");
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"Idle","to_state_id":1,"to":"Working(Job{phase:Ready})""#
    ));
    assert!(trace.contains(
        r#""event":"message_dequeued","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Complete""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":2,"process_id":1,"process":"Worker","stream":"stdout","output_id":2,"text":"worker completed job""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":1,"from":"Working(Job{phase:Ready})","to_state_id":2,"to":"Done(Job{phase:Ready})""#
    ));
    assert!(trace.contains(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":1,"message":"Complete","result":"Stop","state_id":2,"state":"Done(Job{phase:Ready})""#
    ));
}
