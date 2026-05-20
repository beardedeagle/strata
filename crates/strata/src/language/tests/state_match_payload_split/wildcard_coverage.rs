use super::*;

#[test]
fn state_match_payload_wildcard_covers_discovered_same_message_misses() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let checked = check_source(&source)
        .expect("state-match wildcard should cover discovered same-message guarded misses");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(checked_state_labels(worker), ["Idle", "SawReady", "Done"]);
    assert_eq!(worker.transitions().len(), 6);
    assert!(worker.transitions().iter().all(|transition| {
        transition.message() == checked_message_id(0)
            && transition.current_state().is_some()
            && transition.payload_guard().is_some()
    }));

    let artifact =
        lower_to_artifact(&checked, &source).expect("state-match wildcard fallback should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let routed_type = artifact_type_id(&artifact, "Routed");

    assert_eq!(artifact_state_labels(worker), ["Idle", "SawReady", "Done"]);
    assert_eq!(worker.transitions.len(), 6);
    assert!(worker.transitions.iter().all(|transition| {
        transition.message == MessageId::new(0)
            && transition.current_state.is_some()
            && transition
                .payload_guard
                .as_ref()
                .is_some_and(|guard| guard.ty == routed_type)
    }));

    let mut keys = worker
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
                StepResult::Continue => "Continue",
                StepResult::Stop => "Stop",
                StepResult::Panic => "Panic",
            };
            (state_label, guard.label(), result_label)
        })
        .collect::<Vec<_>>();
    keys.sort();

    assert_eq!(
        keys,
        [
            ("Done".to_string(), "Assign(Done)".to_string(), "Stop"),
            ("Done".to_string(), "Assign(Ready)".to_string(), "Stop"),
            ("Idle".to_string(), "Assign(Done)".to_string(), "Stop"),
            ("Idle".to_string(), "Assign(Ready)".to_string(), "Continue"),
            ("SawReady".to_string(), "Assign(Done)".to_string(), "Stop"),
            (
                "SawReady".to_string(),
                "Assign(Ready)".to_string(),
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
        "state-match wildcard fallback must not lower constructor labels as executable fields"
    );
}

#[test]
fn rejects_state_match_wildcard_before_payload_sensitive_signature_clause() {
    let wildcard_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {wildcard_body}

    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Continue(SawReady);
    }}
"#
    ));

    let err = check_source(&source)
        .expect_err("state-match wildcard before payload-sensitive signature should fail");
    assert!(
        err.to_string().contains(
            "process Worker declares payload-sensitive step pattern for message Envelope with a state match wildcard step pattern"
        ),
        "expected order-independent state-match wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_state_match_wildcard_after_payload_sensitive_signature_clause() {
    let wildcard_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Continue(SawReady);
    }}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {wildcard_body}
"#
    ));

    let err = check_source(&source)
        .expect_err("state-match wildcard after payload-sensitive signature should fail");
    assert!(
        err.to_string().contains(
            "process Worker declares payload-sensitive step pattern for message Envelope with a state match wildcard step pattern"
        ),
        "expected order-independent state-match wildcard diagnostic, got {err}"
    );
}

#[test]
fn state_match_payload_wildcard_does_not_create_dynamic_catch_all() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let fallback_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case_with_other(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {fallback_body}
"#
    ));

    let checked = check_source(&source)
        .expect("state-match wildcard should cover only discovered guarded misses");
    let artifact =
        lower_to_artifact(&checked, &source).expect("state-match wildcard fallback should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let routed_type = artifact_type_id(&artifact, "Routed");
    let mut payloads = worker
        .transitions
        .iter()
        .map(|transition| {
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            assert_eq!(guard.ty, routed_type);
            guard.label()
        })
        .collect::<Vec<_>>();
    payloads.sort();
    payloads.dedup();

    assert_eq!(payloads, ["Assign(Other)", "Assign(Ready)"]);
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.payload_guard.is_some()),
        "state-match wildcard fallback must lower exact discovered payload guards only"
    );
}
