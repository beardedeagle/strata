fn checked_process<'a>(checked: &'a CheckedProgram, name: &str) -> &'a CheckedProcess {
    checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == name)
        .unwrap_or_else(|| panic!("checked process {name} should exist"))
}

fn artifact_process<'a>(artifact: &'a MantleArtifact, name: &str) -> &'a ArtifactProcess {
    artifact
        .processes
        .iter()
        .find(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"))
}

fn artifact_process_id(artifact: &MantleArtifact, name: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}

fn artifact_process_mut<'a>(
    artifact: &'a mut MantleArtifact,
    name: &str,
) -> &'a mut ArtifactProcess {
    artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"))
}

fn checked_state_id_by_label(process: &CheckedProcess, label: &str) -> CheckedStateId {
    let index = process
        .state_values()
        .iter()
        .position(|state| state.label() == label)
        .unwrap_or_else(|| panic!("checked state {label} should exist"));
    CheckedStateId::from_index(index).expect("checked state index should fit")
}

fn artifact_state_id_by_label(process: &ArtifactProcess, label: &str) -> StateId {
    let index = process
        .state_values
        .iter()
        .position(|state| state.label == label)
        .unwrap_or_else(|| panic!("artifact state {label} should exist"));
    StateId::from_index(index).expect("artifact state index should fit")
}

fn artifact_message_id_by_label(process: &ArtifactProcess, label: &str) -> MessageId {
    let index = process
        .message_variants
        .iter()
        .position(|message| message.label == label)
        .unwrap_or_else(|| panic!("artifact message {label} should exist"));
    MessageId::from_index(index).expect("artifact message index should fit")
}

fn checked_selected_arm(transition: &CheckedTransition) -> SelectedArm {
    selected_arm_from_label(
        transition
            .payload_guard()
            .expect("selected return-match transition should have a payload guard")
            .label(),
    )
}

fn artifact_selected_arm(transition: &ArtifactTransition) -> SelectedArm {
    selected_arm_from_label(
        &transition
            .payload_guard
            .as_ref()
            .expect("selected artifact transition should have a payload guard")
            .label(),
    )
}

fn selected_arm_from_label(label: &str) -> SelectedArm {
    if label.contains("Ready") {
        SelectedArm::Ready
    } else if label.contains("Done") {
        SelectedArm::Done
    } else {
        panic!("payload guard label should identify selected arm: {label}");
    }
}

fn assert_checked_terminal(
    process: &CheckedProcess,
    transition: &CheckedTransition,
    terminal_profile: TerminalProfile,
) {
    let terminal = terminal_profile.for_arm(checked_selected_arm(transition));
    assert_eq!(transition.step_result(), terminal.checked_step_result());
    assert_eq!(
        transition.next_state(),
        CheckedNextState::Value(checked_state_id_by_label(process, terminal.state_label()))
    );
}

fn assert_artifact_terminal(
    process: &ArtifactProcess,
    transition: &ArtifactTransition,
    terminal_profile: TerminalProfile,
) {
    let terminal = terminal_profile.for_arm(artifact_selected_arm(transition));
    assert_eq!(transition.step_result, terminal.artifact_step_result());
    assert_eq!(
        &transition.next_state,
        &NextState::Value(artifact_state_id_by_label(process, terminal.state_label()))
    );
}

fn checked_action_shapes(actions: &[CheckedAction]) -> Vec<ActionShape> {
    actions
        .iter()
        .map(|action| match action {
            CheckedAction::Emit { .. } => ActionShape::Emit,
            CheckedAction::Spawn { .. } => ActionShape::Spawn,
            CheckedAction::Send { .. } => ActionShape::Send,
            CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => ActionShape::IfElse {
                then_actions: checked_action_shapes(then_actions),
                else_actions: checked_action_shapes(else_actions),
            },
            CheckedAction::ForEach { body, .. } => ActionShape::ForEach {
                body: checked_action_shapes(body),
            },
        })
        .collect()
}

fn artifact_action_shapes(actions: &[ArtifactAction]) -> Vec<ActionShape> {
    actions
        .iter()
        .map(|action| match action {
            ArtifactAction::Emit { .. } => ActionShape::Emit,
            ArtifactAction::Spawn { .. } => ActionShape::Spawn,
            ArtifactAction::Send { .. } => ActionShape::Send,
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => ActionShape::IfElse {
                then_actions: artifact_action_shapes(then_actions),
                else_actions: artifact_action_shapes(else_actions),
            },
            ArtifactAction::ForEach { body, .. } => ActionShape::ForEach {
                body: artifact_action_shapes(body),
            },
        })
        .collect()
}

fn assert_nested_artifact_send_actions_use_ids(
    artifact: &MantleArtifact,
    owner_process: &ArtifactProcess,
    actions: &[ArtifactAction],
) {
    let sink_process = artifact_process(artifact, "Sink");
    let sink_id = artifact_process_id(artifact, "Sink");
    let ack_id = artifact_message_id_by_label(sink_process, "Ack");

    for action in actions {
        match action {
            ArtifactAction::Send {
                target, message, ..
            } => {
                let ArtifactSendTarget::ProcessRef(process_ref) = target else {
                    panic!("selected-arm send should lower through a typed process-ref id");
                };
                let resolved = owner_process
                    .process_refs
                    .get(process_ref.index())
                    .unwrap_or_else(|| panic!("process ref id {process_ref:?} should resolve"));
                assert_eq!(
                    resolved.target, sink_id,
                    "selected-arm send should resolve typed process-ref id to Sink"
                );
                assert_eq!(
                    *message, ack_id,
                    "selected-arm send should use typed Ack id"
                );
            }
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, then_actions);
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, else_actions);
            }
            ArtifactAction::ForEach { body, .. } => {
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, body);
            }
            ArtifactAction::Emit { .. } | ArtifactAction::Spawn { .. } => {}
        }
    }
}

fn assert_no_source_binding_dispatch(artifact: &MantleArtifact) {
    for process in &artifact.processes {
        for transition in &process.transitions {
            if let Some(payload) = &transition.payload_guard {
                assert_artifact_payload_has_no_source_bindings(payload);
            }
            assert_actions_have_no_source_bindings(&transition.actions);
            assert_next_state_has_no_source_bindings(&transition.next_state);
        }
    }
}

fn assert_no_encoded_source_binding_leak(artifact: &MantleArtifact) {
    let encoded = artifact.encode();
    for name in SOURCE_ONLY_BINDINGS {
        assert!(
            !encoded.lines().any(|line| line.contains(name)),
            "artifact must not lower source binding name {name} as executable dispatch"
        );
    }
}

fn assert_actions_have_no_source_bindings(actions: &[ArtifactAction]) {
    for action in actions {
        match action {
            ArtifactAction::Emit { .. } | ArtifactAction::Spawn { .. } => {}
            ArtifactAction::Send { payload, .. } => {
                if let Some(payload) = payload {
                    assert_template_has_no_source_bindings(payload);
                }
            }
            ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                assert_template_has_no_source_bindings(condition);
                assert_actions_have_no_source_bindings(then_actions);
                assert_actions_have_no_source_bindings(else_actions);
            }
            ArtifactAction::ForEach {
                collection, body, ..
            } => {
                assert_template_has_no_source_bindings(collection);
                assert_actions_have_no_source_bindings(body);
            }
        }
    }
}

fn assert_next_state_has_no_source_bindings(next_state: &NextState) {
    match next_state {
        NextState::Current | NextState::Value(_) => {}
        NextState::Template(template) => assert_template_has_no_source_bindings(template),
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            assert_template_has_no_source_bindings(condition);
            assert_next_state_has_no_source_bindings(then_state);
            assert_next_state_has_no_source_bindings(else_state);
        }
    }
}

fn assert_template_has_no_source_bindings(template: &ArtifactValueTemplate) {
    match template {
        ArtifactValueTemplate::Literal { value, .. } => {
            assert_value_has_no_source_bindings(value);
        }
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::LoopElement { .. } => {}
        ArtifactValueTemplate::EnumPayload { value, .. }
        | ArtifactValueTemplate::ListElement { list: value, .. }
        | ArtifactValueTemplate::ListPrefixElement { list: value, .. }
        | ArtifactValueTemplate::ListRest { list: value, .. }
        | ArtifactValueTemplate::MapRest { map: value, .. }
        | ArtifactValueTemplate::BooleanNot { operand: value, .. }
        | ArtifactValueTemplate::EnumVariant { payload: value, .. } => {
            assert_template_has_no_source_bindings(value);
        }
        ArtifactValueTemplate::RecordField { record, field, .. } => {
            assert_no_source_binding_string(field);
            assert_template_has_no_source_bindings(record);
        }
        ArtifactValueTemplate::MapValue { map, key, keys, .. } => {
            assert_template_has_no_source_bindings(map);
            assert_value_has_no_source_bindings(key);
            for key in keys {
                assert_value_has_no_source_bindings(key);
            }
        }
        ArtifactValueTemplate::Record { fields, .. } => {
            for field in fields {
                assert_no_source_binding_string(&field.name);
                assert_template_has_no_source_bindings(&field.value);
            }
        }
        ArtifactValueTemplate::List { items, .. } => {
            for item in items {
                assert_template_has_no_source_bindings(item);
            }
        }
        ArtifactValueTemplate::Map { entries, .. } => {
            for entry in entries {
                assert_template_has_no_source_bindings(&entry.key);
                assert_template_has_no_source_bindings(&entry.value);
            }
        }
        ArtifactValueTemplate::Equality { left, right, .. }
        | ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
            assert_template_has_no_source_bindings(left);
            assert_template_has_no_source_bindings(right);
        }
    }
}

fn assert_artifact_payload_has_no_source_bindings(payload: &ArtifactPayload) {
    assert_value_has_no_source_bindings(&payload.value);
}

fn assert_value_has_no_source_bindings(value: &ArtifactValue) {
    match value {
        ArtifactValue::Atom(value) => assert_no_source_binding_string(value),
        ArtifactValue::EnumVariant { variant, payload } => {
            assert_no_source_binding_string(variant);
            assert_value_has_no_source_bindings(payload);
        }
        ArtifactValue::Record {
            constructor,
            fields,
        } => {
            assert_no_source_binding_string(constructor);
            for field in fields {
                assert_no_source_binding_string(&field.name);
                assert_value_has_no_source_bindings(&field.value);
            }
        }
        ArtifactValue::List(items) => {
            for item in items {
                assert_value_has_no_source_bindings(item);
            }
        }
        ArtifactValue::Map(entries) => {
            for entry in entries {
                assert_value_has_no_source_bindings(&entry.key);
                assert_value_has_no_source_bindings(&entry.value);
            }
        }
        ArtifactValue::ProcessRef { .. } => {}
    }
}

fn assert_no_source_binding_string(value: &str) {
    for name in SOURCE_ONLY_BINDINGS {
        assert!(
            !value.contains(name),
            "artifact must not lower source binding name {name} as executable dispatch"
        );
    }
}

