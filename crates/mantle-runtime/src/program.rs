use mantle_artifact::{
    ArtifactAction, ArtifactTransition, Error, MantleArtifact, MessageId, NextState, OutputId,
    ProcessId, Result, StateId, StepResult,
};

#[derive(Debug, Clone)]
pub(crate) struct LoadedProgram {
    pub(crate) format: String,
    pub(crate) format_version: String,
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
            .map(|process| {
                Ok(LoadedProcess {
                    debug_name: process.debug_name.clone(),
                    state_values: process.state_values.clone(),
                    message_variants: process.message_variants.clone(),
                    mailbox_bound: process.mailbox_bound,
                    init_state: process.init_state,
                    transitions: process
                        .transitions
                        .iter()
                        .map(LoadedTransition::from_artifact)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            format: artifact.format.clone(),
            format_version: artifact.format_version.clone(),
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
            .map(String::as_str)
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
            .map(String::as_str)
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
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcess {
    pub(crate) debug_name: String,
    pub(crate) state_values: Vec<String>,
    pub(crate) message_variants: Vec<String>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
}

impl LoadedProcess {
    pub(crate) fn transition_for_message(&self, message: MessageId) -> Result<&LoadedTransition> {
        self.transitions
            .iter()
            .find(|transition| transition.message == message)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} has no transition for message id {}",
                    self.debug_name,
                    message.as_u32()
                ))
            })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedTransition {
    pub(crate) message: MessageId,
    pub(crate) step_result: StepResult,
    pub(crate) next_state: NextState,
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Self {
        Self {
            message: transition.message,
            step_result: transition.step_result,
            next_state: transition.next_state,
            actions: transition
                .actions
                .iter()
                .map(LoadedAction::from_artifact)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadedAction {
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

impl LoadedAction {
    fn from_artifact(action: &ArtifactAction) -> Self {
        match action {
            ArtifactAction::Emit { output } => Self::Emit { output: *output },
            ArtifactAction::Spawn { target } => Self::Spawn { target: *target },
            ArtifactAction::Send { target, message } => Self::Send {
                target: *target,
                message: *message,
            },
        }
    }
}
