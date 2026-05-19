use super::support::*;

#[test]
fn actor_payload_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_match.str",
        "target/strata/actor_payload_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Assign(Job{phase:Ready}) to Worker"));
    assert!(stdout.contains("worker matched Assign payload"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_match.mta");
    let job_type = value_type_id(&artifact, "Job");
    let payload_type = format!(r#""payload_type_id":{}"#, job_type.as_u32());
    let trace = gate.read_trace("actor_payload_match");
    assert!(trace.contains(&format!(
        r#""event":"message_accepted","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}""#
    )));
    assert!(trace.contains(r#""event":"state_updated","pid":2,"process_id":1,"process":"Worker","from_state_id":0,"from":"WorkerState{job:Job{phase:Done}}","to_state_id":1,"to":"WorkerState{job:Job{phase:Ready}}""#));
    assert!(trace.contains(&format!(
        r#""event":"process_stepped","pid":2,"process_id":1,"process":"Worker","message_id":0,"message":"Assign",{payload_type},"payload":"Job{{phase:Ready}}","result":"Stop","state_id":1,"state":"WorkerState{{job:Job{{phase:Ready}}}}""#
    )));
}

#[test]
fn actor_payload_split_match_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_match.str",
        "target/strata/actor_payload_split_match.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_match.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message split should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .value
                .clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific dispatch must not lower constructor names as executable fields"
    );

    let routed_type = value_type_id(&artifact, "Routed");
    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_match");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_split_signature_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_signature.str",
        "target/strata/actor_payload_split_signature.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_signature.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message signature split should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .value
                .clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific signature dispatch must not lower constructor names as executable fields"
    );

    let routed_type = value_type_id(&artifact, "Routed");
    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_signature");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_split_signature_wildcard_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_split_signature_wildcard.str",
        "target/strata/actor_payload_split_signature_wildcard.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled fallback assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_split_signature_wildcard.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 2);
    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.payload_guard.is_some()),
        "same-message signature wildcard fallback should lower exact typed payload guards"
    );
    let mut payload_guards = worker
        .transitions
        .iter()
        .map(|transition| {
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard");
            assert_eq!(guard.ty, routed_type);
            guard.value.clone()
        })
        .collect::<Vec<_>>();
    payload_guards.sort();
    assert_eq!(
        payload_guards,
        [
            artifact_value("Assign(Done)"),
            artifact_value("Assign(Ready)")
        ]
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific signature wildcard must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_split_signature_wildcard");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_state_match_split_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_state_match_split.str",
        "target/strata/actor_payload_state_match_split.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled Done assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_state_match_split.mta");
    let worker = artifact_process(&artifact, "Worker");
    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");

    assert_eq!(worker.transitions.len(), 6);
    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "SawReady");
    assert_eq!(worker.state_values[2].label, "Done");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.current_state.is_some()
                && transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.ty == routed_type)),
        "state-match payload split should lower message, current-state, and exact typed payload guards"
    );

    let mut transition_keys = worker
        .transitions
        .iter()
        .map(|transition| {
            let current_state = transition
                .current_state
                .expect("state-match transition should carry current state");
            let state_label = worker.state_values[current_state.index()].label.clone();
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            let result_label = match transition.step_result {
                mantle_artifact::StepResult::Continue => "Continue",
                mantle_artifact::StepResult::Stop => "Stop",
                mantle_artifact::StepResult::Panic => "Panic",
            };
            (state_label, guard.value.clone(), result_label)
        })
        .collect::<Vec<_>>();
    transition_keys.sort();

    assert_eq!(
        transition_keys,
        [
            ("Done".to_string(), artifact_value("Assign(Done)"), "Stop"),
            ("Done".to_string(), artifact_value("Assign(Ready)"), "Stop"),
            ("Idle".to_string(), artifact_value("Assign(Done)"), "Stop"),
            (
                "Idle".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Done)"),
                "Stop"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
        ]
    );

    let encoded = artifact.encode();
    assert!(encoded.contains(".current_state="));
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific state-match dispatch must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_state_match_split");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"state_updated""#,
            r#""process":"Worker""#,
            r#""from_state_id":0"#,
            r#""from":"Idle""#,
            r#""to_state_id":1"#,
            r#""to":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}

#[test]
fn actor_payload_state_match_wildcard_checks_builds_and_runs_on_mantle() {
    let gate = GateHarness::new();
    let run = gate.check_build_run(
        "examples/actor_payload_state_match_wildcard.str",
        "target/strata/actor_payload_state_match_wildcard.mta",
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Ready)) to Worker"));
    assert!(stdout.contains("mantle: delivered Envelope(Assign(Done)) to Worker"));
    assert!(stdout.contains("worker handled Ready assignment"));
    assert!(stdout.contains("worker handled wildcard assignment"));
    assert!(stdout.contains("mantle: stopped Worker normally"));

    let artifact = gate.read_artifact("target/strata/actor_payload_state_match_wildcard.mta");
    let worker = artifact_process(&artifact, "Worker");
    assert_eq!(worker.transitions.len(), 6);

    let worker_message = worker.transitions[0].message;
    let routed_type = value_type_id(&artifact, "Routed");

    assert_eq!(worker.state_values[0].label, "Idle");
    assert_eq!(worker.state_values[1].label, "SawReady");
    assert_eq!(worker.state_values[2].label, "Done");
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == worker_message
                && transition.current_state.is_some()
                && transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.ty == routed_type)),
        "state-match wildcard fallback should lower current-state and exact typed payload guards"
    );

    let mut transition_keys = worker
        .transitions
        .iter()
        .map(|transition| {
            let current_state = transition
                .current_state
                .expect("state-match transition should carry current state");
            let state_label = worker.state_values[current_state.index()].label.clone();
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            let result_label = match transition.step_result {
                mantle_artifact::StepResult::Continue => "Continue",
                mantle_artifact::StepResult::Stop => "Stop",
                mantle_artifact::StepResult::Panic => "Panic",
            };
            (state_label, guard.value.clone(), result_label)
        })
        .collect::<Vec<_>>();
    transition_keys.sort();

    assert_eq!(
        transition_keys,
        [
            ("Done".to_string(), artifact_value("Assign(Done)"), "Stop"),
            ("Done".to_string(), artifact_value("Assign(Ready)"), "Stop"),
            ("Idle".to_string(), artifact_value("Assign(Done)"), "Stop"),
            (
                "Idle".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Done)"),
                "Stop"
            ),
            (
                "SawReady".to_string(),
                artifact_value("Assign(Ready)"),
                "Continue"
            ),
        ]
    );

    let encoded = artifact.encode();
    assert!(encoded.contains(".current_state="));
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific state-match wildcard fallback must not lower constructor names as executable fields"
    );

    let message_id = format!(r#""message_id":{}"#, worker_message.as_u32());
    let payload_type = format!(r#""payload_type_id":{}"#, routed_type.as_u32());
    let trace = gate.read_trace("actor_payload_state_match_wildcard");
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"state_updated""#,
            r#""process":"Worker""#,
            r#""from_state_id":0"#,
            r#""from":"Idle""#,
            r#""to_state_id":1"#,
            r#""to":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Ready)""#,
            r#""result":"Continue""#,
            r#""state":"SawReady""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"message_dequeued""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"process_stepped""#,
            r#""process":"Worker""#,
            r#""message":"Envelope""#,
            message_id.as_str(),
            payload_type.as_str(),
            r#""payload":"Assign(Done)""#,
            r#""result":"Stop""#,
            r#""state":"Done""#,
        ],
    );
}
