use mantle_artifact::{
    ArtifactAction, ArtifactProcess, ArtifactProcessHandle, ArtifactTransition, Error,
    MantleArtifact, MessageId, NextState, OutputId, ProcessHandleId, ProcessId, Result, StateId,
    StepResult,
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
    pub(crate) process_handles: Vec<LoadedProcessHandle>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
}

impl LoadedProcess {
    fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_values: process.state_values.clone(),
            message_variants: process.message_variants.clone(),
            process_handles: process
                .process_handles
                .iter()
                .map(LoadedProcessHandle::from_artifact)
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
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcessHandle {
    pub(crate) target: ProcessId,
}

impl LoadedProcessHandle {
    fn from_artifact(handle: &ArtifactProcessHandle) -> Self {
        Self {
            target: handle.target,
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
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Self {
        Self {
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
        handle: ProcessHandleId,
    },
    Send {
        target: ProcessHandleId,
        message: MessageId,
    },
}

impl LoadedAction {
    fn from_artifact(action: &ArtifactAction) -> Self {
        match action {
            ArtifactAction::Emit { output } => Self::Emit { output: *output },
            ArtifactAction::Spawn { target, handle } => Self::Spawn {
                target: *target,
                handle: *handle,
            },
            ArtifactAction::Send { target, message } => Self::Send {
                target: *target,
                message: *message,
            },
        }
    }
}
