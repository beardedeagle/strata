use super::support::*;

#[test]
fn loaded_program_selects_transitions_by_message_id() {
    let mut artifact = sequence_artifact();
    artifact.processes[1].transitions.swap(0, 1);

    let program = LoadedProgram::from_artifact(&artifact)
        .expect("artifact transitions should load by message id");
    let worker = program
        .process(ProcessId::new(1))
        .expect("worker process should be loaded");

    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), None)
            .expect("First transition should be loaded")
            .step_result,
        StepResult::Continue
    );
    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(1), StateId::new(0), None)
            .expect("Second transition should be loaded")
            .step_result,
        StepResult::Stop
    );
}

#[test]
fn loaded_program_selects_payload_guarded_transitions_by_exact_payload_identity() {
    let mut artifact = valid_artifact();
    artifact.types[WORKER_STATE.index()] = worker_state_type(&["Idle", "Ready", "Done"]);
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Idle", "Ready", "Done"]);
    replace_process_message_variants(
        &mut artifact,
        1,
        vec![ArtifactMessageVariant::payload("Assign", JOB)],
    );
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Done}")),
            step_result: StepResult::Stop,
            next_state: NextState::Value(StateId::new(2)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: None,
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Ready}")),
            step_result: StepResult::Continue,
            next_state: NextState::Value(StateId::new(1)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    let program = LoadedProgram::from_artifact(&artifact)
        .expect("payload-specific artifact transitions should load");
    let worker = program
        .process(ProcessId::new(1))
        .expect("worker process should be loaded");
    let ready_payload = RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Ready}"))
        .expect("runtime payload should load");
    let done_payload = RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Done}"))
        .expect("runtime payload should load");

    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), Some(&ready_payload))
            .expect("Ready payload transition should dispatch")
            .step_result,
        StepResult::Continue
    );
    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), Some(&done_payload))
            .expect("Done payload transition should dispatch")
            .step_result,
        StepResult::Stop
    );
    assert!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), None)
            .expect_err("payload-specific transitions must not dispatch without a payload")
            .to_string()
            .contains(
                "process Worker has payload-specific transition(s) for message id 0, but the queued message has no payload"
            )
    );
    let other_payload = RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Other}"))
        .expect("runtime payload should load");
    assert!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), Some(&other_payload))
            .expect_err("payload-specific transitions must reject unmatched payload identity")
            .to_string()
            .contains("process Worker has no transition for message id 0 payload Job{phase:Other}")
    );
}

#[test]
fn loaded_program_selects_state_specific_payload_guarded_transitions_by_exact_payload_identity() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Job{phase:Ready}"),
        }),
    };
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Idle", "Working"]);
    replace_process_message_variants(
        &mut artifact,
        1,
        vec![ArtifactMessageVariant::payload("Assign", JOB)],
    );
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: Some(StateId::new(0)),
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Ready}")),
            step_result: StepResult::Continue,
            next_state: NextState::Value(StateId::new(1)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(0)),
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Done}")),
            step_result: StepResult::Stop,
            next_state: NextState::Value(StateId::new(0)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(1)),
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Ready}")),
            step_result: StepResult::Stop,
            next_state: NextState::Value(StateId::new(1)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(1)),
            message: MessageId::new(0),
            payload_guard: Some(artifact_payload(JOB, "Job{phase:Done}")),
            step_result: StepResult::Continue,
            next_state: NextState::Value(StateId::new(0)),
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    let program = LoadedProgram::from_artifact(&artifact)
        .expect("state-specific payload transitions should load");
    let worker = program
        .process(ProcessId::new(1))
        .expect("worker process should be loaded");
    let ready_payload = RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Ready}"))
        .expect("runtime payload should load");
    let done_payload = RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Done}"))
        .expect("runtime payload should load");

    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), Some(&ready_payload))
            .expect("state 0 Ready payload transition should dispatch")
            .step_result,
        StepResult::Continue
    );
    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(0), Some(&done_payload))
            .expect("state 0 Done payload transition should dispatch")
            .step_result,
        StepResult::Stop
    );
    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(1), Some(&ready_payload))
            .expect("state 1 Ready payload transition should dispatch")
            .step_result,
        StepResult::Stop
    );
    assert_eq!(
        worker
            .transition_for_dispatch(MessageId::new(0), StateId::new(1), Some(&done_payload))
            .expect("state 1 Done payload transition should dispatch")
            .step_result,
        StepResult::Continue
    );
}

#[test]
fn loaded_program_rejects_transition_current_state_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].current_state = Some(StateId::new(99));

    let err = LoadedProgram::from_artifact(&artifact)
        .expect_err("unknown transition current state should fail loaded admission");

    assert!(
        err.to_string()
            .contains("process Worker message id 0 current_state id 99 is not a valid state value")
    );
}

#[test]
fn loaded_program_rejects_current_state_payload_template_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.types[WORKER_STATE.index()] = worker_state_type_with_payloads(&[
        ("Idle", None),
        ("Working", Some(JOB)),
        ("Done", Some(JOB)),
        ("Ready", None),
    ]);
    let mut working = state_value(WORKER_STATE, "Working(Job{phase:Ready})");
    working.payload = Some(artifact_payload(JOB, "Job{phase:Ready}"));
    artifact.processes[1].state_values = vec![state_value(WORKER_STATE, "Idle"), working];
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: Some(StateId::new(0)),
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(1)),
            message: MessageId::new(0),
            payload_guard: None,
            step_result: StepResult::Stop,
            next_state: NextState::Template(ArtifactValueTemplate::EnumVariant {
                ty: WORKER_STATE,
                variant: EnumVariantId::new(2),
                payload: Box::new(ArtifactValueTemplate::CurrentStatePayload { ty: JOB }),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    let err = LoadedProgram::from_artifact(&artifact)
        .expect_err("unadmitted current-state-derived next state should fail loaded admission");

    assert!(err.to_string().contains(
        "process Worker message id 0 current_state id 1 next_state_template produced value Done(Job{phase:Ready}) not admitted by state table"
    ));
}
