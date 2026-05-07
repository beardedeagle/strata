use std::collections::BTreeSet;

use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactEffect,
    ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef, ArtifactSendTarget,
    ArtifactStateValue, ArtifactTransition, ArtifactValueTemplate, Error, MAX_ACTIONS_PER_PROCESS,
    MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TYPE_REF_BYTES,
    MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact, MessageId, NextState,
    OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult, validate_message_label,
    validate_payload_value_label, validate_state_value_label,
};

#[derive(Debug, Clone)]
pub(crate) struct LoadedProgram {
    pub(crate) format: String,
    pub(crate) schema_version: String,
    pub(crate) source_language: String,
    pub(crate) module: String,
    pub(crate) entry_process: ProcessId,
    pub(crate) entry_message: MessageId,
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
    ) -> Result<Option<&str>> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(|message| message.payload_type.as_deref())
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

    pub(crate) fn validate_admission(&self) -> Result<()> {
        validate_loaded_artifact_identity(&self.format, &self.schema_version)?;
        validate_loaded_ident_field("source_language", &self.source_language)?;
        validate_loaded_ident_field("module", &self.module)?;

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
            if !process_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded process debug_name {}",
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
    pub(crate) state_type: String,
    pub(crate) state_values: Vec<ArtifactStateValue>,
    pub(crate) message_variants: Vec<LoadedMessageVariant>,
    pub(crate) process_refs: Vec<LoadedProcessRef>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
}

impl LoadedProcess {
    fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_type: process.state_type.clone(),
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
            transitions: load_transitions_by_message(process)?,
        })
    }

    pub(crate) fn transition_for_message(&self, message: MessageId) -> Result<&LoadedTransition> {
        self.transitions.get(message.index()).ok_or_else(|| {
            Error::new(format!(
                "process {} has no transition for message id {}",
                self.debug_name,
                message.as_u32()
            ))
        })
    }

    fn validate_admission(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        self.validate_state_table()?;
        self.validate_message_table()?;
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
        if self.transitions.len() != self.message_variants.len() {
            return Err(Error::new(format!(
                "process {} loaded transition_count must equal message_count",
                self.debug_name
            )));
        }
        if self.transitions.len() > MAX_MESSAGE_VARIANTS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded transition_count must be no greater than {MAX_MESSAGE_VARIANTS_PER_PROCESS}",
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

        for (message_index, transition) in self.transitions.iter().enumerate() {
            let message = MessageId::from_index(message_index)?;
            transition.validate_admission(program, self, message)?;
            transition.effect_authority.validate_actions(
                &self.debug_name,
                message,
                &transition.actions,
            )?;
        }
        Ok(())
    }

    fn validate_state_table(&self) -> Result<()> {
        validate_loaded_ident_field("process debug_name", &self.debug_name)?;
        validate_loaded_type_field("state_type", &self.state_type)?;
        if self.state_values.is_empty() || self.state_values.len() > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded state_value_count must be between 1 and {MAX_STATE_VALUES_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut states = BTreeSet::new();
        for state in &self.state_values {
            validate_state_value_label(&state.value).map_err(|err| {
                Error::new(format!("process {} state value: {err}", self.debug_name))
            })?;
            validate_state_value_label(&state.label).map_err(|err| {
                Error::new(format!("process {} state label: {err}", self.debug_name))
            })?;
            if state.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} loaded state value {} has type {}, expected {}",
                    self.debug_name, state.label, state.ty, self.state_type
                )));
            }
            if !states.insert((state.ty.as_str(), state.value.as_str())) {
                return Err(Error::new(format!(
                    "process {} loads duplicate state value {} with type {}",
                    self.debug_name, state.value, state.ty
                )));
            }
        }
        Ok(())
    }

    fn validate_message_table(&self) -> Result<()> {
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
            if let Some(payload_type) = &message.payload_type {
                validate_loaded_type_field("message payload_type", payload_type).map_err(
                    |err| {
                        Error::new(format!(
                            "process {} message payload_type: {err}",
                            self.debug_name
                        ))
                    },
                )?;
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
    pub(crate) payload_type: Option<String>,
}

impl LoadedMessageVariant {
    fn from_artifact(message: &ArtifactMessageVariant) -> Self {
        Self {
            label: message.label.clone(),
            payload_type: message.payload_type.clone(),
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

fn load_transitions_by_message(process: &ArtifactProcess) -> Result<Vec<LoadedTransition>> {
    let mut transitions = vec![None; process.message_variants.len()];
    for transition in &process.transitions {
        let Some(slot) = transitions.get_mut(transition.message.index()) else {
            return Err(Error::new(format!(
                "process {} transition message id {} is not loaded",
                process.debug_name,
                transition.message.as_u32()
            )));
        };
        if slot
            .replace(LoadedTransition::from_artifact(transition))
            .is_some()
        {
            return Err(Error::new(format!(
                "process {} declares duplicate transition for message id {}",
                process.debug_name,
                transition.message.as_u32()
            )));
        }
    }

    transitions
        .into_iter()
        .enumerate()
        .map(|(message_index, transition)| {
            transition.ok_or_else(|| {
                Error::new(format!(
                    "process {} has no transition for message id {}",
                    process.debug_name, message_index
                ))
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedTransition {
    pub(crate) step_result: StepResult,
    pub(crate) next_state: NextState,
    pub(crate) effect_authority: LoadedEffectAuthority,
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Self {
        Self {
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

        let mut spawned_refs = vec![false; process.process_refs.len()];
        for action in &self.actions {
            action.validate_admission(program, process, message, &mut spawned_refs)?;
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
                let received_payload_type = process.message_variants[message.index()]
                    .payload_type
                    .as_deref();
                LoadedTemplateAdmission {
                    expected_type: Some(&process.state_type),
                    received_payload_type,
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
                let value = template.evaluate_state_value(None)?;
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
        ty: String,
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
                match (target_message.payload_type.as_deref(), payload) {
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
                            .payload_type
                            .as_deref(),
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
                ty: ty.clone(),
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
                validate_loaded_process_ref_type_target(
                    program,
                    "send target payload type",
                    ty,
                    *target_process,
                )?;
                let received_payload_type = process.message_variants[message.index()]
                    .payload_type
                    .as_deref()
                    .ok_or_else(|| {
                        Error::new(format!(
                            "process {} transition {} send target requires a payload-bearing message",
                            process.debug_name,
                            message.as_u32()
                        ))
                    })?;
                if ty != received_payload_type {
                    return Err(Error::new(format!(
                        "process {} transition {} send target has received payload type {ty}, expected {received_payload_type}",
                        process.debug_name,
                        message.as_u32()
                    )));
                }
                Ok(*target_process)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct LoadedTemplateAdmission<'a> {
    expected_type: Option<&'a str>,
    received_payload_type: Option<&'a str>,
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
        validate_loaded_type_field(&format!("{field}.type"), template.result_type())?;
        if let Some(expected_type) = self.expected_type {
            if template.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type {}, expected {}",
                    template.result_type(),
                    expected_type
                )));
            }
        }

        match template {
            ArtifactValueTemplate::Literal { value, .. } => validate_payload_value_label(value)
                .map_err(|err| Error::new(format!("{field}: {err}"))),
            ArtifactValueTemplate::ReceivedPayload { ty } => {
                self.validate_received_payload(field, ty)
            }
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => self.validate_process_ref(field, ty, *target_process, *process_ref),
            ArtifactValueTemplate::Record { fields, .. } => {
                self.validate_record(field, fields, depth)
            }
        }
    }

    fn validate_received_payload(&self, field: &str, ty: &str) -> Result<()> {
        let Some(received_payload_type) = self.received_payload_type else {
            return Err(Error::new(format!(
                "{field} requires a payload-bearing transition message"
            )));
        };
        if ty != received_payload_type {
            return Err(Error::new(format!(
                "{field} has received payload type {ty}, expected {received_payload_type}"
            )));
        }
        Ok(())
    }

    fn validate_process_ref(
        &self,
        field: &str,
        ty: &str,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        if !self.allow_direct_process_ref {
            return Err(Error::new(format!(
                "{field} process reference template must be a direct message payload"
            )));
        }
        validate_loaded_process_ref_type_target(
            self.program,
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
            "loaded artifact schema version {schema_version:?}; expected {ARTIFACT_SCHEMA_VERSION:?}"
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

fn validate_loaded_type_field(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_TYPE_REF_BYTES {
        return Err(Error::new(format!(
            "{field} exceeds maximum type length of {MAX_TYPE_REF_BYTES} bytes"
        )));
    }
    if value.len() > MAX_IDENTIFIER_BYTES && is_artifact_ident(value) {
        return Err(Error::new(format!(
            "{field} exceeds maximum type identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_artifact_type_ref(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} must be a type reference, got {value:?}"
        )))
    }
}

fn validate_loaded_process_ref_type_field(field: &str, value: &str) -> Result<()> {
    validate_loaded_type_field(field, value)?;
    if process_ref_type_target(value).is_some() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} must be a process reference type, got {value:?}"
        )))
    }
}

fn validate_loaded_process_ref_type_target(
    program: &LoadedProgram,
    field: &str,
    value: &str,
    target_process: ProcessId,
) -> Result<()> {
    validate_loaded_process_ref_type_field(field, value)?;
    let target_name = process_ref_type_target(value)
        .expect("validate_loaded_process_ref_type_field ensures process reference type shape");
    let process = program.process(target_process)?;
    if process.debug_name != target_name {
        return Err(Error::new(format!(
            "{field} {value} targets {target_name}, expected {}",
            process.debug_name
        )));
    }
    Ok(())
}

fn process_ref_type_target(ty: &str) -> Option<&str> {
    ty.strip_prefix("ProcessRef<")
        .and_then(|value| value.strip_suffix('>'))
        .filter(|target| is_bounded_artifact_ident(target))
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

fn is_bounded_artifact_ident(value: &str) -> bool {
    value.len() <= MAX_IDENTIFIER_BYTES && is_artifact_ident(value)
}

fn is_artifact_type_ref(value: &str) -> bool {
    is_bounded_artifact_ident(value) || process_ref_type_target(value).is_some()
}

fn loaded_template_depends_on_received_payload(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } | ArtifactValueTemplate::ProcessRef { .. } => false,
        ArtifactValueTemplate::ReceivedPayload { .. } => true,
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_received_payload(&field.value)),
    }
}
