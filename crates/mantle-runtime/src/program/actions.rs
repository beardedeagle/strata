use super::templates::LoadedTemplateAdmission;
use super::*;
use mantle_artifact::ArtifactEffect;

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
    pub(super) fn effect(&self) -> ArtifactEffect {
        match self {
            Self::Emit { .. } => ArtifactEffect::Emit,
            Self::Spawn { .. } => ArtifactEffect::Spawn,
            Self::Send { .. } => ArtifactEffect::Send,
        }
    }

    pub(super) fn from_artifact(action: &ArtifactAction) -> Self {
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

    pub(super) fn validate_admission(
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

    pub(super) fn validate_admission(
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
