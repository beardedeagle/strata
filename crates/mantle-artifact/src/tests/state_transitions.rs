use super::support::*;

#[test]
fn validate_rejects_unknown_entry_process_id() {
    let mut artifact = valid_artifact();
    artifact.entry_process = ProcessId::new(99);

    let err = artifact
        .validate()
        .expect_err("unknown entry process id should fail");

    assert!(
        err.to_string()
            .contains("entry process id 99 is not defined")
    );
}

#[test]
fn validate_rejects_payload_bearing_entry_message() {
    let mut artifact = valid_artifact();
    artifact.processes[0].message_variants[0] =
        ArtifactMessageVariant::payload("Start", MAIN_PAYLOAD);

    let err = artifact
        .validate()
        .expect_err("entry payload message should fail");

    assert!(
        err.to_string()
            .contains("entry message id 0 must not require a payload")
    );
}

#[test]
fn validate_rejects_unknown_next_state_value_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].next_state = NextState::Value(StateId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown next state value should fail");

    assert!(
        err.to_string()
            .contains("process Worker message id 0 next_state id 99 is not a valid state value")
    );
}

#[test]
fn validate_rejects_static_next_state_template_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: "Missing".to_string(),
        });

    let err = artifact
        .validate()
        .expect_err("static next-state template outside state table should fail");

    assert!(err.to_string().contains(
        "process Worker message id 0 next_state_template produced value Missing not admitted by state table"
    ));
}

#[test]
fn validate_rejects_state_value_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.processes[1].state_values[1] =
        ArtifactStateValue::with_label(OTHER_JOB, "HandledIdentity", "HandledLabel");

    let err = artifact
        .validate()
        .expect_err("state value type mismatch should fail");

    assert!(err.to_string().contains(
        "process Worker state value HandledIdentity (label HandledLabel) has type id 5, expected 2"
    ));
}

#[test]
fn validate_rejects_next_state_template_when_label_matches_but_identity_does_not() {
    let mut artifact = valid_artifact();
    artifact.processes[1].state_values[1] =
        ArtifactStateValue::with_label(WORKER_STATE, "Spoofed", "Handled");
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: "Handled".to_string(),
        });

    let err = artifact
        .validate()
        .expect_err("state labels must not admit mismatched typed values");

    assert!(err.to_string().contains(
        "process Worker message id 0 next_state_template produced value Handled not admitted by state table"
    ));
}

#[test]
fn validate_rejects_current_state_payload_template_outside_state_table() {
    let mut artifact = valid_artifact();
    let mut working = ArtifactStateValue::with_label(
        WORKER_STATE,
        "Working(Job{phase:Ready})",
        "Working(Job{phase:Ready})",
    );
    working.payload = Some(ArtifactPayload {
        ty: JOB,
        value: "Job{phase:Ready}".to_string(),
        process_ref: None,
    });
    artifact.processes[1].state_values =
        vec![ArtifactStateValue::new(WORKER_STATE, "Idle"), working];
    artifact.processes[1].transitions = vec![
        ArtifactTransition {
            current_state: Some(StateId::new(0)),
            message: MessageId::new(0),
            step_result: StepResult::Stop,
            next_state: NextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        },
        ArtifactTransition {
            current_state: Some(StateId::new(1)),
            message: MessageId::new(0),
            step_result: StepResult::Stop,
            next_state: NextState::Template(ArtifactValueTemplate::EnumVariant {
                ty: WORKER_STATE,
                variant: "Done".to_string(),
                payload: Box::new(ArtifactValueTemplate::CurrentStatePayload { ty: JOB }),
            }),
            effects: Vec::new(),
            actions: Vec::new(),
        },
    ];

    let err = artifact
        .validate()
        .expect_err("unadmitted current-state-derived next state should fail");

    assert!(err.to_string().contains(
        "process Worker message id 0 current_state id 1 next_state_template produced value Done(Job{phase:Ready}) not admitted by state table"
    ));
}

#[test]
fn validate_rejects_missing_transition_for_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));

    let err = artifact
        .validate()
        .expect_err("missing transition should fail");

    assert!(
        err.to_string()
            .contains("process Worker has no transition for message id 1")
    );
}

#[test]
fn validate_rejects_duplicate_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));
    let duplicate = artifact.processes[1].transitions[0].clone();
    artifact.processes[1].transitions.push(duplicate);

    let err = artifact
        .validate()
        .expect_err("duplicate transition should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate transition for message id 0")
    );
}

#[test]
fn validate_rejects_unknown_transition_message() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].message = MessageId::new(1);

    let err = artifact
        .validate()
        .expect_err("unknown transition message should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition message id 1 is not accepted")
    );
}

#[test]
fn validate_rejects_transition_current_state_outside_state_table() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].current_state = Some(StateId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown transition current state should fail");

    assert!(
        err.to_string()
            .contains("process Worker message id 0 current_state id 99 is not a valid state value")
    );
}
