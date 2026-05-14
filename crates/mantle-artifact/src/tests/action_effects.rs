use super::support::*;

#[test]
fn validate_rejects_aggregate_process_action_count_above_limit() {
    let mut artifact = valid_artifact();
    artifact.processes[1]
        .message_variants
        .push(ArtifactMessageVariant::unit("Pong"));
    artifact.processes[1].transitions[0].actions = emit_actions(MAX_ACTIONS_PER_PROCESS / 2);
    artifact.processes[1].transitions.push(ArtifactTransition {
        current_state: None,
        message: MessageId::new(1),
        payload_guard: None,
        step_result: StepResult::Stop,
        next_state: NextState::Current,
        effects: vec![ArtifactEffect::Emit],
        actions: emit_actions((MAX_ACTIONS_PER_PROCESS / 2) + 1),
    });

    let err = artifact
        .validate()
        .expect_err("aggregate process action count should be bounded");

    assert!(err.to_string().contains(&format!(
        "action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
}

#[test]
fn validate_rejects_action_without_declared_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];

    let err = artifact
        .validate()
        .expect_err("send without declared send effect should fail");

    assert!(
        err.to_string()
            .contains("process Main transition 0 uses effect send but does not declare it")
    );
}

#[test]
fn validate_rejects_declared_effect_without_action() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0]
        .effects
        .push(ArtifactEffect::Send);

    let err = artifact
        .validate()
        .expect_err("unused declared effect should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 declares effect send but no action uses it")
    );
}

#[test]
fn validate_rejects_duplicate_transition_effect() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Emit, ArtifactEffect::Emit];

    let err = artifact
        .validate()
        .expect_err("duplicate transition effect should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 declares duplicate effect emit")
    );
}

#[test]
fn validate_rejects_unknown_send_message() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(1),
            payload: None,
        });

    let err = artifact
        .validate()
        .expect_err("unknown send message should fail");

    assert!(
        err.to_string()
            .contains("sends message id 1 not accepted by process id 1")
    );
}

#[test]
fn validate_rejects_unknown_send_process_ref() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(99)),
            message: MessageId::new(0),
            payload: None,
        });

    let err = artifact
        .validate()
        .expect_err("unknown send process ref should fail");

    assert!(
        err.to_string()
            .contains("references undefined process reference id 99")
    );
}

#[test]
fn validate_rejects_unknown_spawn_target() {
    let mut artifact = valid_artifact();
    artifact.processes[0].process_refs[0].target = ProcessId::new(99);

    let err = artifact
        .validate()
        .expect_err("unknown spawn target should fail");

    assert!(
        err.to_string()
            .contains("process reference worker targets undefined process id 99")
    );
}

#[test]
fn validate_rejects_unknown_output_id() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::Emit {
        output: OutputId::new(99),
    }];

    let err = artifact
        .validate()
        .expect_err("unknown output id should fail");

    assert!(err.to_string().contains("emits undefined output id 99"));
}
