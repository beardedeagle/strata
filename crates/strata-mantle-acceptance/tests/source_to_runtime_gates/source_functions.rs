use super::support::*;

#[path = "source_functions/local_bindings.rs"]
mod local_bindings;
#[path = "source_functions/return_match_arm_action_block.rs"]
mod return_match_arm_action_block;
#[path = "source_functions/return_match_arm_bounded_runtime.rs"]
mod return_match_arm_bounded_runtime;
#[path = "source_functions/return_match_arm_for.rs"]
mod return_match_arm_for;
#[path = "source_functions/return_match_arm_for_if.rs"]
mod return_match_arm_for_if;
#[path = "source_functions/return_match_arm_if_for.rs"]
mod return_match_arm_if_for;

#[test]
fn function_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_match.str",
        "target/strata/function_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source functions selected WarmReady"));
    assert!(stdout.contains("process-local function assigned job"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/function_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.debug_name, "Main");
    assert_eq!(main.init_state, mantle_artifact::StateId::new(0));
    assert_eq!(
        main.state_values[0].label,
        "MainState{signature:WarmReady,body:WarmReady}"
    );

    let worker = &artifact.processes[1];
    assert_eq!(worker.debug_name, "Worker");
    assert_eq!(
        worker.state_values[0].label,
        "WorkerState{job:Job{phase:Done}}"
    );
    assert_eq!(
        worker.state_values[1].label,
        "WorkerState{job:Job{phase:Ready}}"
    );
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Record {
            ty: value_type_id(&artifact, "WorkerState"),
            fields: vec![ArtifactValueTemplateField {
                name: "job".to_string(),
                value: ArtifactValueTemplate::ReceivedPayload {
                    ty: value_type_id(&artifact, "Job"),
                },
            }],
        })
    );

    let trace = gate.read_trace("function_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{signature:WarmReady,body:WarmReady}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source functions selected WarmReady""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#
    ));
}

#[test]
fn function_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_payload_match.str",
        "target/strata/function_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function matched payload enum"));
    assert!(stdout.contains("process-local function wrapped payload enum"));
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/function_payload_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}"
    );

    let worker = &artifact.processes[1];
    assert_eq!(worker.state_values[0].label, "WorkerState{work:Empty}");
    assert_eq!(
        worker.state_values[1].label,
        "WorkerState{work:Assigned(Job{phase:Ready})}"
    );
    assert_eq!(
        worker.transitions[0].next_state,
        mantle_artifact::NextState::Template(ArtifactValueTemplate::Record {
            ty: value_type_id(&artifact, "WorkerState"),
            fields: vec![ArtifactValueTemplateField {
                name: "work".to_string(),
                value: ArtifactValueTemplate::EnumVariant {
                    ty: value_type_id(&artifact, "Work"),
                    variant: EnumVariantId::new(1),
                    payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                        ty: value_type_id(&artifact, "Job"),
                    }),
                },
            }],
        })
    );

    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("function_payload_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{signature:Active(Job{phase:Ready}),body:Active(Job{phase:Done})}""#
    ));
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(
        r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{work:Empty}","to_state_id":1,"to":"WorkerState{work:Assigned(Job{phase:Ready})}""#
    ));
}

#[test]
fn function_if_else_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_if_else.str",
        "target/strata/function_if_else.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("pure conditional selected source values"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_if_else.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values.len(), 2);
    assert_eq!(
        main.state_values[0].label,
        "MainState{init:WarmReady,step:ColdReady}"
    );
    assert_eq!(
        main.state_values[1].label,
        "MainState{init:ColdReady,step:WarmReady}"
    );
    let encoded = artifact.encode();
    assert!(!encoded.contains("is_warm"));
    assert!(!encoded.contains("choose"));
    assert!(!encoded.contains("choose_block"));
    assert!(!encoded.contains("readiness"));
    assert!(!encoded.contains("readiness_block"));

    let trace = gate.read_trace("function_if_else");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{init:WarmReady,step:ColdReady}""#
    ));
    assert!(trace.contains(
        r#""event":"state_updated","pid":1,"process_id":0,"process":"Main","from_state_id":0,"from":"MainState{init:WarmReady,step:ColdReady}","to_state_id":1,"to":"MainState{init:ColdReady,step:WarmReady}""#
    ));
}

#[test]
fn function_collection_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_collection_match.str",
        "target/strata/function_collection_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function collection match selected values"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_collection_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{selected:Ready,tail:List[Done]}"
    );

    let trace = gate.read_trace("function_collection_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{selected:Ready,tail:List[Done]}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source function collection match selected values""#
    ));
}

#[test]
fn nested_patterns_check_build_and_run_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/nested_patterns.str",
        "target/strata/nested_patterns.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));

    let trace = gate.read_trace("nested_patterns");
    assert!(trace.contains("\"event\":\"artifact_loaded\""));

    let artifact = gate.read_artifact("target/strata/nested_patterns.mta");
    let encoded = artifact.encode();
    assert!(
        encoded.contains(".kind=enum_payload"),
        "nested constructor projection should lower as typed enum payload templates"
    );
}

#[test]
fn function_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_return_match.str",
        "target/strata/function_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function return match selected payload"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_return_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(
        main.state_values[0].label,
        "MainState{status:Active(Job{phase:Ready})}"
    );

    let trace = gate.read_trace("function_return_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{status:Active(Job{phase:Ready})}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source function return match selected payload""#
    ));
}

#[test]
fn process_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/process_return_match.str",
        "target/strata/process_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout
            .matches("process return match uniform prefix")
            .count(),
        2
    );

    let artifact = gate.read_artifact("target/strata/process_return_match.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(
        worker
            .state_values
            .iter()
            .map(|state| state.label.as_str())
            .collect::<Vec<_>>(),
        ["Idle", "SawReady", "Done"]
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .map(|payload| payload.value.label())
                .expect("process return-match transition should have a payload guard")
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(payload_guards, ["Assign(Done)", "Assign(Ready)"]);
    for transition in &worker.transitions {
        assert_eq!(transition.effects, [ArtifactEffect::Emit]);
        assert!(
            matches!(transition.actions.as_slice(), [ArtifactAction::Emit { .. }]),
            "process return-match prefix must lower as one typed emit action"
        );
    }
    let encoded = artifact.encode();
    assert!(
        !encoded.contains("field_name=Assign"),
        "process return-match must not lower constructor names as executable fields"
    );

    let trace = gate.read_trace("process_return_match");
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_eq!(
        trace
            .matches(r#""text":"process return match uniform prefix""#)
            .count(),
        2
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn process_return_match_arm_prefix_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/process_return_match_arm_prefix.str",
        "target/strata/process_return_match_arm_prefix.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert!(stdout.contains("mantle: stopped Sink normally"));
    assert_eq!(stdout.matches("return-match uniform prefix").count(), 2);
    assert_eq!(stdout.matches("return-match ready arm prefix").count(), 1);
    assert_eq!(stdout.matches("return-match done arm prefix").count(), 1);
    assert_eq!(stdout.matches("sink received ready notice").count(), 1);
    assert_eq!(stdout.matches("sink received done notice").count(), 1);

    let artifact = gate.read_artifact("target/strata/process_return_match_arm_prefix.mta");
    assert!(
        artifact
            .outputs
            .iter()
            .any(|output| output == "return-match uniform prefix")
    );
    assert!(
        artifact
            .outputs
            .iter()
            .any(|output| output == "return-match ready arm prefix")
    );
    assert!(
        artifact
            .outputs
            .iter()
            .any(|output| output == "return-match done arm prefix")
    );
    assert!(
        artifact
            .outputs
            .iter()
            .any(|output| output == "sink received ready notice")
    );
    assert!(
        artifact
            .outputs
            .iter()
            .any(|output| output == "sink received done notice")
    );
    let worker = artifact_process(&artifact, "Worker");
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
            matches!(
                transition.actions.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Spawn { .. },
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Send { .. },
                ]
            ),
            "selected return-match arm prefix must lower as typed actions"
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

    let trace = gate.read_trace("process_return_match_arm_prefix");
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
    let worker_uniform_lines = trace
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            [
                r#""event":"program_output""#,
                r#""process":"Worker""#,
                r#""text":"return-match uniform prefix""#,
            ]
            .iter()
            .all(|field| line.contains(field))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        worker_uniform_lines.len(),
        2,
        "Worker uniform prefix should execute once per selected return-match transition"
    );
    let ready_arm = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready arm prefix""#,
        ],
    );
    let done_arm = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match done arm prefix""#,
        ],
    );
    assert!(
        worker_uniform_lines[0] < ready_arm,
        "uniform prefix should execute before selected Ready arm prefix"
    );
    assert!(
        ready_arm < worker_uniform_lines[1] && worker_uniform_lines[1] < done_arm,
        "selected Done transition should execute its own uniform prefix before its arm prefix"
    );
}

#[test]
fn process_return_match_arm_runtime_if_prefix_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/process_return_match_arm_runtime_if_prefix.str",
        "target/strata/process_return_match_arm_runtime_if_prefix.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout
            .matches("return-match runtime-if uniform prefix")
            .count(),
        2
    );
    assert_eq!(stdout.matches("return-match ready true branch").count(), 1);
    assert_eq!(stdout.matches("return-match ready false branch").count(), 0);
    assert_eq!(stdout.matches("return-match done true branch").count(), 0);
    assert_eq!(stdout.matches("return-match done false branch").count(), 1);
    assert_eq!(
        stdout.matches("sink received ready branch notice").count(),
        1
    );
    assert_eq!(
        stdout.matches("sink received done branch notice").count(),
        1
    );

    let artifact =
        gate.read_artifact("target/strata/process_return_match_arm_runtime_if_prefix.mta");
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
            matches!(
                transition.actions.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Spawn { .. },
                    ArtifactAction::IfElse { then_actions, else_actions, .. },
                ] if matches!(
                    (then_actions.as_slice(), else_actions.as_slice()),
                    ([ArtifactAction::Emit { .. }, ArtifactAction::Send { .. }], [ArtifactAction::Emit { .. }])
                        | ([ArtifactAction::Emit { .. }], [ArtifactAction::Emit { .. }, ArtifactAction::Send { .. }])
                )
            ),
            "selected return-match arm runtime-if must lower as typed branch actions"
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

    let trace = gate.read_trace("process_return_match_arm_runtime_if_prefix");
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Assignment{phase:Ready,flag:True})""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""payload":"Assign(Assignment{phase:Done,flag:False})""#,
            r#""result":"Stop""#,
            r#""state":"SawDone""#,
        ],
    );
    let first_uniform = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match runtime-if uniform prefix""#,
        ],
    );
    let ready_branch = trace_line_index_with_fields(
        &trace,
        &[
            r#""event":"program_output""#,
            r#""process":"Worker""#,
            r#""text":"return-match ready true branch""#,
        ],
    );
    assert!(
        first_uniform < ready_branch,
        "uniform prefix should execute before selected arm runtime-if branch"
    );
}

#[test]
fn function_record_pattern_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_pattern.str",
        "target/strata/function_record_pattern.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function record pattern selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_pattern.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_pattern");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source function record pattern selected field""#
    ));
}

#[test]
fn function_record_return_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_return_match.str",
        "target/strata/function_record_return_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function record return match selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_return_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_return_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source function record return match selected field""#
    ));
}

#[test]
fn function_record_body_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/function_record_body_match.str",
        "target/strata/function_record_body_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("source function record body match selected field"));
    assert!(stdout.contains("mantle: stopped Main normally"));

    let artifact = gate.read_artifact("target/strata/function_record_body_match.mta");
    let main = &artifact.processes[0];
    assert_eq!(main.state_values[0].label, "MainState{phase:Ready}");

    let trace = gate.read_trace("function_record_body_match");
    assert!(trace.contains(
        r#""event":"process_spawned","pid":1,"process_id":0,"process":"Main","state_id":0,"state":"MainState{phase:Ready}""#
    ));
    assert!(trace.contains(
        r#""event":"program_output","pid":1,"process_id":0,"process":"Main","stream":"stdout","output_id":0,"text":"source function record body match selected field""#
    ));
}
