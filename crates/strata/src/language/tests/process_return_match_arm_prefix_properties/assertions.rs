fn checked_process<'a>(checked: &'a CheckedProgram, name: &str) -> &'a CheckedProcess {
    checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == name)
        .unwrap_or_else(|| panic!("checked process {name} should exist"))
}

fn checked_action_kinds(actions: &[CheckedAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            CheckedAction::Spawn { .. } => ActionKind::Spawn,
            CheckedAction::SpawnOutcome { .. } => ActionKind::SpawnOutcome,
            CheckedAction::Emit { .. } => ActionKind::Emit,
            CheckedAction::Send { .. } => ActionKind::Send,
            CheckedAction::SendOutcome { .. } => ActionKind::SendOutcome,
            CheckedAction::IfElse { .. } => ActionKind::IfElse,
            CheckedAction::ForEach { .. } => ActionKind::ForEach,
        })
        .collect()
}

fn artifact_action_kinds(actions: &[ArtifactAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            ArtifactAction::Spawn { .. } => ActionKind::Spawn,
            ArtifactAction::SpawnOutcome { .. } => ActionKind::SpawnOutcome,
            ArtifactAction::Emit { .. } => ActionKind::Emit,
            ArtifactAction::Send { .. } => ActionKind::Send,
            ArtifactAction::SendOutcome { .. } => ActionKind::SendOutcome,
            ArtifactAction::IfElse { .. } => ActionKind::IfElse,
            ArtifactAction::ForEach { .. } => ActionKind::ForEach,
        })
        .collect()
}

fn assert_artifact_send_actions_use_ids(actions: &[ArtifactAction]) {
    for action in actions {
        let ArtifactAction::Send {
            target, message, ..
        } = action
        else {
            continue;
        };
        assert_eq!(
            *target,
            ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            "arm-local send should lower through a process-ref id, not a source target name"
        );
        assert_eq!(
            *message,
            MessageId::new(0),
            "arm-local send should lower through a message id, not a source message name"
        );
    }
}

fn assert_nested_artifact_send_actions_use_ids(actions: &[ArtifactAction]) {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                assert_artifact_send_actions_use_ids(then_actions);
                assert_artifact_send_actions_use_ids(else_actions);
                assert_nested_artifact_send_actions_use_ids(then_actions);
                assert_nested_artifact_send_actions_use_ids(else_actions);
            }
            ArtifactAction::ForEach { body, .. } => {
                assert_artifact_send_actions_use_ids(body);
                assert_nested_artifact_send_actions_use_ids(body);
            }
            ArtifactAction::Spawn { .. }
            | ArtifactAction::SpawnOutcome { .. }
            | ArtifactAction::Emit { .. }
            | ArtifactAction::SendOutcome { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
}

fn effects_for_kind(kind: ArmPrefixKind) -> &'static [Effect] {
    match kind {
        ArmPrefixKind::None => &[Effect::Spawn],
        ArmPrefixKind::Emit => &[Effect::Emit, Effect::Spawn],
        ArmPrefixKind::Send => &[Effect::Spawn, Effect::Send],
        ArmPrefixKind::EmitThenSend => &[Effect::Emit, Effect::Spawn, Effect::Send],
    }
}

fn actions_for_kind(kind: ArmPrefixKind) -> Vec<ActionKind> {
    let mut actions = vec![ActionKind::Spawn];
    match kind {
        ArmPrefixKind::None => {}
        ArmPrefixKind::Emit => actions.push(ActionKind::Emit),
        ArmPrefixKind::Send => actions.push(ActionKind::Send),
        ArmPrefixKind::EmitThenSend => {
            actions.push(ActionKind::Emit);
            actions.push(ActionKind::Send);
        }
    }
    actions
}

fn union_effects(left: ArmPrefixKind, right: ArmPrefixKind) -> Vec<Effect> {
    let mut effects = vec![Effect::Spawn];
    if [left, right].iter().any(|kind| kind.uses_emit()) {
        effects.insert(0, Effect::Emit);
    }
    if [left, right].iter().any(|kind| kind.uses_send()) {
        effects.push(Effect::Send);
    }
    effects
}

fn effects_source(effects: &[Effect]) -> String {
    let effects = effects
        .iter()
        .map(|effect| match effect {
            Effect::Emit => "emit",
            Effect::Spawn => "spawn",
            Effect::Send => "send",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{effects}]")
}

fn artifact_effect_for(effect: Effect) -> ArtifactEffect {
    match effect {
        Effect::Emit => ArtifactEffect::Emit,
        Effect::Spawn => ArtifactEffect::Spawn,
        Effect::Send => ArtifactEffect::Send,
    }
}
