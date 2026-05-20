use super::*;

#[test]
fn state_match_dispatches_same_message_by_disjoint_fieldless_nested_predicates() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let checked = check_source(&source)
        .expect("state-match same-message predicate split should check when guards are disjoint");
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

    let artifact = lower_to_artifact(&checked, &source)
        .expect("payload-specific state-match steps should lower");
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
        "payload-specific state-match dispatch must not lower constructor labels as executable fields"
    );
}

#[test]
fn state_match_payload_split_revisits_guards_for_payload_derived_states() {
    let body = payload_derived_state_match_body();
    let source = state_match_payload_split_payload_derived_state_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {body}

    fn step(state: WorkerState, Envelope(Cancel(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {body}
"#
    ));

    let checked = check_source(&source)
        .expect("state-match payload split should revisit guards for payload-derived states");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "Saw(Ready)", "Saw(Done)"]
    );
    assert_eq!(worker.transitions().len(), 6);

    let artifact = lower_to_artifact(&checked, &source)
        .expect("payload-derived state-match split should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
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
            (state_label, guard.label())
        })
        .collect::<Vec<_>>();
    keys.sort();

    assert_eq!(
        keys,
        [
            ("Idle".to_string(), "Assign(Ready)".to_string()),
            ("Idle".to_string(), "Cancel(Done)".to_string()),
            ("Saw(Done)".to_string(), "Assign(Ready)".to_string()),
            ("Saw(Done)".to_string(), "Cancel(Done)".to_string()),
            ("Saw(Ready)".to_string(), "Assign(Ready)".to_string()),
            ("Saw(Ready)".to_string(), "Cancel(Done)".to_string()),
        ]
    );
}
