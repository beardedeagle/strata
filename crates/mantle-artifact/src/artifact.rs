use std::collections::BTreeSet;

use crate::fields::ArtifactFields;
use crate::validation::{
    validate_count, validate_encoded_artifact_size, validate_ident_field, validate_output_text,
    validate_source_hash, validate_unique_ident_list, validate_unique_state_value_list,
};
use crate::{
    Error, MessageId, OutputId, ProcessHandleId, ProcessId, Result, StateId, ARTIFACT_FORMAT,
    ARTIFACT_MAGIC, ARTIFACT_SCHEMA_VERSION, MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_HANDLES_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Stop,
}

impl StepResult {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "Continue" => Ok(Self::Continue),
            "Stop" => Ok(Self::Stop),
            _ => Err(Error::new(format!("invalid step_result value {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextState {
    Current,
    Value(StateId),
}

impl NextState {
    pub(crate) fn kind_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Value(_) => "value",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub schema_version: String,
    pub source_language: String,
    pub module: String,
    pub entry_process: ProcessId,
    pub entry_message: MessageId,
    pub outputs: Vec<String>,
    pub processes: Vec<ArtifactProcess>,
    pub source_hash_fnv1a64: String,
}

impl MantleArtifact {
    pub fn encode(&self) -> String {
        let mut encoded = format!(
            "{ARTIFACT_MAGIC}\nformat={}\nschema_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\noutput_count={}\nprocess_count={}\n",
            self.format,
            self.schema_version,
            self.source_language,
            self.module,
            self.entry_process.as_u32(),
            self.entry_message.as_u32(),
            self.outputs.len(),
            self.processes.len()
        );
        for (output_index, output) in self.outputs.iter().enumerate() {
            encoded.push_str(&format!("output.{output_index}={output}\n"));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            let prefix = format!("process.{process_index}");
            encoded.push_str(&format!(
                "{prefix}.debug_name={}\n{prefix}.state_type={}\n{prefix}.state_value_count={}\n",
                process.debug_name,
                process.state_type,
                process.state_values.len()
            ));
            for (value_index, value) in process.state_values.iter().enumerate() {
                encoded.push_str(&format!("{prefix}.state_value.{value_index}={value}\n"));
            }
            encoded.push_str(&format!(
                "{prefix}.message_type={}\n{prefix}.message_count={}\n",
                process.message_type,
                process.message_variants.len()
            ));
            for (message_index, message) in process.message_variants.iter().enumerate() {
                encoded.push_str(&format!("{prefix}.message.{message_index}={message}\n"));
            }
            encoded.push_str(&format!(
                "{prefix}.handle_count={}\n",
                process.process_handles.len()
            ));
            for (handle_index, handle) in process.process_handles.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.handle.{handle_index}.debug_name={}\n{prefix}.handle.{handle_index}.target_process={}\n",
                    handle.debug_name,
                    handle.target.as_u32()
                ));
            }
            encoded.push_str(&format!(
                "{prefix}.mailbox_bound={}\n{prefix}.init_state={}\n{prefix}.transition_count={}\n",
                process.mailbox_bound,
                process.init_state.as_u32(),
                process.transitions.len()
            ));
            for (transition_index, transition) in process.transitions.iter().enumerate() {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                encoded.push_str(&format!(
                    "{transition_prefix}.message={}\n{transition_prefix}.step_result={}\n{transition_prefix}.next_state={}\n",
                    transition.message.as_u32(),
                    transition.step_result.as_str(),
                    transition.next_state.kind_str()
                ));
                if let NextState::Value(state) = transition.next_state {
                    encoded.push_str(&format!(
                        "{transition_prefix}.next_state_value={}\n",
                        state.as_u32()
                    ));
                }
                encoded.push_str(&format!(
                    "{transition_prefix}.action_count={}\n",
                    transition.actions.len()
                ));
                for (action_index, action) in transition.actions.iter().enumerate() {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    encode_action(&mut encoded, &action_prefix, action);
                }
            }
        }

        encoded.push_str(&format!(
            "source_hash_fnv1a64={}\n",
            self.source_hash_fnv1a64
        ));
        encoded
    }

    pub fn decode(contents: &str) -> Result<Self> {
        let mut fields = ArtifactFields::parse(contents)?;
        let format = fields.take_required("format")?;
        let schema_version = fields.take_required("schema_version")?;
        validate_artifact_identity(&format, &schema_version)?;

        let process_count = fields.take_bounded_usize("process_count", 1, MAX_PROCESS_COUNT)?;
        let output_count = fields.take_bounded_usize("output_count", 0, MAX_OUTPUT_LITERALS)?;
        let mut outputs = Vec::with_capacity(output_count);
        for output_index in 0..output_count {
            outputs.push(fields.take_required(&format!("output.{output_index}"))?);
        }

        let mut processes = Vec::with_capacity(process_count);
        for process_index in 0..process_count {
            let prefix = format!("process.{process_index}");
            let state_value_count = fields.take_bounded_usize(
                &format!("{prefix}.state_value_count"),
                1,
                MAX_STATE_VALUES_PER_PROCESS,
            )?;
            let mut state_values = Vec::with_capacity(state_value_count);
            for value_index in 0..state_value_count {
                state_values
                    .push(fields.take_required(&format!("{prefix}.state_value.{value_index}"))?);
            }

            let message_count = fields.take_bounded_usize(
                &format!("{prefix}.message_count"),
                1,
                MAX_MESSAGE_VARIANTS_PER_PROCESS,
            )?;
            let mut message_variants = Vec::with_capacity(message_count);
            for message_index in 0..message_count {
                message_variants
                    .push(fields.take_required(&format!("{prefix}.message.{message_index}"))?);
            }

            let handle_count = fields.take_bounded_usize(
                &format!("{prefix}.handle_count"),
                0,
                MAX_PROCESS_HANDLES_PER_PROCESS,
            )?;
            let mut process_handles = Vec::with_capacity(handle_count);
            for handle_index in 0..handle_count {
                let handle_prefix = format!("{prefix}.handle.{handle_index}");
                process_handles.push(ArtifactProcessHandle {
                    debug_name: fields.take_required(&format!("{handle_prefix}.debug_name"))?,
                    target: fields.take_process_id(&format!("{handle_prefix}.target_process"))?,
                });
            }

            let transition_count = fields.take_bounded_usize(
                &format!("{prefix}.transition_count"),
                1,
                MAX_TRANSITIONS_PER_PROCESS,
            )?;
            let mut transitions = Vec::with_capacity(transition_count);
            for transition_index in 0..transition_count {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                let action_count = fields.take_bounded_usize(
                    &format!("{transition_prefix}.action_count"),
                    0,
                    MAX_ACTIONS_PER_PROCESS,
                )?;
                let mut actions = Vec::with_capacity(action_count);
                for action_index in 0..action_count {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    actions.push(decode_action(&mut fields, &action_prefix)?);
                }

                transitions.push(ArtifactTransition {
                    message: fields.take_message_id(&format!("{transition_prefix}.message"))?,
                    step_result: fields
                        .take_step_result(&format!("{transition_prefix}.step_result"))?,
                    next_state: fields.take_next_state(&transition_prefix)?,
                    actions,
                });
            }

            processes.push(ArtifactProcess {
                debug_name: fields.take_required(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_required(&format!("{prefix}.state_type"))?,
                state_values,
                message_type: fields.take_required(&format!("{prefix}.message_type"))?,
                message_variants,
                process_handles,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_state_id(&format!("{prefix}.init_state"))?,
                transitions,
            });
        }

        let artifact = Self {
            format,
            schema_version,
            source_language: fields.take_required("source_language")?,
            module: fields.take_required("module")?,
            entry_process: fields.take_process_id("entry_process")?,
            entry_message: fields.take_message_id("entry_message")?,
            outputs,
            processes,
            source_hash_fnv1a64: fields.take_required("source_hash_fnv1a64")?,
        };

        fields.finish()?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<()> {
        validate_artifact_identity(&self.format, &self.schema_version)?;
        validate_ident_field("source_language", &self.source_language)?;
        validate_ident_field("module", &self.module)?;
        validate_source_hash(&self.source_hash_fnv1a64)?;
        validate_count("process_count", self.processes.len(), 1, MAX_PROCESS_COUNT)?;
        validate_count("output_count", self.outputs.len(), 0, MAX_OUTPUT_LITERALS)?;
        for output in &self.outputs {
            validate_output_text(output)?;
        }

        let mut process_debug_names = BTreeSet::new();
        for process in &self.processes {
            process.validate_identity()?;
            if !process_debug_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate process debug_name {}",
                    process.debug_name
                )));
            }
        }

        let Some(entry_process) = self.processes.get(self.entry_process.index()) else {
            return Err(Error::new(format!(
                "entry process id {} is not defined",
                self.entry_process.as_u32()
            )));
        };
        if self.entry_message.index() >= entry_process.message_variants.len() {
            return Err(Error::new(format!(
                "entry message id {} is not accepted by process id {}",
                self.entry_message.as_u32(),
                self.entry_process.as_u32()
            )));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            process.validate_references(self, ProcessId::from_index(process_index)?)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }
}

fn validate_artifact_identity(format: &str, schema_version: &str) -> Result<()> {
    if format != ARTIFACT_FORMAT {
        return Err(Error::new(format!(
            "unsupported artifact format {format}; expected {ARTIFACT_FORMAT}"
        )));
    }
    if schema_version != ARTIFACT_SCHEMA_VERSION {
        return Err(Error::new(format!(
            "unsupported artifact schema version {schema_version}; expected {ARTIFACT_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn validate_unique_process_handle_list(handles: &[ArtifactProcessHandle]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for handle in handles {
        validate_ident_field("process handle", &handle.debug_name)?;
        if !seen.insert(handle.debug_name.as_str()) {
            return Err(Error::new(format!(
                "duplicate process handle {}",
                handle.debug_name
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub debug_name: String,
    pub state_type: String,
    pub state_values: Vec<String>,
    pub message_type: String,
    pub message_variants: Vec<String>,
    pub process_handles: Vec<ArtifactProcessHandle>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub transitions: Vec<ArtifactTransition>,
}

impl ArtifactProcess {
    fn validate_identity(&self) -> Result<()> {
        validate_ident_field("process debug_name", &self.debug_name)?;
        validate_ident_field("state_type", &self.state_type)?;
        validate_ident_field("message_type", &self.message_type)?;
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
            "handle_count",
            self.process_handles.len(),
            0,
            MAX_PROCESS_HANDLES_PER_PROCESS,
        )?;
        validate_count(
            "transition_count",
            self.transitions.len(),
            1,
            MAX_TRANSITIONS_PER_PROCESS,
        )?;
        validate_unique_state_value_list(&self.state_values)?;
        validate_unique_ident_list("message variant", &self.message_variants)?;
        validate_unique_process_handle_list(&self.process_handles)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.transitions.len() != self.message_variants.len() {
            return Err(Error::new(format!(
                "process {} transition_count must equal message_count",
                self.debug_name
            )));
        }
        let mut transition_messages = BTreeSet::new();
        let mut action_count = 0usize;
        for transition in &self.transitions {
            if !transition_messages.insert(transition.message.as_u32()) {
                return Err(Error::new(format!(
                    "process {} declares duplicate transition for message id {}",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            if transition.message.index() >= self.message_variants.len() {
                return Err(Error::new(format!(
                    "process {} transition message id {} is not accepted",
                    self.debug_name,
                    transition.message.as_u32()
                )));
            }
            if let NextState::Value(state) = transition.next_state {
                if state.index() >= self.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} transition next_state id {} is not a valid state value",
                        self.debug_name,
                        state.as_u32()
                    )));
                }
            }
            action_count = action_count
                .checked_add(transition.actions.len())
                .ok_or_else(|| Error::new("process action_count overflowed"))?;
        }
        validate_count("action_count", action_count, 0, MAX_ACTIONS_PER_PROCESS)?;
        for message_index in 0..self.message_variants.len() {
            if !transition_messages.contains(&(message_index as u32)) {
                return Err(Error::new(format!(
                    "process {} has no transition for message id {}",
                    self.debug_name, message_index
                )));
            }
        }
        Ok(())
    }

    fn validate_references(&self, artifact: &MantleArtifact, process_id: ProcessId) -> Result<()> {
        for handle in &self.process_handles {
            if handle.target.index() >= artifact.processes.len() {
                return Err(Error::new(format!(
                    "process {} handle {} targets undefined process id {}",
                    self.debug_name,
                    handle.debug_name,
                    handle.target.as_u32()
                )));
            }
            if handle.target == artifact.entry_process {
                return Err(Error::new(format!(
                    "process {} handle {} targets entry process id {}",
                    self.debug_name,
                    handle.debug_name,
                    handle.target.as_u32()
                )));
            }
            if handle.target == process_id {
                return Err(Error::new(format!(
                    "process {} handle {} targets itself, which is not supported",
                    self.debug_name, handle.debug_name
                )));
            }
        }
        for transition in &self.transitions {
            let mut spawned_handles = BTreeSet::new();
            for action in &transition.actions {
                self.validate_action_reference(artifact, transition, &mut spawned_handles, action)?;
            }
        }
        Ok(())
    }

    fn validate_action_reference(
        &self,
        artifact: &MantleArtifact,
        transition: &ArtifactTransition,
        spawned_handles: &mut BTreeSet<ProcessHandleId>,
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
            ArtifactAction::Spawn { target, handle } => {
                let declared_target = self.process_handle_target(*handle)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn handle id {} targets process id {}, expected {}",
                        self.debug_name,
                        handle.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                if !spawned_handles.insert(*handle) {
                    return Err(Error::new(format!(
                        "process {} duplicates process handle id {} within message transition {}",
                        self.debug_name,
                        handle.as_u32(),
                        transition.message.as_u32()
                    )));
                }
            }
            ArtifactAction::Send { target, message } => {
                let target_process_id = self.process_handle_target(*target)?;
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
            }
        }
        Ok(())
    }

    fn process_handle_target(&self, handle: ProcessHandleId) -> Result<ProcessId> {
        self.process_handles
            .get(handle.index())
            .map(|process_handle| process_handle.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process handle id {}",
                    self.debug_name,
                    handle.as_u32()
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcessHandle {
    pub debug_name: String,
    pub target: ProcessId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTransition {
    pub message: MessageId,
    pub step_result: StepResult,
    pub next_state: NextState,
    pub actions: Vec<ArtifactAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        handle: ProcessHandleId,
    },
    Send {
        target: ProcessHandleId,
        message: MessageId,
    },
}

fn encode_action(encoded: &mut String, action_prefix: &str, action: &ArtifactAction) {
    match action {
        ArtifactAction::Emit { output } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=emit\n{action_prefix}.output={}\n",
                output.as_u32()
            ));
        }
        ArtifactAction::Spawn { target, handle } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n{action_prefix}.handle={}\n",
                target.as_u32(),
                handle.as_u32()
            ));
        }
        ArtifactAction::Send { target, message } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=send\n{action_prefix}.target_handle={}\n{action_prefix}.message={}\n",
                target.as_u32(),
                message.as_u32()
            ));
        }
    }
}

fn decode_action(fields: &mut ArtifactFields, action_prefix: &str) -> Result<ArtifactAction> {
    let kind = fields.take_required(&format!("{action_prefix}.kind"))?;
    match kind.as_str() {
        "emit" => Ok(ArtifactAction::Emit {
            output: fields.take_output_id(&format!("{action_prefix}.output"))?,
        }),
        "spawn" => Ok(ArtifactAction::Spawn {
            target: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
            handle: fields.take_process_handle_id(&format!("{action_prefix}.handle"))?,
        }),
        "send" => Ok(ArtifactAction::Send {
            target: fields.take_process_handle_id(&format!("{action_prefix}.target_handle"))?,
            message: fields.take_message_id(&format!("{action_prefix}.message"))?,
        }),
        _ => Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
    }
}
