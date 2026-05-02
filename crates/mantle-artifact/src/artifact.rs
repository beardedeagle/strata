use crate::fields::ArtifactFields;
use crate::validation::{
    validate_count, validate_encoded_artifact_size, validate_ident_field, validate_output_text,
    validate_source_hash, validate_unique_ident_list, validate_unique_state_value_list,
};
use crate::{
    Error, MessageId, OutputId, ProcessId, Result, StateId, ARTIFACT_FORMAT, ARTIFACT_MAGIC,
    ARTIFACT_VERSION, MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT, MAX_STATE_VALUES_PER_PROCESS,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub format_version: String,
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
            "{ARTIFACT_MAGIC}\nformat={}\nformat_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\noutput_count={}\nprocess_count={}\n",
            self.format,
            self.format_version,
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
                "{prefix}.mailbox_bound={}\n{prefix}.init_state={}\n{prefix}.step_result={}\n{prefix}.final_state={}\n{prefix}.action_count={}\n",
                process.mailbox_bound,
                process.init_state.as_u32(),
                process.step_result.as_str(),
                process.final_state.as_u32(),
                process.actions.len()
            ));
            for (action_index, action) in process.actions.iter().enumerate() {
                let action_prefix = format!("{prefix}.action.{action_index}");
                match action {
                    ArtifactAction::Emit { output } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=emit\n{action_prefix}.output={}\n",
                            output.as_u32()
                        ));
                    }
                    ArtifactAction::Spawn { target } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n",
                            target.as_u32()
                        ));
                    }
                    ArtifactAction::Send { target, message } => {
                        encoded.push_str(&format!(
                            "{action_prefix}.kind=send\n{action_prefix}.target_process={}\n{action_prefix}.message={}\n",
                            target.as_u32(),
                            message.as_u32()
                        ));
                    }
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

            let action_count = fields.take_bounded_usize(
                &format!("{prefix}.action_count"),
                0,
                MAX_ACTIONS_PER_PROCESS,
            )?;
            let mut actions = Vec::with_capacity(action_count);
            for action_index in 0..action_count {
                let action_prefix = format!("{prefix}.action.{action_index}");
                let kind = fields.take_required(&format!("{action_prefix}.kind"))?;
                let action = match kind.as_str() {
                    "emit" => ArtifactAction::Emit {
                        output: fields.take_output_id(&format!("{action_prefix}.output"))?,
                    },
                    "spawn" => ArtifactAction::Spawn {
                        target: fields
                            .take_process_id(&format!("{action_prefix}.target_process"))?,
                    },
                    "send" => ArtifactAction::Send {
                        target: fields
                            .take_process_id(&format!("{action_prefix}.target_process"))?,
                        message: fields.take_message_id(&format!("{action_prefix}.message"))?,
                    },
                    _ => return Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
                };
                actions.push(action);
            }

            processes.push(ArtifactProcess {
                debug_name: fields.take_required(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_required(&format!("{prefix}.state_type"))?,
                state_values,
                message_type: fields.take_required(&format!("{prefix}.message_type"))?,
                message_variants,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_state_id(&format!("{prefix}.init_state"))?,
                step_result: fields.take_step_result(&format!("{prefix}.step_result"))?,
                final_state: fields.take_state_id(&format!("{prefix}.final_state"))?,
                actions,
            });
        }

        let artifact = Self {
            format: fields.take_required("format")?,
            format_version: fields.take_required("format_version")?,
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
        if self.format != ARTIFACT_FORMAT {
            return Err(Error::new(format!(
                "unsupported artifact format {}; expected {}",
                self.format, ARTIFACT_FORMAT
            )));
        }
        if self.format_version != ARTIFACT_VERSION {
            return Err(Error::new(format!(
                "unsupported artifact version {}; expected {}",
                self.format_version, ARTIFACT_VERSION
            )));
        }
        validate_ident_field("source_language", &self.source_language)?;
        validate_ident_field("module", &self.module)?;
        validate_source_hash(&self.source_hash_fnv1a64)?;
        validate_count("process_count", self.processes.len(), 1, MAX_PROCESS_COUNT)?;
        validate_count("output_count", self.outputs.len(), 0, MAX_OUTPUT_LITERALS)?;
        for output in &self.outputs {
            validate_output_text(output)?;
        }

        let mut process_debug_names = std::collections::BTreeSet::new();
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

        for process in &self.processes {
            process.validate_references(self)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub debug_name: String,
    pub state_type: String,
    pub state_values: Vec<String>,
    pub message_type: String,
    pub message_variants: Vec<String>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub step_result: StepResult,
    pub final_state: StateId,
    pub actions: Vec<ArtifactAction>,
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
            "action_count",
            self.actions.len(),
            0,
            MAX_ACTIONS_PER_PROCESS,
        )?;
        validate_unique_state_value_list(&self.state_values)?;
        validate_unique_ident_list("message variant", &self.message_variants)?;
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a valid state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.final_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} final_state id {} is not a valid state value",
                self.debug_name,
                self.final_state.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_references(&self, artifact: &MantleArtifact) -> Result<()> {
        for action in &self.actions {
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
                ArtifactAction::Spawn { target } => {
                    if target.index() >= artifact.processes.len() {
                        return Err(Error::new(format!(
                            "process {} spawns undefined process id {}",
                            self.debug_name,
                            target.as_u32()
                        )));
                    }
                }
                ArtifactAction::Send { target, message } => {
                    let Some(target_process) = artifact.processes.get(target.index()) else {
                        return Err(Error::new(format!(
                            "process {} sends to undefined process id {}",
                            self.debug_name,
                            target.as_u32()
                        )));
                    };
                    if message.index() >= target_process.message_variants.len() {
                        return Err(Error::new(format!(
                            "process {} sends message id {} not accepted by process id {}",
                            self.debug_name,
                            message.as_u32(),
                            target.as_u32()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
    },
    Send {
        target: ProcessId,
        message: MessageId,
    },
}
