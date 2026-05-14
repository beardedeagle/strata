use std::collections::BTreeSet;

pub(crate) use actions::{LoadedAction, LoadedSendTarget};
use admission::{
    validate_loaded_artifact_identity, validate_loaded_ident_field, validate_loaded_output_text,
};
pub(crate) use effects::LoadedEffectAuthority;
use templates::{
    LoadedTemplateAdmission, evaluate_loaded_state_value,
    loaded_template_depends_on_received_payload,
};
use transitions::{TransitionLookup, load_transitions, validate_loaded_transition_coverage};
pub use values::RuntimePayload;
pub(crate) use values::{
    LoadedStateValue, LoadedValueTemplate, LoadedValueTemplateField, LoadedValueTemplateMapEntry,
    RuntimeValue,
};

mod actions;
mod admission;
mod effects;
mod templates;
mod transitions;
mod values;

use mantle_artifact::{
    ArtifactAction, ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef,
    ArtifactSendTarget, ArtifactTransition, ArtifactType, ArtifactTypeKind, EnumVariantId, Error,
    MAX_ACTIONS_PER_PROCESS, MAX_ENUM_VARIANTS_PER_TYPE, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS,
    MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact, MessageId,
    NextState, OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult, TypeId,
    validate_message_label, validate_state_value_identity_label,
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

    pub(crate) fn enum_variant_label(&self, ty: TypeId, variant: EnumVariantId) -> Result<&str> {
        let type_entry = self.type_entry(ty)?;
        type_entry
            .enum_variants
            .get(variant.index())
            .map(String::as_str)
            .ok_or_else(|| {
                Error::new(format!(
                    "loaded type id {} has no enum variant id {}",
                    ty.as_u32(),
                    variant.as_u32()
                ))
            })
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
            self.validate_type_enum_variants(type_index, ty)?;
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

    fn validate_type_enum_variants(&self, type_index: usize, ty: &ArtifactType) -> Result<()> {
        if ty.enum_variants.len() > MAX_ENUM_VARIANTS_PER_TYPE {
            return Err(Error::new(format!(
                "type.{type_index}.enum_variant_count must be no greater than {MAX_ENUM_VARIANTS_PER_TYPE}"
            )));
        }
        let mut seen = BTreeSet::new();
        for (variant_index, variant) in ty.enum_variants.iter().enumerate() {
            validate_loaded_ident_field(
                &format!("type.{type_index}.enum_variant.{variant_index}"),
                variant,
            )?;
            if !seen.insert(variant.as_str()) {
                return Err(Error::new(format!(
                    "type.{type_index} duplicates enum variant {variant}"
                )));
            }
        }
        if matches!(ty.kind, ArtifactTypeKind::ProcessRef { .. }) && !ty.enum_variants.is_empty() {
            return Err(Error::new(format!(
                "type.{type_index} process reference type must not declare enum variants"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcess {
    pub(crate) debug_name: String,
    pub(crate) state_type: TypeId,
    pub(crate) state_values: Vec<LoadedStateValue>,
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
            state_values: process
                .state_values
                .iter()
                .map(LoadedStateValue::from_artifact)
                .collect::<Result<Vec<_>>>()?,
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
            state.value.validate("state value").map_err(|err| {
                Error::new(format!("process {} state value: {err}", self.debug_name))
            })?;
            if state.value.contains_process_ref() {
                return Err(Error::new(format!(
                    "process {} state value {} carries a process reference value",
                    self.debug_name, state.label
                )));
            }
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
                payload
                    .value
                    .validate("state value payload")
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload: {err}",
                            self.debug_name
                        ))
                    })?;
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state.label
                    )));
                }
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

#[derive(Debug, Clone)]
pub(crate) struct LoadedTransition {
    pub(crate) current_state: Option<StateId>,
    pub(crate) message: MessageId,
    pub(crate) payload_guard: Option<RuntimePayload>,
    pub(crate) step_result: StepResult,
    pub(crate) next_state: LoadedNextState,
    pub(crate) effect_authority: LoadedEffectAuthority,
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Result<Self> {
        Ok(Self {
            current_state: transition.current_state,
            message: transition.message,
            payload_guard: transition
                .payload_guard
                .as_ref()
                .map(RuntimePayload::from_artifact)
                .transpose()?,
            step_result: transition.step_result,
            next_state: LoadedNextState::from_artifact(&transition.next_state)?,
            effect_authority: LoadedEffectAuthority::from_artifact(&transition.effects),
            actions: transition
                .actions
                .iter()
                .map(LoadedAction::from_artifact)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        self.validate_next_state(program, process, message)?;
        self.validate_payload_guard(program, process, message)?;

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

    fn validate_payload_guard(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let Some(payload_guard) = &self.payload_guard else {
            return Ok(());
        };
        if payload_guard.process_ref.is_some() || payload_guard.value.contains_process_ref() {
            return Err(Error::new(format!(
                "process {} message id {} payload guard cannot be a process reference payload",
                process.debug_name,
                message.as_u32()
            )));
        }
        let message_variant = process
            .message_variants
            .get(message.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} is not loaded",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        let payload_type = message_variant
            .payload_type
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} has a payload guard but the message does not accept a payload",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        if payload_guard.ty != payload_type {
            return Err(Error::new(format!(
                "process {} message id {} payload guard has type id {}, expected {}",
                process.debug_name,
                message.as_u32(),
                payload_guard.ty.as_u32(),
                payload_type.as_u32()
            )));
        }
        program.validate_value_type(
            &format!(
                "process {} message id {} payload guard",
                process.debug_name,
                message.as_u32()
            ),
            payload_guard.ty,
        )?;
        Ok(())
    }

    fn validate_next_state(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let context = self.transition_context(message);
        match &self.next_state {
            LoadedNextState::Current => Ok(()),
            LoadedNextState::Value(state) => {
                if state.index() >= process.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} {} next_state id {} is not a loaded state value",
                        process.debug_name,
                        context,
                        state.as_u32()
                    )));
                }
                Ok(())
            }
            LoadedNextState::Template(template) => {
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
                        "process {} {} next_state_template",
                        process.debug_name, context
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
                let value =
                    evaluate_loaded_state_value(program, template, None, current_state_payload)?;
                if process.state_values.iter().any(|state_value| {
                    state_value.ty == value.ty && state_value.value == value.value
                }) {
                    return Ok(());
                }
                Err(Error::new(format!(
                    "process {} {} next_state_template produced value {} not admitted by loaded state table",
                    process.debug_name, context, value.label
                )))
            }
        }
    }

    fn transition_context(&self, message: MessageId) -> String {
        match self.current_state {
            Some(current_state) => format!(
                "message id {} current_state id {}",
                message.as_u32(),
                current_state.as_u32()
            ),
            None => format!("message id {}", message.as_u32()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedNextState {
    Current,
    Value(StateId),
    Template(LoadedValueTemplate),
}

impl LoadedNextState {
    pub(crate) fn from_artifact(next_state: &NextState) -> Result<Self> {
        match next_state {
            NextState::Current => Ok(Self::Current),
            NextState::Value(state) => Ok(Self::Value(*state)),
            NextState::Template(template) => Ok(Self::Template(
                LoadedValueTemplate::from_artifact(template)?,
            )),
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
