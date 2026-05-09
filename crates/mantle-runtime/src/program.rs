use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactEffect,
    ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef, ArtifactSendTarget,
    ArtifactStateValue, ArtifactTransition, ArtifactType, ArtifactTypeKind, ArtifactValueTemplate,
    Error, MAX_ACTIONS_PER_PROCESS, MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS,
    MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact, MessageId,
    NextState, OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult, TypeId,
    validate_message_label, validate_payload_value_label, validate_state_value_label,
};

#[derive(Debug, Clone)]
pub(crate) struct LoadedProgram {
    pub(crate) format: String,
    pub(crate) schema_version: String,
    pub(crate) source_language: String,
    pub(crate) module: String,
    pub(crate) entry_process: ProcessId,
    pub(crate) entry_message: MessageId,
    pub(crate) types: Vec<ArtifactType>,
    pub(crate) outputs: Vec<String>,
    pub(crate) processes: Vec<LoadedProcess>,
}

impl LoadedProgram {
    pub(crate) fn from_artifact(artifact: &MantleArtifact) -> Result<Self> {
        artifact.validate()?;
        let processes = artifact
            .processes
            .iter()
            .map(LoadedProcess::from_artifact)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            format: artifact.format.clone(),
            schema_version: artifact.schema_version.clone(),
            source_language: artifact.source_language.clone(),
            module: artifact.module.clone(),
            entry_process: artifact.entry_process,
            entry_message: artifact.entry_message,
            types: artifact.types.clone(),
            outputs: artifact.outputs.clone(),
            processes,
        })
    }

    pub(crate) fn process(&self, id: ProcessId) -> Result<&LoadedProcess> {
        self.processes
            .get(id.index())
            .ok_or_else(|| Error::new(format!("process id {} is not loaded", id.as_u32())))
    }

    pub(crate) fn process_label(&self, id: ProcessId) -> Result<&str> {
        Ok(self.process(id)?.debug_name.as_str())
    }

    pub(crate) fn state_label(&self, process_id: ProcessId, state_id: StateId) -> Result<&str> {
        self.process(process_id)?
            .state_values
            .get(state_id.index())
            .map(|state| state.label.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "state id {} is not loaded for process id {}",
                    state_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn message_label(
        &self,
        process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<&str> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(|message| message.label.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    message_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn message_payload_type(
        &self,
        process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<Option<TypeId>> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(|message| message.payload_type)
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    message_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn output(&self, output_id: OutputId) -> Result<&str> {
        self.outputs
            .get(output_id.index())
            .map(String::as_str)
            .ok_or_else(|| Error::new(format!("output id {} is not loaded", output_id.as_u32())))
    }

    pub(crate) fn type_entry(&self, ty: TypeId) -> Result<&ArtifactType> {
        self.types
            .get(ty.index())
            .ok_or_else(|| Error::new(format!("loaded type id {} is not loaded", ty.as_u32())))
    }

    pub(crate) fn type_label(&self, ty: TypeId) -> Result<&str> {
        Ok(self.type_entry(ty)?.label.as_str())
    }

    pub(crate) fn validate_value_type(&self, field: &str, ty: TypeId) -> Result<()> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::Value => Ok(()),
            ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
                "{field} type id {} must be a value type",
                ty.as_u32()
            ))),
        }
    }

    pub(crate) fn process_ref_target_for_type_id(
        &self,
        field: &str,
        ty: TypeId,
    ) -> Result<ProcessId> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::ProcessRef { target } => {
                self.process(target)?;
                Ok(target)
            }
            ArtifactTypeKind::Value => Err(Error::new(format!(
                "{field} type id {} must be a process reference type",
                ty.as_u32()
            ))),
        }
    }

    pub(crate) fn validate_process_ref_type_id_target(
        &self,
        field: &str,
        ty: TypeId,
        target_process: ProcessId,
    ) -> Result<()> {
        let target = self.process_ref_target_for_type_id(field, ty)?;
        if target != target_process {
            return Err(Error::new(format!(
                "{field} type id {} targets process id {}, expected {}",
                ty.as_u32(),
                target.as_u32(),
                target_process.as_u32()
            )));
        }
        Ok(())
    }

    fn evaluate_state_value_with_current_state(
        &self,
        template: &ArtifactValueTemplate,
        received_payload: Option<&mantle_artifact::ArtifactPayload>,
        current_state_payload: Option<&mantle_artifact::ArtifactPayload>,
    ) -> Result<ArtifactStateValue> {
        template.evaluate_state_value(received_payload, current_state_payload, &|ty| {
            self.type_label(ty).map(str::to_owned)
        })
    }

    pub(crate) fn validate_admission(&self) -> Result<()> {
        validate_loaded_artifact_identity(&self.format, &self.schema_version)?;
        validate_loaded_ident_field("source_language", &self.source_language)?;
        validate_loaded_ident_field("module", &self.module)?;

        if self.types.is_empty() || self.types.len() > MAX_TYPE_COUNT {
            return Err(Error::new(format!(
                "loaded type_count must be between 1 and {MAX_TYPE_COUNT}"
            )));
        }
        if self.processes.is_empty() || self.processes.len() > MAX_PROCESS_COUNT {
            return Err(Error::new(format!(
                "loaded process_count must be between 1 and {MAX_PROCESS_COUNT}"
            )));
        }
        if self.outputs.len() > MAX_OUTPUT_LITERALS {
            return Err(Error::new(format!(
                "loaded output_count must be no greater than {MAX_OUTPUT_LITERALS}"
            )));
        }
        for output in &self.outputs {
            validate_loaded_output_text(output)?;
        }
        for (type_index, ty) in self.types.iter().enumerate() {
            validate_loaded_ident_field(&format!("type.{type_index}.label"), &ty.label)?;
            if let ArtifactTypeKind::ProcessRef { target } = ty.kind {
                self.process(target)?;
            }
        }

        let entry_process = self.process(self.entry_process)?;
        if self.entry_message.index() >= entry_process.message_variants.len() {
            return Err(Error::new(format!(
                "entry message id {} is not loaded for process id {}",
                self.entry_message.as_u32(),
                self.entry_process.as_u32()
            )));
        }
        if entry_process.message_variants[self.entry_message.index()]
            .payload_type
            .is_some()
        {
            return Err(Error::new(format!(
                "entry message id {} must not require a payload",
                self.entry_message.as_u32()
            )));
        }

        let mut process_names = BTreeSet::new();
        for process in &self.processes {
            validate_loaded_ident_field("process debug_name", &process.debug_name)?;
            if !process_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded process debug_name {:?}",
                    process.debug_name
                )));
            }
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            process.validate_admission(self, ProcessId::from_index(process_index)?)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcess {
    pub(crate) debug_name: String,
    pub(crate) state_type: TypeId,
    pub(crate) state_values: Vec<ArtifactStateValue>,
    pub(crate) message_variants: Vec<LoadedMessageVariant>,
    pub(crate) process_refs: Vec<LoadedProcessRef>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
    transition_lookup: TransitionLookup,
}

impl LoadedProcess {
    fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        let transitions = load_transitions(process)?;
        let transition_lookup = TransitionLookup::from_transitions(&transitions);

        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_type: process.state_type,
            state_values: process.state_values.clone(),
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
    ) -> Result<&LoadedTransition> {
        let lookup_state = self
            .transition_lookup
            .is_state_specific_message(message)
            .then_some(current_state);
        let transition_index = self
            .transition_lookup
            .for_dispatch(message, current_state)
            .ok_or_else(|| self.transition_lookup_error(message, lookup_state))?;
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

    fn transition_lookup_error(&self, message: MessageId, current_state: Option<StateId>) -> Error {
        let state = current_state
            .map(|state| format!(" current_state id {}", state.as_u32()))
            .unwrap_or_default();
        Error::new(format!(
            "process {} has no transition for message id {}{}",
            self.debug_name,
            message.as_u32(),
            state
        ))
    }

    fn validate_admission(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
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
                    .checked_add(transition.actions.len())
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
            transition.validate_admission(program, self, message)?;
            transition.effect_authority.validate_actions(
                &self.debug_name,
                message,
                &transition.actions,
            )?;
        }
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
            validate_state_value_label(&state.value).map_err(|err| {
                Error::new(format!("process {} state value: {err}", self.debug_name))
            })?;
            validate_state_value_label(&state.label).map_err(|err| {
                Error::new(format!("process {} state label: {err}", self.debug_name))
            })?;
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
                validate_payload_value_label(&payload.value).map_err(|err| {
                    Error::new(format!(
                        "process {} state value payload: {err}",
                        self.debug_name
                    ))
                })?;
                if payload.process_ref.is_some() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state.label
                    )));
                }
            }
            if !states.insert((state.ty, state.value.as_str())) {
                return Err(Error::new(format!(
                    "process {} loads duplicate state value {} with type id {}",
                    self.debug_name,
                    state.value,
                    state.ty.as_u32()
                )));
            }
        }
        Ok(())
    }

    fn validate_message_table(&self, program: &LoadedProgram) -> Result<()> {
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

    fn process_ref_target(&self, process_ref: ProcessRefId) -> Result<ProcessId> {
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

#[derive(Debug, Clone)]
struct TransitionLookup {
    by_key: BTreeMap<(u32, Option<u32>), usize>,
    state_specific_messages: BTreeSet<u32>,
}

impl TransitionLookup {
    fn from_transitions(transitions: &[LoadedTransition]) -> Self {
        let mut by_key = BTreeMap::new();
        let mut state_specific_messages = BTreeSet::new();
        for (index, transition) in transitions.iter().enumerate() {
            let message = transition.message.as_u32();
            let current_state = transition.current_state.map(StateId::as_u32);
            if current_state.is_some() {
                state_specific_messages.insert(message);
            }
            by_key.insert((message, current_state), index);
        }
        Self {
            by_key,
            state_specific_messages,
        }
    }

    fn for_dispatch(&self, message: MessageId, current_state: StateId) -> Option<usize> {
        if self.is_state_specific_message(message) {
            self.exact(message, Some(current_state))
        } else {
            self.exact(message, None)
        }
    }

    fn exact(&self, message: MessageId, current_state: Option<StateId>) -> Option<usize> {
        self.by_key
            .get(&(message.as_u32(), current_state.map(StateId::as_u32)))
            .copied()
    }

    fn is_state_specific_message(&self, message: MessageId) -> bool {
        self.state_specific_messages.contains(&message.as_u32())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedMessageVariant {
    pub(crate) label: String,
    pub(crate) payload_type: Option<TypeId>,
}

impl LoadedMessageVariant {
    fn from_artifact(message: &ArtifactMessageVariant) -> Self {
        Self {
            label: message.label.clone(),
            payload_type: message.payload_type,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcessRef {
    pub(crate) target: ProcessId,
}

impl LoadedProcessRef {
    fn from_artifact(process_ref: &ArtifactProcessRef) -> Self {
        Self {
            target: process_ref.target,
        }
    }
}

fn load_transitions(process: &ArtifactProcess) -> Result<Vec<LoadedTransition>> {
    process
        .transitions
        .iter()
        .map(|transition| {
            if transition.message.index() >= process.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not loaded",
                    process.debug_name,
                    transition.message.as_u32()
                )));
            }
            Ok(LoadedTransition::from_artifact(transition))
        })
        .collect()
}

fn validate_loaded_transition_coverage(process: &LoadedProcess) -> Result<()> {
    let mut transition_keys = BTreeSet::new();
    for transition in &process.transitions {
        if !transition_keys.insert((
            transition.message.as_u32(),
            transition.current_state.map(StateId::as_u32),
        )) {
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {} current_state {:?}",
                process.debug_name,
                transition.message.as_u32(),
                transition.current_state.map(StateId::as_u32)
            )));
        }
    }

    for message_index in 0..process.message_variants.len() {
        let message = message_index as u32;
        let has_unguarded = transition_keys.contains(&(message, None));
        let has_guarded = (0..process.state_values.len())
            .any(|state_index| transition_keys.contains(&(message, Some(state_index as u32))));
        if has_unguarded {
            if has_guarded {
                return Err(Error::new(format!(
                    "process {} mixes unguarded and state-specific transitions for message id {}",
                    process.debug_name, message
                )));
            }
            continue;
        }
        if !has_guarded {
            return Err(Error::new(format!(
                "process {} has no transition for message id {}",
                process.debug_name, message
            )));
        }
        for state_index in 0..process.state_values.len() {
            if !transition_keys.contains(&(message, Some(state_index as u32))) {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {} current_state id {}",
                    process.debug_name, message, state_index
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedTransition {
    pub(crate) current_state: Option<StateId>,
    pub(crate) message: MessageId,
    pub(crate) step_result: StepResult,
    pub(crate) next_state: NextState,
    pub(crate) effect_authority: LoadedEffectAuthority,
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Self {
        Self {
            current_state: transition.current_state,
            message: transition.message,
            step_result: transition.step_result,
            next_state: transition.next_state.clone(),
            effect_authority: LoadedEffectAuthority::from_artifact(&transition.effects),
            actions: transition
                .actions
                .iter()
                .map(LoadedAction::from_artifact)
                .collect(),
        }
    }

    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        self.validate_next_state(program, process, message)?;

        let current_state_payload_type = transition_current_state_payload_type(process, self)?;
        let mut spawned_refs = vec![false; process.process_refs.len()];
        for action in &self.actions {
            action.validate_admission(
                program,
                process,
                message,
                current_state_payload_type,
                &mut spawned_refs,
            )?;
        }
        Ok(())
    }

    fn validate_next_state(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        match &self.next_state {
            NextState::Current => Ok(()),
            NextState::Value(state) => {
                if state.index() >= process.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} transition {} next_state id {} is not a loaded state value",
                        process.debug_name,
                        message.as_u32(),
                        state.as_u32()
                    )));
                }
                Ok(())
            }
            NextState::Template(template) => {
                let received_payload_type = process.message_variants[message.index()].payload_type;
                let current_state_payload_type =
                    transition_current_state_payload_type(process, self)?;
                LoadedTemplateAdmission {
                    expected_type: Some(process.state_type),
                    received_payload_type,
                    current_state_payload_type,
                    allow_direct_process_ref: false,
                    program,
                    process,
                    spawned_refs: &[],
                }
                .validate(
                    &format!(
                        "process {} transition {} next_state_template",
                        process.debug_name,
                        message.as_u32()
                    ),
                    template,
                )?;
                if loaded_template_depends_on_received_payload(template) {
                    return Ok(());
                }
                let current_state_payload = self
                    .current_state
                    .and_then(|state| process.state_values.get(state.index()))
                    .and_then(|state| state.payload.as_ref());
                let value = program.evaluate_state_value_with_current_state(
                    template,
                    None,
                    current_state_payload,
                )?;
                if process.state_values.iter().any(|state_value| {
                    state_value.ty == value.ty && state_value.value == value.value
                }) {
                    return Ok(());
                }
                Err(Error::new(format!(
                    "process {} transition {} next_state_template produced value {} not admitted by loaded state table",
                    process.debug_name,
                    message.as_u32(),
                    value.label
                )))
            }
        }
    }
}

fn transition_current_state_payload_type(
    process: &LoadedProcess,
    transition: &LoadedTransition,
) -> Result<Option<TypeId>> {
    let Some(current_state) = transition.current_state else {
        return Ok(None);
    };
    let state_value = process
        .state_values
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} current_state id {} is not a loaded state value",
                process.debug_name,
                transition.message.as_u32(),
                current_state.as_u32()
            ))
        })?;
    Ok(state_value.payload.as_ref().map(|payload| payload.ty))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedEffectAuthority {
    effects: Vec<ArtifactEffect>,
}

impl LoadedEffectAuthority {
    pub(crate) fn from_artifact(effects: &[ArtifactEffect]) -> Self {
        Self {
            effects: effects.to_vec(),
        }
    }

    pub(crate) fn validate_actions(
        &self,
        process_name: &str,
        message: MessageId,
        actions: &[LoadedAction],
    ) -> Result<()> {
        let mut admitted = [false; 3];
        for &effect in &self.effects {
            let index = Self::effect_index(effect);
            if admitted[index] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} admits duplicate effect {effect}",
                    message.as_u32()
                )));
            }
            admitted[index] = true;
        }

        let mut used = [false; 3];
        for action in actions {
            let effect = action.effect();
            let index = Self::effect_index(effect);
            if !admitted[index] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} uses effect {effect} without admitted authority",
                    message.as_u32()
                )));
            }
            used[index] = true;
        }

        for &effect in &self.effects {
            if !used[Self::effect_index(effect)] {
                return Err(Error::new(format!(
                    "process {process_name} transition {} admits effect {effect} but no action uses it",
                    message.as_u32()
                )));
            }
        }

        Ok(())
    }

    fn effect_index(effect: ArtifactEffect) -> usize {
        match effect {
            ArtifactEffect::Emit => 0,
            ArtifactEffect::Spawn => 1,
            ArtifactEffect::Send => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        process_ref: ProcessRefId,
    },
    Send {
        target: LoadedSendTarget,
        message: MessageId,
        payload: Option<ArtifactValueTemplate>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedSendTarget {
    ProcessRef(ProcessRefId),
    ReceivedPayload {
        ty: TypeId,
        target_process: ProcessId,
    },
}

impl LoadedAction {
    fn effect(&self) -> ArtifactEffect {
        match self {
            Self::Emit { .. } => ArtifactEffect::Emit,
            Self::Spawn { .. } => ArtifactEffect::Spawn,
            Self::Send { .. } => ArtifactEffect::Send,
        }
    }

    fn from_artifact(action: &ArtifactAction) -> Self {
        match action {
            ArtifactAction::Emit { output } => Self::Emit { output: *output },
            ArtifactAction::Spawn {
                target,
                process_ref,
            } => Self::Spawn {
                target: *target,
                process_ref: *process_ref,
            },
            ArtifactAction::Send {
                target,
                message,
                payload,
            } => Self::Send {
                target: LoadedSendTarget::from_artifact(target),
                message: *message,
                payload: payload.clone(),
            },
        }
    }

    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
        current_state_payload_type: Option<TypeId>,
        spawned_refs: &mut [bool],
    ) -> Result<()> {
        match self {
            Self::Emit { output } => {
                program.output(*output)?;
                Ok(())
            }
            Self::Spawn {
                target,
                process_ref,
            } => {
                program.process(*target)?;
                let declared_target = process.process_ref_target(*process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn process reference id {} targets process id {}, expected {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                let Some(is_spawned) = spawned_refs.get_mut(process_ref.index()) else {
                    return Err(Error::new(format!(
                        "process {} transition {} spawn references unloaded process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                };
                if *is_spawned {
                    return Err(Error::new(format!(
                        "process {} transition {} duplicates process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                }
                *is_spawned = true;
                Ok(())
            }
            Self::Send {
                target,
                message: sent_message,
                payload,
            } => {
                let target_process_id =
                    target.validate_admission(program, process, message, spawned_refs)?;
                let target_process = program.process(target_process_id)?;
                let target_message = target_process.message_variants.get(sent_message.index()).ok_or_else(|| {
                    Error::new(format!(
                        "process {} transition {} sends message id {} not loaded by process id {}",
                        process.debug_name,
                        message.as_u32(),
                        sent_message.as_u32(),
                        target_process_id.as_u32()
                    ))
                })?;
                match (target_message.payload_type, payload) {
                    (None, None) => Ok(()),
                    (None, Some(_)) => Err(Error::new(format!(
                        "process {} transition {} sends payload to process id {} message id {}, which does not accept one",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                    (Some(_), None) => Err(Error::new(format!(
                        "process {} transition {} sends process id {} message id {} without required payload",
                        process.debug_name,
                        message.as_u32(),
                        target_process_id.as_u32(),
                        sent_message.as_u32()
                    ))),
                    (Some(payload_type), Some(payload)) => LoadedTemplateAdmission {
                        expected_type: Some(payload_type),
                        received_payload_type: process.message_variants[message.index()]
                            .payload_type,
                        current_state_payload_type,
                        allow_direct_process_ref: true,
                        program,
                        process,
                        spawned_refs,
                    }
                    .validate(
                        &format!(
                            "process {} transition {} send payload",
                            process.debug_name,
                            message.as_u32()
                        ),
                        payload,
                    ),
                }
            }
        }
    }
}

impl LoadedSendTarget {
    fn from_artifact(target: &ArtifactSendTarget) -> Self {
        match target {
            ArtifactSendTarget::ProcessRef(process_ref) => Self::ProcessRef(*process_ref),
            ArtifactSendTarget::ReceivedPayload { ty, target_process } => Self::ReceivedPayload {
                ty: *ty,
                target_process: *target_process,
            },
        }
    }

    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
        spawned_refs: &[bool],
    ) -> Result<ProcessId> {
        match self {
            Self::ProcessRef(process_ref) => {
                let target_process = process.process_ref_target(*process_ref)?;
                let is_spawned = spawned_refs.get(process_ref.index()).copied().ok_or_else(|| {
                    Error::new(format!(
                        "process {} transition {} sends through unloaded process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    ))
                })?;
                if !is_spawned {
                    return Err(Error::new(format!(
                        "process {} transition {} sends through unbound process reference id {}",
                        process.debug_name,
                        message.as_u32(),
                        process_ref.as_u32()
                    )));
                }
                Ok(target_process)
            }
            Self::ReceivedPayload { ty, target_process } => {
                program.validate_process_ref_type_id_target(
                    "send target payload type",
                    *ty,
                    *target_process,
                )?;
                let received_payload_type = process.message_variants[message.index()]
                    .payload_type
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            process.debug_name,
                            message.as_u32()
                        ))
                    })?;
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type id {}, expected {}",
                        process.debug_name,
                        message.as_u32(),
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct LoadedTemplateAdmission<'a> {
    expected_type: Option<TypeId>,
    received_payload_type: Option<TypeId>,
    current_state_payload_type: Option<TypeId>,
    allow_direct_process_ref: bool,
    program: &'a LoadedProgram,
    process: &'a LoadedProcess,
    spawned_refs: &'a [bool],
}

impl LoadedTemplateAdmission<'_> {
    fn validate(&self, field: &str, template: &ArtifactValueTemplate) -> Result<()> {
        self.validate_with_depth(field, template, 0)
    }

    fn validate_with_depth(
        &self,
        field: &str,
        template: &ArtifactValueTemplate,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        self.program.type_entry(template.result_type())?;
        if let Some(expected_type) = self.expected_type {
            if template.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type id {}, expected {}",
                    template.result_type().as_u32(),
                    expected_type.as_u32()
                )));
            }
        }

        match template {
            ArtifactValueTemplate::Literal { ty, value } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_payload_value_label(value)
                    .map_err(|err| Error::new(format!("{field}: {err}")))
            }
            ArtifactValueTemplate::ReceivedPayload { ty } => {
                self.validate_received_payload(field, *ty)
            }
            ArtifactValueTemplate::CurrentStatePayload { ty } => {
                self.validate_current_state_payload(field, *ty)
            }
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => self.validate_process_ref(field, *ty, *target_process, *process_ref),
            ArtifactValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                validate_loaded_ident_field(&format!("{field}.variant"), variant)?;
                let nested = Self {
                    expected_type: None,
                    allow_direct_process_ref: false,
                    ..*self
                };
                nested.validate_with_depth(&format!("{field}.payload"), payload, depth + 1)
            }
            ArtifactValueTemplate::Record { ty, fields } => {
                self.program
                    .validate_value_type(&format!("{field}.type"), *ty)?;
                self.validate_record(field, fields, depth)
            }
        }
    }

    fn validate_received_payload(&self, field: &str, ty: TypeId) -> Result<()> {
        let Some(received_payload_type) = self.received_payload_type else {
            return Err(Error::new(format!(
                "{field} requires a payload-bearing transition message"
            )));
        };
        if ty != received_payload_type {
            return Err(Error::new(format!(
                "{field} has received payload type id {}, expected {}",
                ty.as_u32(),
                received_payload_type.as_u32()
            )));
        }
        if !self.allow_direct_process_ref
            && matches!(
                self.program.type_entry(ty)?.kind,
                ArtifactTypeKind::ProcessRef { .. }
            )
        {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        Ok(())
    }

    fn validate_current_state_payload(&self, field: &str, ty: TypeId) -> Result<()> {
        let Some(current_state_payload_type) = self.current_state_payload_type else {
            return Err(Error::new(format!(
                "{field} requires a payload-bearing current state"
            )));
        };
        if ty != current_state_payload_type {
            return Err(Error::new(format!(
                "{field} has current state payload type id {}, expected {}",
                ty.as_u32(),
                current_state_payload_type.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_process_ref(
        &self,
        field: &str,
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        if !self.allow_direct_process_ref {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        self.program.validate_process_ref_type_id_target(
            "process reference payload type",
            ty,
            target_process,
        )?;
        let declared_target = self.process.process_ref_target(process_ref)?;
        if declared_target != target_process {
            return Err(Error::new(format!(
                "process {} process reference payload id {} targets process id {}, expected {}",
                self.process.debug_name,
                process_ref.as_u32(),
                declared_target.as_u32(),
                target_process.as_u32()
            )));
        }
        let is_spawned = self
            .spawned_refs
            .get(process_ref.index())
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unloaded process reference id {} as payload",
                    self.process.debug_name,
                    process_ref.as_u32()
                ))
            })?;
        if !is_spawned {
            return Err(Error::new(format!(
                "process {} sends unbound process reference id {} as payload",
                self.process.debug_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_record(
        &self,
        field: &str,
        fields: &[mantle_artifact::ArtifactValueTemplateField],
        depth: usize,
    ) -> Result<()> {
        if fields.is_empty() || fields.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "{field}.field_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        let mut names = BTreeSet::new();
        for record_field in fields {
            validate_loaded_ident_field(&format!("{field}.field"), &record_field.name)?;
            if !names.insert(record_field.name.as_str()) {
                return Err(Error::new(format!(
                    "{field} duplicates field {}",
                    record_field.name
                )));
            }
            let nested = Self {
                expected_type: None,
                allow_direct_process_ref: false,
                ..*self
            };
            nested.validate_with_depth(
                &format!("{field}.field.{}", record_field.name),
                &record_field.value,
                depth + 1,
            )?;
        }
        Ok(())
    }
}

fn validate_loaded_artifact_identity(format: &str, schema_version: &str) -> Result<()> {
    validate_loaded_identity_field("loaded artifact format", format)?;
    validate_loaded_identity_field("loaded artifact schema_version", schema_version)?;
    if format != ARTIFACT_FORMAT {
        return Err(Error::new(format!(
            "loaded artifact format {format:?}; expected {ARTIFACT_FORMAT:?}"
        )));
    }
    if schema_version != ARTIFACT_SCHEMA_VERSION {
        return Err(Error::new(format!(
            "loaded artifact schema_version {schema_version:?}; expected {ARTIFACT_SCHEMA_VERSION:?}"
        )));
    }
    Ok(())
}

fn validate_loaded_identity_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(Error::new(format!(
            "{field} must be non-empty and contain no control characters, got {value:?}"
        )));
    }
    Ok(())
}

fn validate_loaded_output_text(output: &str) -> Result<()> {
    if output.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "loaded output exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(Error::new(
            "loaded output must be non-empty and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_loaded_ident_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_artifact_ident(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} must be an identifier, got {value:?}"
        )))
    }
}

fn is_artifact_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn loaded_template_depends_on_received_payload(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } | ArtifactValueTemplate::ProcessRef { .. } => false,
        ArtifactValueTemplate::ReceivedPayload { .. } => true,
        ArtifactValueTemplate::CurrentStatePayload { .. } => false,
        ArtifactValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_received_payload(payload)
        }
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_received_payload(&field.value)),
    }
}
