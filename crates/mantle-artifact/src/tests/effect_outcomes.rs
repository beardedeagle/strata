use super::support::*;
use crate::{MAX_EFFECT_OUTCOMES_PER_TRANSITION, MAX_VALUE_TEMPLATE_DEPTH};

#[test]
fn validate_accepts_typed_send_outcome_used_as_next_state_template() {
    outcome_artifact()
        .validate()
        .expect("typed send outcome artifact should validate");
}

#[test]
fn validate_rejects_unbound_effect_outcome_template() {
    let mut artifact = outcome_artifact();
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EffectOutcome {
            ty: TypeId::new(12),
            outcome: EffectOutcomeId::new(1),
        });

    let err = artifact
        .validate()
        .expect_err("unbound effect outcome template should fail");

    assert!(
        err.to_string()
            .contains("next_state template references unbound effect outcome id 1"),
        "{err}"
    );
}

#[test]
fn validate_rejects_send_outcome_type_that_does_not_preserve_message_type() {
    let mut artifact = outcome_artifact();
    artifact.types[TypeId::new(11).index()] = send_error_type(UnitAndOutcomeTypes::UNIT);

    let err = artifact
        .validate()
        .expect_err("malformed SendError payload type should fail");

    assert!(
        err.to_string()
            .contains("send outcome error type variant Full must preserve payload type id 3"),
        "{err}"
    );
}

#[test]
fn validate_rejects_send_outcome_type_without_mailbox_closed_variant() {
    let mut artifact = outcome_artifact();
    artifact.types[TypeId::new(11).index()] =
        send_error_type_with_labels(WORKER_MSG, &["Full", "Stopped", "Crashed"]);

    let err = artifact
        .validate()
        .expect_err("SendError without MailboxClosed should fail");

    assert!(
        err.to_string()
            .contains("send outcome error type type id 11 has 3 variants, expected 4"),
        "{err}"
    );
}

#[test]
fn validate_rejects_effect_outcome_template_type_mismatch() {
    let mut artifact = outcome_artifact();
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EffectOutcome {
            ty: UnitAndOutcomeTypes::UNIT,
            outcome: EffectOutcomeId::new(0),
        });

    let err = artifact
        .validate()
        .expect_err("effect outcome template type mismatch should fail");

    assert!(
        err.to_string()
            .contains("next_state_template has type id 10, expected 12"),
        "{err}"
    );
}

#[test]
fn validate_rejects_effect_outcome_id_outside_transition_limit() {
    let mut artifact = outcome_artifact();
    let ArtifactAction::SendOutcome { outcome, .. } =
        &mut artifact.processes[0].transitions[0].actions[1]
    else {
        panic!("test artifact action should be send outcome");
    };
    *outcome = EffectOutcomeId::from_index(MAX_EFFECT_OUTCOMES_PER_TRANSITION)
        .expect("limit boundary id should fit");

    let err = artifact
        .validate()
        .expect_err("out-of-range effect outcome id should fail");

    let expected = format!(
        "effect outcome id {MAX_EFFECT_OUTCOMES_PER_TRANSITION} must be less than {MAX_EFFECT_OUTCOMES_PER_TRANSITION}"
    );
    assert!(err.to_string().contains(&expected), "{err}");
}

#[test]
fn validate_rejects_effect_outcome_after_ordinary_action() {
    let mut artifact = outcome_artifact();
    let transition = &mut artifact.processes[0].transitions[0];
    transition.effects = vec![
        ArtifactEffect::Spawn,
        ArtifactEffect::Emit,
        ArtifactEffect::Send,
    ];
    transition.actions.insert(
        1,
        ArtifactAction::Emit {
            output: OutputId::new(0),
        },
    );

    let err = artifact
        .validate()
        .expect_err("effect outcome after ordinary effect should fail");

    assert!(
        err.to_string()
            .contains("effect outcome id 0 appears after ordinary effects"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_outcome_targeting_entry_process() {
    let mut artifact = spawn_outcome_artifact();
    let ArtifactAction::SpawnOutcome { target, .. } =
        &mut artifact.processes[0].transitions[0].actions[0]
    else {
        panic!("test artifact action should be spawn outcome");
    };
    *target = ProcessId::new(0);

    let err = artifact
        .validate()
        .expect_err("spawn outcome targeting entry process should fail");

    assert!(
        err.to_string()
            .contains("spawn outcome targets entry process id 0"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_outcome_targeting_self() {
    let mut artifact = spawn_outcome_artifact();
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SpawnOutcomeTypes::SPAWN_RESULT,
        target: ProcessId::new(1),
        spawn_site: SPAWN_WORKER_SITE,
    }];

    let err = artifact
        .validate()
        .expect_err("spawn outcome targeting self should fail");

    assert!(
        err.to_string()
            .contains("spawn outcome targets itself, which is not supported"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_outcome_type_without_process_ref_success() {
    let mut artifact = spawn_outcome_artifact();
    artifact.types[SpawnOutcomeTypes::SPAWN_RESULT.index()] =
        result_type(UnitAndOutcomeTypes::UNIT, TypeId::new(11));

    let err = artifact
        .validate()
        .expect_err("spawn outcome success must be process reference");

    assert!(
        err.to_string()
            .contains("spawn outcome success type type id 10 must be a process reference type"),
        "{err}"
    );
}

#[test]
fn validate_rejects_process_ref_spawn_outcome_as_state_template() {
    let mut artifact = spawn_outcome_artifact();
    artifact.processes[0].state_type = SpawnOutcomeTypes::SPAWN_RESULT;
    artifact.processes[0].state_values =
        state_values(SpawnOutcomeTypes::SPAWN_RESULT, &["Err(Exhausted(Unit))"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EffectOutcome {
            ty: SpawnOutcomeTypes::SPAWN_RESULT,
            outcome: EffectOutcomeId::new(0),
        });

    let err = artifact
        .validate()
        .expect_err("process reference spawn outcome must not become process state");

    assert!(
        err.to_string()
            .contains("process reference outcome must remain step-local"),
        "{err}"
    );
}

#[test]
fn validate_accepts_spawn_outcome_variant_branch_without_process_ref_equality() {
    let mut artifact = spawn_outcome_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].effects =
        vec![ArtifactEffect::Spawn, ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::SpawnOutcome {
            outcome: EffectOutcomeId::new(0),
            outcome_ty: SpawnOutcomeTypes::SPAWN_RESULT,
            target: ProcessId::new(1),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: SpawnOutcomeTypes::SPAWN_RESULT,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: SpawnOutcomeTypes::SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: SpawnOutcomeTypes::SPAWN_RESULT,
                    value: artifact_value("Err(Exhausted(Unit))"),
                }),
            },
            then_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: Vec::new(),
        },
    ];

    artifact
        .validate()
        .expect("spawn outcome branch by error variant should validate");
}

#[test]
fn validate_rejects_process_ref_spawn_outcome_structural_equality() {
    let mut artifact = spawn_outcome_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: SpawnOutcomeTypes::SPAWN_RESULT,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: SpawnOutcomeTypes::SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: SpawnOutcomeTypes::SPAWN_RESULT,
                    outcome: EffectOutcomeId::new(0),
                }),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    let err = artifact
        .validate()
        .expect_err("spawn outcome structural equality should fail admission");

    assert!(
        err.to_string().contains(
            "built-in payload enum requires one operand to be a safe built-in variant pattern"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_send_outcome_structural_equality() {
    let mut artifact = outcome_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let send_result = artifact.processes[0].state_type;
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: send_result,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: send_result,
                    outcome: EffectOutcomeId::new(0),
                }),
                right: Box::new(ArtifactValueTemplate::EffectOutcome {
                    ty: send_result,
                    outcome: EffectOutcomeId::new(0),
                }),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    let err = artifact
        .validate()
        .expect_err("send outcome structural equality should fail admission");

    assert!(
        err.to_string().contains(
            "built-in payload enum requires one operand to be a safe built-in variant pattern"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_nested_builtin_equality_pattern_past_depth_limit() {
    let mut artifact = outcome_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let mut payload_ty = UnitAndOutcomeTypes::UNIT;
    let mut option_types = Vec::new();
    for _ in 0..=MAX_VALUE_TEMPLATE_DEPTH {
        let option_ty = push_type(&mut artifact, option_type(payload_ty));
        option_types.push(option_ty);
        payload_ty = option_ty;
    }
    let operand_ty = payload_ty;
    artifact.processes[0].transitions[0]
        .actions
        .push(ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(nested_some_template(
                    &option_types,
                    UnitAndOutcomeTypes::UNIT,
                )),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: operand_ty,
                    value: artifact_value("None"),
                }),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        });

    let err = artifact
        .validate()
        .expect_err("nested equality pattern beyond depth limit should fail admission");

    let expected = format!(
        "equality payload.operand_type_id nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
    );
    assert!(err.to_string().contains(&expected), "{err}");
}

#[test]
fn validate_rejects_message_type_variant_label_mismatch() {
    let mut artifact = outcome_artifact();
    artifact.types[WORKER_MSG.index()] =
        ArtifactType::enum_value("WorkerMsg", vec!["Pong".to_string()]);

    let err = artifact
        .validate()
        .expect_err("message type label mismatch should fail");

    assert!(
        err.to_string()
            .contains("variant 0 label Pong does not match message label Ping"),
        "{err}"
    );
}

#[test]
fn validate_rejects_message_type_variant_payload_mismatch() {
    let mut artifact = outcome_artifact();
    artifact.types[WORKER_MSG.index()] = ArtifactType::enum_value_with_payloads(
        "WorkerMsg",
        vec![ArtifactEnumVariant {
            label: "Ping".to_string(),
            payload_type: Some(UnitAndOutcomeTypes::UNIT),
        }],
    );

    let err = artifact
        .validate()
        .expect_err("message type payload mismatch should fail");

    assert!(
        err.to_string()
            .contains("variant 0 payload type Some(10), expected None"),
        "{err}"
    );
}

fn outcome_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    let types = UnitAndOutcomeTypes::push(&mut artifact, WORKER_MSG);
    artifact.types[WORKER_MSG.index()] = ArtifactType::enum_value("WorkerMsg", vec!["Ping".into()]);
    artifact.processes[0].state_type = types.send_result;
    artifact.processes[0].state_values = state_values(types.send_result, &["Ok(Unit)"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::EffectOutcome {
            ty: types.send_result,
            outcome: EffectOutcomeId::new(0),
        });
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::SendOutcome {
            outcome: EffectOutcomeId::new(0),
            outcome_ty: types.send_result,
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            port: None,
            message: MessageId::new(0),
            payload: None,
        },
    ];
    artifact
}

fn spawn_outcome_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    let types = SpawnOutcomeTypes::push(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].next_state = NextState::Current;
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::SpawnOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: types.spawn_result,
        target: ProcessId::new(1),
        spawn_site: SPAWN_WORKER_SITE,
    }];
    artifact
}

struct UnitAndOutcomeTypes {
    send_result: TypeId,
}

impl UnitAndOutcomeTypes {
    const UNIT: TypeId = TypeId::new(10);

    fn push(artifact: &mut MantleArtifact, message_ty: TypeId) -> Self {
        let unit = push_type(artifact, ArtifactType::value("Unit"));
        assert_eq!(unit, Self::UNIT);
        let send_error = push_type(artifact, send_error_type(message_ty));
        let send_result = push_type(artifact, result_type(unit, send_error));
        Self { send_result }
    }
}

struct SpawnOutcomeTypes {
    spawn_result: TypeId,
}

impl SpawnOutcomeTypes {
    const SPAWN_RESULT: TypeId = TypeId::new(12);

    fn push(artifact: &mut MantleArtifact) -> Self {
        let unit = push_type(artifact, ArtifactType::value("Unit"));
        assert_eq!(unit, UnitAndOutcomeTypes::UNIT);
        let spawn_error = push_type(artifact, spawn_error_type(unit));
        let spawn_result = push_type(artifact, result_type(PROCESS_REF_WORKER, spawn_error));
        assert_eq!(spawn_result, Self::SPAWN_RESULT);
        Self { spawn_result }
    }
}

fn push_type(artifact: &mut MantleArtifact, ty: ArtifactType) -> TypeId {
    let id = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ty);
    id
}

fn result_type(ok: TypeId, err: TypeId) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "Result",
        vec![
            ArtifactEnumVariant {
                label: "Ok".to_string(),
                payload_type: Some(ok),
            },
            ArtifactEnumVariant {
                label: "Err".to_string(),
                payload_type: Some(err),
            },
        ],
    )
}

fn option_type(payload: TypeId) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "Option",
        vec![
            ArtifactEnumVariant {
                label: "None".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Some".to_string(),
                payload_type: Some(payload),
            },
        ],
    )
}

fn nested_some_template(option_types: &[TypeId], unit_ty: TypeId) -> ArtifactValueTemplate {
    option_types.iter().copied().fold(
        ArtifactValueTemplate::Literal {
            ty: unit_ty,
            value: artifact_value("Unit"),
        },
        |payload, ty| ArtifactValueTemplate::EnumVariant {
            ty,
            variant: EnumVariantId::new(1),
            payload: Box::new(payload),
        },
    )
}

fn send_error_type(message_ty: TypeId) -> ArtifactType {
    send_error_type_with_labels(message_ty, &["Full", "Stopped", "Crashed", "MailboxClosed"])
}

fn send_error_type_with_labels(message_ty: TypeId, labels: &[&str]) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SendError",
        labels
            .iter()
            .map(|label| ArtifactEnumVariant {
                label: (*label).to_string(),
                payload_type: Some(message_ty),
            })
            .collect(),
    )
}

fn spawn_error_type(unit_ty: TypeId) -> ArtifactType {
    ArtifactType::enum_value_with_payloads(
        "SpawnError",
        ["Denied", "Exhausted", "BackendUnavailable"]
            .into_iter()
            .map(|label| ArtifactEnumVariant {
                label: label.to_string(),
                payload_type: Some(unit_ty),
            })
            .collect(),
    )
}
