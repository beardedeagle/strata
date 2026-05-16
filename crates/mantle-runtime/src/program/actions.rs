use std::collections::BTreeSet;

use super::templates::LoadedTemplateAdmission;
use super::templates::validate_loaded_bool_condition;
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
        payload: Option<LoadedValueTemplate>,
    },
    IfElse {
        condition: LoadedValueTemplate,
        then_actions: Vec<LoadedAction>,
        else_actions: Vec<LoadedAction>,
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
    pub(super) fn collect_effects(&self, effects: &mut BTreeSet<ArtifactEffect>) {
        match self {
            Self::Emit { .. } => {
                effects.insert(ArtifactEffect::Emit);
            }
            Self::Spawn { .. } => {
                effects.insert(ArtifactEffect::Spawn);
            }
            Self::Send { .. } => {
                effects.insert(ArtifactEffect::Send);
            }
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                for action in then_actions {
                    action.collect_effects(effects);
                }
                for action in else_actions {
                    action.collect_effects(effects);
                }
            }
        }
    }

    fn action_count_at_depth(&self, depth: usize) -> Result<usize> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "loaded action nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        match self {
            Self::Emit { .. } | Self::Spawn { .. } | Self::Send { .. } => Ok(1),
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                let then_count = action_count_at_depth(then_actions, depth + 1)?;
                let else_count = action_count_at_depth(else_actions, depth + 1)?;
                then_count
                    .checked_add(else_count)
                    .and_then(|count| count.checked_add(1))
                    .ok_or_else(|| Error::new("loaded action_count overflowed"))
            }
        }
    }

    pub(super) fn from_artifact(action: &ArtifactAction) -> Result<Self> {
        match action {
            ArtifactAction::Emit { output } => Ok(Self::Emit { output: *output }),
            ArtifactAction::Spawn {
                target,
                process_ref,
            } => Ok(Self::Spawn {
                target: *target,
                process_ref: *process_ref,
            }),
            ArtifactAction::Send {
                target,
                message,
                payload,
            } => Ok(Self::Send {
                target: LoadedSendTarget::from_artifact(target),
                message: *message,
                payload: payload
                    .as_ref()
                    .map(LoadedValueTemplate::from_artifact)
                    .transpose()?,
            }),
            ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => Ok(Self::IfElse {
                condition: LoadedValueTemplate::from_artifact(condition)?,
                then_actions: then_actions
                    .iter()
                    .map(LoadedAction::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
                else_actions: else_actions
                    .iter()
                    .map(LoadedAction::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
            }),
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
            Self::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                validate_loaded_bool_condition(
                    program,
                    process,
                    &format!(
                        "process {} transition {} if condition",
                        process.debug_name,
                        message.as_u32()
                    ),
                    condition,
                    process.message_variants[message.index()].payload_type,
                    current_state_payload_type,
                )?;
                let mut then_refs = spawned_refs.to_vec();
                for action in then_actions {
                    action.validate_admission(
                        program,
                        process,
                        message,
                        current_state_payload_type,
                        &mut then_refs,
                    )?;
                }
                let mut else_refs = spawned_refs.to_vec();
                for action in else_actions {
                    action.validate_admission(
                        program,
                        process,
                        message,
                        current_state_payload_type,
                        &mut else_refs,
                    )?;
                }
                for (spawned, (then_spawned, else_spawned)) in spawned_refs
                    .iter_mut()
                    .zip(then_refs.into_iter().zip(else_refs))
                {
                    *spawned = *spawned || (then_spawned && else_spawned);
                }
                Ok(())
            }
        }
    }
}

pub(super) fn action_count(actions: &[LoadedAction]) -> Result<usize> {
    action_count_at_depth(actions, 0)
}

fn action_count_at_depth(actions: &[LoadedAction], depth: usize) -> Result<usize> {
    actions.iter().try_fold(0usize, |count, action| {
        count
            .checked_add(action.action_count_at_depth(depth)?)
            .ok_or_else(|| Error::new("loaded action_count overflowed"))
    })
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
