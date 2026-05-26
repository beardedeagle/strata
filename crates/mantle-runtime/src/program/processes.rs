use std::collections::BTreeSet;

use super::*;

impl LoadedProcess {
    pub(in crate::program) fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        let transitions = load_transitions(process)?;
        let transition_lookup = TransitionLookup::from_transitions(&transitions);

        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_type: process.state_type,
            state_values: process
                .state_values
                .iter()
                .map(LoadedStateValue::from_artifact)
                .collect::<Result<Vec<_>>>()?,
            message_type: process.message_type,
            message_variants: process
                .message_variants
                .iter()
                .map(LoadedMessageVariant::from_artifact)
                .collect(),
            process_refs: process
                .process_refs
                .iter()
                .map(LoadedProcessRef::from_artifact)
                .collect(),
            mailbox_bound: process.mailbox_bound,
            init_state: process.init_state,
            transitions,
            transition_lookup,
        })
    }

    pub(crate) fn transition_for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Result<&LoadedTransition> {
        let lookup_state = self
            .transition_lookup
            .is_state_specific_message(message)
            .then_some(current_state);
        let payload_specific = self
            .transition_lookup
            .is_payload_specific_base(message, lookup_state);
        let transition_index = self
            .transition_lookup
            .for_dispatch(message, current_state, payload)
            .ok_or_else(|| {
                self.transition_lookup_error(message, lookup_state, payload_specific, payload)
            })?;
        self.transition_by_lookup_index(transition_index)
    }

    fn transition_by_lookup_index(&self, index: usize) -> Result<&LoadedTransition> {
        self.transitions.get(index).ok_or_else(|| {
            Error::new(format!(
                "process {} transition index {} is not loaded",
                self.debug_name, index
            ))
        })
    }

    fn transition_lookup_error(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
        payload_specific: bool,
        payload: Option<&RuntimePayload>,
    ) -> Error {
        let state = current_state
            .map(|state| format!(" current_state id {}", state.as_u32()))
            .unwrap_or_default();
        if payload_specific {
            return match payload {
                Some(payload) => Error::new(format!(
                    "process {} has no transition for message id {}{} payload {}",
                    self.debug_name,
                    message.as_u32(),
                    state,
                    payload.label()
                )),
                None => Error::new(format!(
                    "process {} has payload-specific transition(s) for message id {}{}, but the queued message has no payload",
                    self.debug_name,
                    message.as_u32(),
                    state
                )),
            };
        }
        Error::new(format!(
            "process {} has no transition for message id {}{}",
            self.debug_name,
            message.as_u32(),
            state
        ))
    }

    pub(in crate::program) fn validate_admission(
        &self,
        program: &LoadedProgram,
        process_id: ProcessId,
    ) -> Result<()> {
        self.validate_state_table(program)?;
        self.validate_message_table(program)?;
        self.validate_process_refs(program, process_id)?;
        if self.mailbox_bound == 0 || self.mailbox_bound > MAX_MAILBOX_BOUND {
            return Err(Error::new(format!(
                "process {} loaded mailbox_bound must be between 1 and {MAX_MAILBOX_BOUND}",
                self.debug_name
            )));
        }
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a loaded state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.transitions.is_empty() || self.transitions.len() > MAX_TRANSITIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded transition_count must be between 1 and {MAX_TRANSITIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let action_count = self
            .transitions
            .iter()
            .try_fold(0usize, |count, transition| {
                count
                    .checked_add(actions::action_count(&transition.actions)?)
                    .ok_or_else(|| Error::new("loaded action_count overflowed"))
            })?;
        if action_count > MAX_ACTIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        validate_loaded_transition_coverage(self)?;
        for transition in &self.transitions {
            let message = transition.message;
            transition.validate_admission(program, self, process_id, message)?;
            transition.effect_authority.validate_actions(
                &self.debug_name,
                message,
                &transition.actions,
            )?;
        }
        self.validate_message_type_shape(program)?;
        Ok(())
    }

    fn validate_state_table(&self, program: &LoadedProgram) -> Result<()> {
        validate_loaded_ident_field("process debug_name", &self.debug_name)?;
        program.validate_value_type("state_type", self.state_type)?;
        if self.state_values.is_empty() || self.state_values.len() > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded state_value_count must be between 1 and {MAX_STATE_VALUES_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut states = BTreeSet::new();
        for state in &self.state_values {
            program
                .validate_value_type("state value type", state.ty)
                .map_err(|err| {
                    Error::new(format!(
                        "process {} state value type: {err}",
                        self.debug_name
                    ))
                })?;
            program
                .validate_value_matches_type("state value", state.ty, &state.value)
                .map_err(|err| {
                    Error::new(format!("process {} state value: {err}", self.debug_name))
                })?;
            validate_state_value_identity_label(&state.value, &state.label)
                .map_err(|err| Error::new(format!("process {} {err}", self.debug_name)))?;
            if state.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} loaded state value {} has type id {}, expected {}",
                    self.debug_name,
                    state.label,
                    state.ty.as_u32(),
                    self.state_type.as_u32()
                )));
            }
            if let Some(payload) = &state.payload {
                program
                    .validate_value_type("state value payload type", payload.ty)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload type: {err}",
                            self.debug_name
                        ))
                    })?;
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state.label
                    )));
                }
                program
                    .validate_value_matches_type("state value payload", payload.ty, &payload.value)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload: {err}",
                            self.debug_name
                        ))
                    })?;
            }
            if !states.insert((state.ty, state.value.clone())) {
                return Err(Error::new(format!(
                    "process {} loads duplicate state value {} with type id {}",
                    self.debug_name,
                    state.value.label(),
                    state.ty.as_u32()
                )));
            }
        }
        Ok(())
    }

    fn validate_message_table(&self, program: &LoadedProgram) -> Result<()> {
        program.validate_value_type("message_type", self.message_type)?;
        if self.message_variants.is_empty()
            || self.message_variants.len() > MAX_MESSAGE_VARIANTS_PER_PROCESS
        {
            return Err(Error::new(format!(
                "process {} loaded message_count must be between 1 and {MAX_MESSAGE_VARIANTS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut messages = BTreeSet::new();
        for message in &self.message_variants {
            validate_message_label(&message.label).map_err(|err| {
                Error::new(format!("process {} message label: {err}", self.debug_name))
            })?;
            if let Some(payload_type) = message.payload_type {
                program.type_entry(payload_type).map_err(|err| {
                    Error::new(format!(
                        "process {} message payload_type: {err}",
                        self.debug_name
                    ))
                })?;
            }
            if !messages.insert(message.label.as_str()) {
                return Err(Error::new(format!(
                    "process {} loads duplicate message label {}",
                    self.debug_name, message.label
                )));
            }
        }
        Ok(())
    }

    fn validate_message_type_shape(&self, program: &LoadedProgram) -> Result<()> {
        let message_type = program.type_entry(self.message_type)?;
        let ArtifactValueShape::Enum { variants } = message_type.value_shape()? else {
            return Err(Error::new(format!(
                "process {} loaded message_type id {} must be an enum aligned with message variants",
                self.debug_name,
                self.message_type.as_u32()
            )));
        };
        if variants.len() != self.message_variants.len() {
            return Err(Error::new(format!(
                "process {} loaded message_type id {} declares {} variant(s), expected {} message variant(s)",
                self.debug_name,
                self.message_type.as_u32(),
                variants.len(),
                self.message_variants.len()
            )));
        }
        for (index, (message, variant)) in self
            .message_variants
            .iter()
            .zip(variants.iter())
            .enumerate()
        {
            if variant.label != message.label {
                return Err(Error::new(format!(
                    "process {} loaded message_type id {} variant {index} label {} does not match message label {}",
                    self.debug_name,
                    self.message_type.as_u32(),
                    variant.label,
                    message.label
                )));
            }
            if variant.payload_type != message.payload_type {
                return Err(Error::new(format!(
                    "process {} loaded message_type id {} variant {index} payload type {:?}, expected {:?}",
                    self.debug_name,
                    self.message_type.as_u32(),
                    variant.payload_type.map(TypeId::as_u32),
                    message.payload_type.map(TypeId::as_u32)
                )));
            }
        }
        Ok(())
    }

    fn validate_process_refs(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        if self.process_refs.len() > MAX_PROCESS_REFS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded process_ref_count must be no greater than {MAX_PROCESS_REFS_PER_PROCESS}",
                self.debug_name
            )));
        }

        for (process_ref_index, process_ref) in self.process_refs.iter().enumerate() {
            program.process(process_ref.target)?;
            if process_ref.target == program.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets entry process id {}",
                    self.debug_name,
                    process_ref_index,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets itself",
                    self.debug_name, process_ref_index
                )));
            }
        }
        Ok(())
    }

    pub(in crate::program) fn process_ref_target(
        &self,
        process_ref: ProcessRefId,
    ) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references unloaded process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }
}
