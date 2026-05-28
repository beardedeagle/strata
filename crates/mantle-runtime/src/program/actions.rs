use std::collections::BTreeSet;

use super::*;
use mantle_artifact::ArtifactEffect;

mod admission;

pub(in crate::program) use admission::ActionAdmissionContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        process_ref: ProcessRefId,
        spawn_site: SpawnSiteId,
    },
    SpawnOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
        target: ProcessId,
        spawn_site: SpawnSiteId,
    },
    Send {
        target: LoadedSendTarget,
        message: MessageId,
        payload: Option<LoadedValueTemplate>,
    },
    SendOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
        target: LoadedSendTarget,
        message: MessageId,
        payload: Option<LoadedValueTemplate>,
    },
    IfElse {
        condition: LoadedValueTemplate,
        then_actions: Vec<LoadedAction>,
        else_actions: Vec<LoadedAction>,
    },
    ForEach {
        element: LoadedLoopElement,
        collection: LoadedValueTemplate,
        max_items: usize,
        body: Vec<LoadedAction>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedLoopElement {
    pub(crate) id: LoopElementId,
    pub(crate) ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedSendTarget {
    ProcessRef(ProcessRefId),
    SupervisorChild {
        supervisor: SupervisorId,
        child: SupervisorChildId,
        target_process: ProcessId,
    },
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
            Self::Spawn { .. } | Self::SpawnOutcome { .. } => {
                effects.insert(ArtifactEffect::Spawn);
            }
            Self::Send { .. } | Self::SendOutcome { .. } => {
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
            Self::ForEach { body, .. } => {
                for action in body {
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
            Self::Emit { .. }
            | Self::Spawn { .. }
            | Self::SpawnOutcome { .. }
            | Self::Send { .. }
            | Self::SendOutcome { .. } => Ok(1),
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
            Self::ForEach { body, .. } => action_count_at_depth(body, depth + 1)?
                .checked_add(1)
                .ok_or_else(|| Error::new("loaded action_count overflowed")),
        }
    }

    pub(super) fn from_artifact(action: &ArtifactAction) -> Result<Self> {
        match action {
            ArtifactAction::Emit { output } => Ok(Self::Emit { output: *output }),
            ArtifactAction::Spawn {
                target,
                process_ref,
                spawn_site,
            } => Ok(Self::Spawn {
                target: *target,
                process_ref: *process_ref,
                spawn_site: *spawn_site,
            }),
            ArtifactAction::SpawnOutcome {
                outcome,
                outcome_ty,
                target,
                spawn_site,
            } => Ok(Self::SpawnOutcome {
                outcome: *outcome,
                outcome_ty: *outcome_ty,
                target: *target,
                spawn_site: *spawn_site,
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
            ArtifactAction::SendOutcome {
                outcome,
                outcome_ty,
                target,
                message,
                payload,
            } => Ok(Self::SendOutcome {
                outcome: *outcome,
                outcome_ty: *outcome_ty,
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
            ArtifactAction::ForEach {
                element,
                collection,
                max_items,
                body,
            } => Ok(Self::ForEach {
                element: LoadedLoopElement {
                    id: element.id,
                    ty: element.ty,
                },
                collection: LoadedValueTemplate::from_artifact(collection)?,
                max_items: *max_items,
                body: body
                    .iter()
                    .map(LoadedAction::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
            }),
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
            ArtifactSendTarget::SupervisorChild {
                supervisor,
                child,
                target_process,
            } => Self::SupervisorChild {
                supervisor: *supervisor,
                child: *child,
                target_process: *target_process,
            },
            ArtifactSendTarget::ReceivedPayload { ty, target_process } => Self::ReceivedPayload {
                ty: *ty,
                target_process: *target_process,
            },
        }
    }
}
