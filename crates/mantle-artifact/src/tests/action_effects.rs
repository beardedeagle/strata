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
fn validate_rejects_nested_if_else_action_without_declared_effect() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].effects = Vec::new();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
        else_actions: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("nested emit without declared emit effect should fail");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 uses effect emit but does not declare it")
    );
}

#[test]
fn validate_rejects_if_else_action_nesting_above_limit() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].actions = vec![nested_if_else_action(
        MAX_VALUE_TEMPLATE_DEPTH + 1,
        bool_type,
    )];

    let err = artifact
        .validate()
        .expect_err("overly nested if_else action should fail");

    assert!(err.to_string().contains(&format!(
        "artifact action nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
    )));
}

#[test]
fn validate_rejects_send_after_process_ref_spawned_in_only_one_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            },
            then_actions: vec![ArtifactAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }],
            else_actions: Vec::new(),
        },
        ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: None,
        },
    ];

    let err = artifact
        .validate()
        .expect_err("branch-local process reference should not be available after if");

    assert!(err.to_string().contains(
        "process Main sends through unbound process reference id 0 within message transition 0"
    ));
}

#[test]
fn validate_accepts_send_after_process_ref_spawned_in_both_branches() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            },
            then_actions: vec![ArtifactAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }],
            else_actions: vec![ArtifactAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }],
        },
        ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: None,
        },
    ];

    artifact
        .validate()
        .expect("process reference spawned in both branches should be available after if");
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
