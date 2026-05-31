use std::cmp::Ordering;

use mantle_artifact::{
    AuthorityId, EffectOutcomeId, Error, MessageId, OutputId, PortId, ProcessId, ProcessRefId,
    Result, SpawnSiteId, StateId, StepResult, SupervisorChildId, SupervisorId, TypeId,
};

use crate::program::{
    LoadedAction, LoadedLoopElement, LoadedNextState, LoadedProcess, LoadedProgram,
    LoadedSendTarget, LoadedSpawnKind, LoadedValueTemplate, RuntimePayload,
};

mod compact;
use compact::{CompactList, CompactListBuilder};
mod dispatch;
use dispatch::ExecutableDispatchTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ExecutableTransitionId(u32);

impl ExecutableTransitionId {
    fn from_index(index: usize) -> Result<Self> {
        let id = u32::try_from(index)
            .map_err(|_| Error::new(format!("executable transition index {index} exceeds u32")))?;
        Ok(Self(id))
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
pub(crate) struct ExecutableProgram<'program> {
    processes: CompactList<ExecutableProcess<'program>>,
    actions: CompactList<ExecutableActionPlan<'program>>,
    entry: ExecutableEntry<'program>,
}

impl<'program> ExecutableProgram<'program> {
    pub(crate) fn from_admitted(loaded: &'program LoadedProgram) -> Result<Self> {
        loaded.validate_admission()?;

        let mut builder = ExecutablePlanBuilder::new(loaded);
        let mut processes = CompactListBuilder::with_expected_len(loaded.processes.len());
        for process in &loaded.processes {
            processes.push(ExecutableProcess::from_loaded(&mut builder, process)?);
        }
        let entry_process = loaded
            .processes
            .get(loaded.entry_process.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "executable entry process id {} is not loaded",
                    loaded.entry_process.as_u32()
                ))
            })?;
        let entry_message = entry_process
            .message_variants
            .get(loaded.entry_message.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "executable entry message id {} is not loaded for process id {}",
                    loaded.entry_message.as_u32(),
                    loaded.entry_process.as_u32()
                ))
            })?;

        Ok(Self {
            processes: processes.finish(),
            actions: builder.finish_actions(),
            entry: ExecutableEntry {
                process_id: loaded.entry_process,
                process_label: entry_process.debug_name.as_str(),
                message_id: loaded.entry_message,
                message_label: entry_message.label.as_str(),
            },
        })
    }

    pub(crate) const fn entry(&self) -> ExecutableEntry<'program> {
        self.entry
    }

    pub(crate) fn process(&self, id: ProcessId) -> Result<&ExecutableProcess<'program>> {
        self.processes.get(id.index()).ok_or_else(|| {
            Error::new(format!(
                "executable process id {} is not loaded",
                id.as_u32()
            ))
        })
    }

    pub(crate) fn transition_for_dispatch(
        &self,
        process_id: ProcessId,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Result<&ExecutableTransition<'program>> {
        self.process(process_id)?
            .transition_for_dispatch(message, current_state, payload)
    }

    pub(crate) fn process_count(&self) -> usize {
        self.processes.as_slice().len()
    }

    pub(crate) fn actions(&self) -> &[ExecutableActionPlan<'program>] {
        self.actions.as_slice()
    }

    #[cfg(test)]
    pub(crate) fn transition_signature(&self) -> Vec<(u32, u32, u32, Option<u32>)> {
        self.processes
            .iter()
            .enumerate()
            .flat_map(|(process_index, process)| {
                process.transitions.iter().map(move |transition| {
                    (
                        process_index as u32,
                        transition.id.as_u32(),
                        transition.message.as_u32(),
                        transition.current_state.map(StateId::as_u32),
                    )
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct ExecutablePlanBuilder<'program> {
    actions: CompactListBuilder<ExecutableActionPlan<'program>>,
    action_count: usize,
}

impl<'program> ExecutablePlanBuilder<'program> {
    fn new(loaded: &'program LoadedProgram) -> Self {
        Self::with_action_capacity(count_loaded_actions(loaded))
    }

    fn with_action_capacity(action_capacity: usize) -> Self {
        Self {
            actions: CompactListBuilder::with_expected_len(action_capacity),
            action_count: 0,
        }
    }

    fn append_actions(&mut self, actions: CompactListBuilder<ExecutableActionPlan<'program>>) {
        self.actions.append_from(actions);
    }

    fn finish_actions(self) -> CompactList<ExecutableActionPlan<'program>> {
        self.actions.finish()
    }
}

fn count_loaded_actions(loaded: &LoadedProgram) -> usize {
    loaded
        .processes
        .iter()
        .flat_map(|process| &process.transitions)
        .map(|transition| count_loaded_action_block(&transition.actions))
        .sum()
}

fn count_loaded_action_block(actions: &[LoadedAction]) -> usize {
    actions
        .iter()
        .map(|action| {
            1 + match action {
                LoadedAction::IfElse {
                    then_actions,
                    else_actions,
                    ..
                } => {
                    count_loaded_action_block(then_actions)
                        + count_loaded_action_block(else_actions)
                }
                LoadedAction::ForEach { body, .. } => count_loaded_action_block(body),
                LoadedAction::Emit { .. }
                | LoadedAction::Spawn { .. }
                | LoadedAction::SpawnOutcome { .. }
                | LoadedAction::Send { .. }
                | LoadedAction::SendOutcome { .. } => 0,
            }
        })
        .sum()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutableEntry<'program> {
    pub(crate) process_id: ProcessId,
    pub(crate) process_label: &'program str,
    pub(crate) message_id: MessageId,
    pub(crate) message_label: &'program str,
}

#[derive(Debug)]
pub(crate) struct ExecutableProcess<'program> {
    loaded: &'program LoadedProcess,
    transitions: CompactList<ExecutableTransition<'program>>,
    dispatch: ExecutableDispatchTable,
}

impl<'program> ExecutableProcess<'program> {
    fn from_loaded(
        builder: &mut ExecutablePlanBuilder<'program>,
        loaded: &'program LoadedProcess,
    ) -> Result<Self> {
        let mut transitions = CompactListBuilder::with_expected_len(loaded.transitions.len());
        for (index, transition) in loaded.transitions.iter().enumerate() {
            transitions.push(ExecutableTransition::from_loaded(
                builder,
                ExecutableTransitionId::from_index(index)?,
                loaded,
                transition,
            )?);
        }
        let mut transitions = transitions.finish();
        transitions.as_mut_slice().sort_by(compare_transition_order);
        for (index, transition) in transitions.as_mut_slice().iter_mut().enumerate() {
            transition.id = ExecutableTransitionId::from_index(index)?;
        }
        let dispatch = ExecutableDispatchTable::from_transitions(transitions.as_slice());

        Ok(Self {
            loaded,
            transitions,
            dispatch,
        })
    }

    pub(crate) fn process_ref_count(&self) -> usize {
        self.loaded.process_refs.len()
    }

    pub(crate) fn transition_for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Result<&ExecutableTransition<'program>> {
        let lookup_state = self
            .dispatch
            .is_state_specific_message(message)
            .then_some(current_state);
        let payload_specific = self
            .dispatch
            .is_payload_specific_base(message, lookup_state);
        let transition_id = self
            .dispatch
            .for_dispatch(message, current_state, payload, self.transitions.as_slice())
            .ok_or_else(|| {
                self.transition_lookup_error(message, lookup_state, payload_specific, payload)
            })?;
        self.transitions.get(transition_id.index()).ok_or_else(|| {
            Error::new(format!(
                "process {} executable transition id {} is not loaded",
                self.loaded.debug_name,
                transition_id.as_u32()
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
                    "process {} has no executable transition for message id {}{} payload {}",
                    self.loaded.debug_name,
                    message.as_u32(),
                    state,
                    payload.label()
                )),
                None => Error::new(format!(
                    "process {} has payload-specific executable transition(s) for message id {}{}, but the queued message has no payload",
                    self.loaded.debug_name,
                    message.as_u32(),
                    state
                )),
            };
        }
        Error::new(format!(
            "process {} has no executable transition for message id {}{}",
            self.loaded.debug_name,
            message.as_u32(),
            state
        ))
    }
}

fn compare_transition_order(
    left: &ExecutableTransition<'_>,
    right: &ExecutableTransition<'_>,
) -> Ordering {
    left.message
        .cmp(&right.message)
        .then_with(|| left.current_state.cmp(&right.current_state))
        .then_with(|| compare_payload_guard(left.payload_guard, right.payload_guard))
}

fn compare_payload_guard(
    left: Option<&RuntimePayload>,
    right: Option<&RuntimePayload>,
) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left
            .ty
            .cmp(&right.ty)
            .then_with(|| left.value.cmp(&right.value)),
    }
}

#[derive(Debug)]
pub(crate) struct ExecutableTransition<'program> {
    id: ExecutableTransitionId,
    current_state: Option<StateId>,
    message: MessageId,
    payload_guard: Option<&'program RuntimePayload>,
    step_result: StepResult,
    next_state: &'program LoadedNextState,
    actions: ExecutableActionBlock<'program>,
}

impl<'program> ExecutableTransition<'program> {
    fn from_loaded(
        builder: &mut ExecutablePlanBuilder<'program>,
        id: ExecutableTransitionId,
        process: &LoadedProcess,
        loaded: &'program crate::program::LoadedTransition,
    ) -> Result<Self> {
        Ok(Self {
            id,
            current_state: loaded.current_state,
            message: loaded.message,
            payload_guard: loaded.payload_guard.as_ref(),
            step_result: loaded.step_result,
            next_state: &loaded.next_state,
            actions: ExecutableActionBlock::from_loaded(builder, process, &loaded.actions)?,
        })
    }

    pub(crate) const fn step_result(&self) -> StepResult {
        self.step_result
    }

    pub(crate) const fn next_state(&self) -> &'program LoadedNextState {
        self.next_state
    }

    pub(crate) const fn actions(&self) -> ExecutableActionBlock<'program> {
        self.actions
    }

    fn payload_matches(&self, payload: &RuntimePayload) -> bool {
        self.payload_guard
            .is_some_and(|guard| guard.ty == payload.ty && guard.value == payload.value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutableActionBlock<'program> {
    start: u32,
    len: u32,
    prestate_prefix_len: u32,
    _marker: std::marker::PhantomData<&'program ()>,
}

impl<'program> ExecutableActionBlock<'program> {
    fn from_loaded(
        builder: &mut ExecutablePlanBuilder<'program>,
        process: &LoadedProcess,
        actions: &'program [LoadedAction],
    ) -> Result<Self> {
        let mut prestate_prefix_open = true;
        let mut prestate_prefix_len = 0usize;
        let mut plans = CompactListBuilder::with_expected_len(actions.len());

        for (index, action) in actions.iter().enumerate() {
            let plan = ExecutableActionPlan::from_loaded(builder, process, action)?;
            if prestate_prefix_open && plan.is_prestate_prefix_action() {
                prestate_prefix_len = index.saturating_add(1);
            } else {
                prestate_prefix_open = false;
                if plan.is_effect_outcome_action() {
                    return Err(Error::new(format!(
                        "process {} executable effect outcome action appears after ordinary effects",
                        process.debug_name
                    )));
                }
            }
            plans.push(plan);
        }

        let start = u32::try_from(builder.action_count).map_err(|_| {
            Error::new(format!(
                "process {} executable action start exceeds u32",
                process.debug_name
            ))
        })?;
        let len = u32::try_from(actions.len()).map_err(|_| {
            Error::new(format!(
                "process {} executable action count exceeds u32",
                process.debug_name
            ))
        })?;
        let prestate_prefix_len = u32::try_from(prestate_prefix_len).map_err(|_| {
            Error::new(format!(
                "process {} executable prestate action count exceeds u32",
                process.debug_name
            ))
        })?;
        builder.action_count = builder
            .action_count
            .checked_add(actions.len())
            .ok_or_else(|| Error::new("executable action count overflowed"))?;
        builder.append_actions(plans);

        Ok(Self {
            start,
            len,
            prestate_prefix_len,
            _marker: std::marker::PhantomData,
        })
    }

    pub(crate) fn prestate_actions<'actions>(
        self,
        actions: &'actions [ExecutableActionPlan<'program>],
    ) -> &'actions [ExecutableActionPlan<'program>] {
        let start = self.start as usize;
        let end = start + self.prestate_prefix_len as usize;
        &actions[start..end]
    }

    pub(crate) fn poststate_actions<'actions>(
        self,
        actions: &'actions [ExecutableActionPlan<'program>],
    ) -> impl Iterator<Item = (usize, &'actions ExecutableActionPlan<'program>)> {
        let start = self.start as usize;
        let block_len = self.len as usize;
        let prestate_prefix_len = self.prestate_prefix_len as usize;
        actions[start..start + block_len]
            .iter()
            .enumerate()
            .skip(prestate_prefix_len)
    }

    pub(crate) fn all_actions<'actions>(
        self,
        actions: &'actions [ExecutableActionPlan<'program>],
    ) -> impl Iterator<Item = (usize, &'actions ExecutableActionPlan<'program>)> {
        let start = self.start as usize;
        let block_len = self.len as usize;
        actions[start..start + block_len].iter().enumerate()
    }
}

#[derive(Debug)]
pub(crate) enum ExecutableActionPlan<'program> {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        process_ref: ExecutableProcessRef,
        spawn: ExecutableSpawnSite,
    },
    SpawnOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
        target: ProcessId,
        spawn: ExecutableSpawnSite,
    },
    Send {
        target: ExecutableSendTarget,
        port: Option<PortId>,
        message: MessageId,
        payload: Option<&'program LoadedValueTemplate>,
    },
    SendOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
        target: ExecutableSendTarget,
        port: Option<PortId>,
        message: MessageId,
        payload: Option<&'program LoadedValueTemplate>,
    },
    IfElse {
        condition: &'program LoadedValueTemplate,
        then_actions: ExecutableActionBlock<'program>,
        else_actions: ExecutableActionBlock<'program>,
    },
    ForEach {
        element: LoadedLoopElement,
        collection: &'program LoadedValueTemplate,
        max_items: usize,
        body: ExecutableActionBlock<'program>,
    },
}

impl<'program> ExecutableActionPlan<'program> {
    fn from_loaded(
        builder: &mut ExecutablePlanBuilder<'program>,
        process: &LoadedProcess,
        action: &'program LoadedAction,
    ) -> Result<Self> {
        match action {
            LoadedAction::Emit { output } => Ok(Self::Emit { output: *output }),
            LoadedAction::Spawn {
                target,
                process_ref,
                spawn_site,
            } => Ok(Self::Spawn {
                target: *target,
                process_ref: executable_process_ref(process, *process_ref)?,
                spawn: executable_spawn_site(process, *spawn_site, *target)?,
            }),
            LoadedAction::SpawnOutcome {
                outcome,
                outcome_ty,
                target,
                spawn_site,
            } => Ok(Self::SpawnOutcome {
                outcome: *outcome,
                outcome_ty: *outcome_ty,
                target: *target,
                spawn: executable_spawn_site(process, *spawn_site, *target)?,
            }),
            LoadedAction::Send {
                target,
                port,
                message,
                payload,
            } => Ok(Self::Send {
                target: ExecutableSendTarget::from_loaded(process, target)?,
                port: *port,
                message: *message,
                payload: payload.as_ref(),
            }),
            LoadedAction::SendOutcome {
                outcome,
                outcome_ty,
                target,
                port,
                message,
                payload,
            } => Ok(Self::SendOutcome {
                outcome: *outcome,
                outcome_ty: *outcome_ty,
                target: ExecutableSendTarget::from_loaded(process, target)?,
                port: *port,
                message: *message,
                payload: payload.as_ref(),
            }),
            LoadedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => Ok(Self::IfElse {
                condition,
                then_actions: ExecutableActionBlock::from_loaded(builder, process, then_actions)?,
                else_actions: ExecutableActionBlock::from_loaded(builder, process, else_actions)?,
            }),
            LoadedAction::ForEach {
                element,
                collection,
                max_items,
                body,
            } => Ok(Self::ForEach {
                element: element.clone(),
                collection,
                max_items: *max_items,
                body: ExecutableActionBlock::from_loaded(builder, process, body)?,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_loaded_for_test(
        process: &LoadedProcess,
        action: &'program LoadedAction,
    ) -> Result<ExecutableTestActionPlan<'program>> {
        let mut builder = ExecutablePlanBuilder::with_action_capacity(count_loaded_action_block(
            std::slice::from_ref(action),
        ));
        let action = Self::from_loaded(&mut builder, process, action)?;
        Ok(ExecutableTestActionPlan {
            action,
            actions: builder.finish_actions(),
        })
    }

    const fn is_prestate_prefix_action(&self) -> bool {
        matches!(
            self,
            Self::Spawn { .. } | Self::SpawnOutcome { .. } | Self::SendOutcome { .. }
        )
    }

    const fn is_effect_outcome_action(&self) -> bool {
        matches!(self, Self::SpawnOutcome { .. } | Self::SendOutcome { .. })
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ExecutableTestActionPlan<'program> {
    action: ExecutableActionPlan<'program>,
    actions: CompactList<ExecutableActionPlan<'program>>,
}

#[cfg(test)]
impl<'program> ExecutableTestActionPlan<'program> {
    pub(crate) const fn action(&self) -> &ExecutableActionPlan<'program> {
        &self.action
    }

    pub(crate) fn actions(&self) -> &[ExecutableActionPlan<'program>] {
        self.actions.as_slice()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableProcessRef {
    pub(crate) id: ProcessRefId,
    pub(crate) target_process: ProcessId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableSpawnSite {
    pub(crate) id: SpawnSiteId,
    pub(crate) authority: AuthorityId,
    pub(crate) kind: LoadedSpawnKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutableSendTarget {
    ProcessRef(ExecutableProcessRef),
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

impl ExecutableSendTarget {
    fn from_loaded(process: &LoadedProcess, target: &LoadedSendTarget) -> Result<Self> {
        match target {
            LoadedSendTarget::ProcessRef(process_ref) => Ok(Self::ProcessRef(
                executable_process_ref(process, *process_ref)?,
            )),
            LoadedSendTarget::SupervisorChild {
                supervisor,
                child,
                target_process,
            } => Ok(Self::SupervisorChild {
                supervisor: *supervisor,
                child: *child,
                target_process: *target_process,
            }),
            LoadedSendTarget::ReceivedPayload { ty, target_process } => Ok(Self::ReceivedPayload {
                ty: *ty,
                target_process: *target_process,
            }),
        }
    }
}

fn executable_process_ref(
    process: &LoadedProcess,
    process_ref: ProcessRefId,
) -> Result<ExecutableProcessRef> {
    let target_process = process
        .process_refs
        .get(process_ref.index())
        .map(|process_ref| process_ref.target)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} executable action references unloaded process reference id {}",
                process.debug_name,
                process_ref.as_u32()
            ))
        })?;
    Ok(ExecutableProcessRef {
        id: process_ref,
        target_process,
    })
}

fn executable_spawn_site(
    process: &LoadedProcess,
    spawn_site: SpawnSiteId,
    target: ProcessId,
) -> Result<ExecutableSpawnSite> {
    let site = process.validate_spawn_site(spawn_site, target)?;
    let authority = site.authority.ok_or_else(|| {
        Error::new(format!(
            "process {} executable dynamic spawn site id {} has no authority id",
            process.debug_name,
            spawn_site.as_u32()
        ))
    })?;
    Ok(ExecutableSpawnSite {
        id: spawn_site,
        authority,
        kind: site.kind,
    })
}

#[cfg(test)]
mod tests;
