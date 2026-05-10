use super::*;

impl ArtifactProcess {
    pub(super) fn validate_identity(&self, artifact: &MantleArtifact) -> Result<()> {
        validate_ident_field("process debug_name", &self.debug_name)?;
        artifact.validate_value_type("state_type", self.state_type)?;
        artifact.validate_value_type("message_type", self.message_type)?;
        validate_count("mailbox_bound", self.mailbox_bound, 1, MAX_MAILBOX_BOUND)?;
        validate_count(
            "state_value_count",
            self.state_values.len(),
            1,
            MAX_STATE_VALUES_PER_PROCESS,
        )?;
        validate_count(
            "message_count",
            self.message_variants.len(),
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        validate_count(
            "process_ref_count",
            self.process_refs.len(),
            0,
            MAX_PROCESS_REFS_PER_PROCESS,
        )?;
        validate_count(
            "transition_count",
            self.transitions.len(),
            1,
            MAX_TRANSITIONS_PER_PROCESS,
        )?;
        validate_unique_state_value_list(&self.state_values)?;
        for state_value in &self.state_values {
            artifact.validate_value_type("state value type", state_value.ty)?;
            if state_value.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} state value {} (label {}) has type id {}, expected {}",
                    self.debug_name,
                    state_value.value,
                    state_value.label,
                    state_value.ty.as_u32(),
                    self.state_type.as_u32()
                )));
            }
            if let Some(payload) = &state_value.payload {
                artifact.validate_value_type("state value payload type", payload.ty)?;
                validate_value_label("state value payload", &payload.value)?;
                if payload.process_ref.is_some() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state_value.label
                    )));
                }
            }
        }
        validate_unique_message_variant_list(&self.message_variants)?;
        for message in &self.message_variants {
            if let Some(payload_type) = message.payload_type {
                artifact.type_entry(payload_type).map_err(|err| {
                    Error::new(format!(
                        "process {} message {} payload_type_id {} is invalid: {err}",
                        self.debug_name,
                        message.label,
                        payload_type.as_u32()
                    ))
                })?;
            }
        }
        validate_unique_process_ref_list(&self.process_refs)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        let mut transition_keys = BTreeSet::new();
        let mut action_count = 0usize;
        for transition in &self.transitions {
            let transition_key = (
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32),
            );
            if !transition_keys.insert(transition_key) {
                return Err(Error::new(format!(
                    "process {} declares duplicate transition for message id {} current_state {:?}",
                    self.debug_name,
                    transition.message.as_u32(),
                    transition.current_state.map(StateId::as_u32)
                )));
            }
            if transition.message.index() >= self.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not accepted",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            let current_state_payload_type =
                self.transition_current_state_payload_type(transition)?;
            let transition_context = transition.transition_context();
            match &transition.next_state {
                NextState::Current => {}
                NextState::Value(state) => {
                    if state.index() >= self.state_values.len() {
                        return Err(Error::new(format!(
                            "process {} {} next_state id {} is not a valid state value",
                            self.debug_name,
                            transition_context,
                            state.as_u32()
                        )));
                    }
                }
                NextState::Template(template) => {
                    let received_payload_type = self
                        .message_variants
                        .get(transition.message.index())
                        .and_then(|message| message.payload_type);
                    template.validate_for_received_payload(
                        artifact,
                        &format!(
                            "process {} {} next_state_template",
                            self.debug_name, transition_context
                        ),
                        Some(self.state_type),
                        received_payload_type,
                        current_state_payload_type,
                        0,
                    )?;
                    self.validate_static_next_state_template_value(artifact, transition, template)?;
                }
            }
            action_count = action_count
                .checked_add(transition.actions.len())
                .ok_or_else(|| Error::new("process action_count overflowed"))?;
        }
        validate_count("action_count", action_count, 0, MAX_ACTIONS_PER_PROCESS)?;
        self.validate_transition_coverage(&transition_keys)?;
        Ok(())
    }

    fn validate_transition_coverage(
        &self,
        transition_keys: &BTreeSet<(u32, Option<u32>)>,
    ) -> Result<()> {
        for message_index in 0..self.message_variants.len() {
            let message = message_index as u32;
            let has_unguarded = transition_keys.contains(&(message, None));
            let has_guarded = (0..self.state_values.len())
                .any(|state_index| transition_keys.contains(&(message, Some(state_index as u32))));
            if has_unguarded {
                if has_guarded {
                    return Err(Error::new(format!(
                        "process {} mixes unguarded and state-specific transitions for message id {}",
                        self.debug_name, message
                    )));
                }
                continue;
            }
            if !has_guarded {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {}",
                    self.debug_name, message
                )));
            }
            for state_index in 0..self.state_values.len() {
                if !transition_keys.contains(&(message, Some(state_index as u32))) {
                    return Err(Error::new(format!(
                        "process {} has no transition for message id {} current_state id {}",
                        self.debug_name, message, state_index
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_static_next_state_template_value(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        template: &ArtifactValueTemplate,
    ) -> Result<()> {
        if template.depends_on_received_payload() {
            return Ok(());
        }
        let current_state_payload = transition
            .current_state
            .and_then(|state| self.state_values.get(state.index()))
            .and_then(|state| state.payload.as_ref());
        let value = artifact.evaluate_state_value_with_current_state(
            template,
            None,
            current_state_payload,
        )?;
        if self
            .state_values
            .iter()
            .any(|state_value| state_value.has_same_identity(&value))
        {
            return Ok(());
        }
        Err(Error::new(format!(
            "process {} {} next_state_template produced value {} not admitted by state table",
            self.debug_name,
            transition.transition_context(),
            value.label
        )))
    }

    fn transition_current_state_payload_type(
        &self,
        transition: &ArtifactTransition,
    ) -> Result<Option<TypeId>> {
        let Some(current_state) = transition.current_state else {
            return Ok(None);
        };
        let state_value = self
            .state_values
            .get(current_state.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} current_state id {} is not a valid state value",
                    self.debug_name,
                    transition.message.as_u32(),
                    current_state.as_u32()
                ))
            })?;
        Ok(state_value.payload.as_ref().map(|payload| payload.ty))
    }

    pub(super) fn validate_references(
        &self,
        artifact: &MantleArtifact,
        process_id: ProcessId,
    ) -> Result<()> {
        for process_ref in &self.process_refs {
            if process_ref.target.index() >= artifact.processes.len() {
                return Err(Error::new(format!(
                    "process {} process reference {} targets undefined process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == artifact.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference {} targets entry process id {}",
                    self.debug_name,
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference {} targets itself, which is not supported",
                    self.debug_name, process_ref.debug_name
                )));
            }
        }
        for transition in &self.transitions {
            let declared_effects = transition.validate_effects(&self.debug_name)?;
            let mut spawned_refs = BTreeSet::new();
            let mut used_effects = BTreeSet::new();
            for action in &transition.actions {
                let action_effect = action.effect();
                if !declared_effects.contains(&action_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} uses effect {action_effect} but does not declare it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
                used_effects.insert(action_effect);
                self.validate_action_reference(artifact, transition, &mut spawned_refs, action)?;
            }
            for declared_effect in &declared_effects {
                if !used_effects.contains(declared_effect) {
                    return Err(Error::new(format!(
                        "process {} transition {} declares effect {declared_effect} but no action uses it",
                        self.debug_name,
                        transition.message.as_u32()
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_action_reference(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        spawned_refs: &mut BTreeSet<ProcessRefId>,
        action: &ArtifactAction,
    ) -> Result<()> {
        match action {
            ArtifactAction::Emit { output } => {
                if output.index() >= artifact.outputs.len() {
                    return Err(Error::new(format!(
                        "process {} emits undefined output id {}",
                        self.debug_name,
                        output.as_u32()
                    )));
                }
            }
            ArtifactAction::Spawn {
                target,
                process_ref,
            } => {
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn process reference id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_refs.insert(*process_ref) {
                    return Err(Error::new(format!(
                        "process {} duplicates process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
            }
            ArtifactAction::Send {
                target,
                message,
                payload,
            } => {
                let target_process_id = self.validate_send_target_reference(
                    artifact,
                    target,
                    transition,
                    spawned_refs,
                )?;
                let target_process = artifact
                    .processes
                    .get(target_process_id.index())
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} sends to undefined process id {}",
                            self.debug_name,
                            target_process_id.as_u32()
                        ))
                    })?;
                if message.index() >= target_process.message_variants.len() {
                    return Err(Error::new(format!(
                        "process {} sends message id {} not accepted by process id {}",
                        self.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32()
                    )));
                }
                let target_message = &target_process.message_variants[message.index()];
                match (&target_message.payload_type, payload) {
                    (None, None) => {}
                    (None, Some(_)) => {
                        return Err(Error::new(format!(
                            "process {} sends payload to process id {} message id {}, which does not accept one",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(_), None) => {
                        return Err(Error::new(format!(
                            "process {} sends process id {} message id {} without required payload",
                            self.debug_name,
                            target_process_id.as_u32(),
                            message.as_u32()
                        )));
                    }
                    (Some(payload_type), Some(payload)) => {
                        self.validate_template_process_refs(artifact, payload, spawned_refs)?;
                        let received_payload_type = self
                            .message_variants
                            .get(transition.message.index())
                            .and_then(|message| message.payload_type);
                        let current_state_payload_type =
                            self.transition_current_state_payload_type(transition)?;
                        payload.validate_for_received_payload(
                            artifact,
                            &format!(
                                "process {} transition {} send payload",
                                self.debug_name,
                                transition.message.as_u32()
                            ),
                            Some(*payload_type),
                            received_payload_type,
                            current_state_payload_type,
                            0,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_send_target_reference(
        &self,
        artifact: &MantleArtifact,
        target: &ArtifactSendTarget,
        transition: &ArtifactTransition,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<ProcessId> {
        match target {
            ArtifactSendTarget::ProcessRef(process_ref) => {
                let target_process_id = self.process_ref_target(*process_ref)?;
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends through unbound process reference id {} within message transition {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        transition.message.as_u32()
                    )));
                }
                Ok(target_process_id)
            }
            ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
                artifact.validate_process_ref_type_id_target(
                    "send target payload type",
                    *ty,
                    *target_process,
                )?;
                let received_payload_type = self
                    .message_variants
                    .get(transition.message.index())
                    .and_then(|message| message.payload_type)
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            self.debug_name,
                            transition.message.as_u32()
                        ))
                    })?;
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type id {}, expected {}",
                        self.debug_name,
                        transition.message.as_u32(),
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }

    fn validate_template_process_refs(
        &self,
        artifact: &MantleArtifact,
        template: &ArtifactValueTemplate,
        spawned_refs: &BTreeSet<ProcessRefId>,
    ) -> Result<()> {
        match template {
            ArtifactValueTemplate::Literal { .. }
            | ArtifactValueTemplate::ReceivedPayload { .. }
            | ArtifactValueTemplate::CurrentStatePayload { .. } => Ok(()),
            ArtifactValueTemplate::RecordField { record, .. } => {
                self.validate_template_process_refs(artifact, record, spawned_refs)
            }
            ArtifactValueTemplate::ListElement { list, .. } => {
                self.validate_template_process_refs(artifact, list, spawned_refs)
            }
            ArtifactValueTemplate::MapValue { map, .. } => {
                self.validate_template_process_refs(artifact, map, spawned_refs)
            }
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => {
                artifact.validate_process_ref_type_id_target(
                    "process reference payload type",
                    *ty,
                    *target_process,
                )?;
                let declared_target = self.process_ref_target(*process_ref)?;
                if declared_target != *target_process {
                    return Err(Error::new(format!(
                        "process {} process reference payload id {} targets process id {}, expected {}",
                        self.debug_name,
                        process_ref.as_u32(),
                        declared_target.as_u32(),
                        target_process.as_u32()
                    )));
                }
                if !spawned_refs.contains(process_ref) {
                    return Err(Error::new(format!(
                        "process {} sends unbound process reference id {} as payload",
                        self.debug_name,
                        process_ref.as_u32()
                    )));
                }
                Ok(())
            }
            ArtifactValueTemplate::EnumVariant { payload, .. } => {
                self.validate_template_process_refs(artifact, payload, spawned_refs)
            }
            ArtifactValueTemplate::Record { fields, .. } => {
                for field in fields {
                    self.validate_template_process_refs(artifact, &field.value, spawned_refs)?;
                }
                Ok(())
            }
            ArtifactValueTemplate::List { items, .. } => {
                for item in items {
                    self.validate_template_process_refs(artifact, item, spawned_refs)?;
                }
                Ok(())
            }
            ArtifactValueTemplate::Map { entries, .. } => {
                for entry in entries {
                    self.validate_template_process_refs(artifact, &entry.key, spawned_refs)?;
                    self.validate_template_process_refs(artifact, &entry.value, spawned_refs)?;
                }
                Ok(())
            }
        }
    }

    fn process_ref_target(&self, process_ref: ProcessRefId) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }
}
