use std::collections::{BTreeMap, VecDeque};

use mantle_artifact::ArtifactValue;

use super::super::super::checked::{
    CheckedAction, CheckedLoopElementId, CheckedMessageId, CheckedNextState, CheckedPayloadValue,
    CheckedProcess, CheckedProcessId, CheckedProcessRefId, CheckedSendTarget, CheckedStateId,
    CheckedStepResult, CheckedValueTemplate,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::super::{STATIC_RUNTIME_DISPATCH_LIMIT, STATIC_RUNTIME_PROCESS_LIMIT};
use super::process_refs::{process_by_id, process_label, process_ref_target};

mod conditions;
mod effect_outcomes;
mod templates;
mod transitions;

use conditions::evaluate_checked_bool_condition;
use effect_outcomes::{
    StaticEffectOutcomeBinding, bind_static_effect_outcome, ok_process_ref_outcome,
    ok_unit_outcome, send_error_outcome, spawn_error_outcome, static_original_message,
};
use templates::{checked_payload_value, evaluate_checked_runtime_template};
use transitions::transition_for_message;

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticLoopElementBinding {
    id: CheckedLoopElementId,
    value: CheckedPayloadValue,
}

#[derive(Clone, Copy)]
struct StaticActionContext<'a> {
    processes: &'a [CheckedProcess],
    process: &'a CheckedProcess,
    envelope: &'a StaticMessageEnvelope,
    current_state_payload: Option<&'a CheckedPayloadValue>,
    loop_elements: &'a [StaticLoopElementBinding],
}

impl<'a> StaticActionContext<'a> {
    fn with_loop_elements<'b>(
        self,
        loop_elements: &'b [StaticLoopElementBinding],
    ) -> StaticActionContext<'b>
    where
        'a: 'b,
    {
        StaticActionContext {
            loop_elements,
            ..self
        }
    }
}

struct StaticActionState<'a> {
    instances: &'a mut Vec<StaticProcessInstance>,
    next_pid: &'a mut StaticProcessId,
    local_process_refs: &'a mut BTreeMap<CheckedProcessRefId, StaticProcessId>,
    effect_outcomes: &'a mut Vec<StaticEffectOutcomeBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StaticProcessStatus {
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct StaticProcessId(u32);

impl StaticProcessId {
    pub(super) const FIRST: Self = Self(1);

    pub(super) fn checked_next(self) -> Result<Self> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| Error::new("static runtime process id overflowed"))
    }

    pub(super) fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticProcessInstance {
    pub(super) pid: StaticProcessId,
    pub(super) process_id: CheckedProcessId,
    pub(super) state: CheckedStateId,
    pub(super) status: StaticProcessStatus,
    pub(super) mailbox: VecDeque<StaticMessageEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticMessageEnvelope {
    message: CheckedMessageId,
    payload: Option<CheckedPayloadValue>,
}

impl StaticMessageEnvelope {
    fn new(message: CheckedMessageId, payload: Option<CheckedPayloadValue>) -> Self {
        Self { message, payload }
    }
}

pub(super) fn bind_static_process_ref(
    process: &CheckedProcess,
    process_refs: &mut BTreeMap<CheckedProcessRefId, StaticProcessId>,
    process_ref: CheckedProcessRefId,
    pid: StaticProcessId,
) -> Result<()> {
    process_ref_target(process, process_ref)?;
    if process_refs.insert(process_ref, pid).is_some() {
        return Err(Error::new(format!(
            "rebinds process reference id {}",
            process_ref.as_u32()
        )));
    }
    Ok(())
}

pub(super) fn resolve_static_process_ref(
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    process_ref: CheckedProcessRefId,
) -> Result<StaticProcessId> {
    process_ref_target(process, process_ref)?;
    process_refs.get(&process_ref).copied().ok_or_else(|| {
        Error::new(format!(
            "sends to unbound process reference id {}",
            process_ref.as_u32()
        ))
    })
}

fn resolve_static_send_target(
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    target: &CheckedSendTarget,
    received_payload: Option<&CheckedPayloadValue>,
) -> Result<StaticProcessId> {
    match target {
        CheckedSendTarget::ProcessRef(process_ref) => {
            resolve_static_process_ref(process, process_refs, *process_ref)
        }
        CheckedSendTarget::ReceivedPayload { ty, target } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received process reference send target requires a payload")
            })?;
            if payload.ty() != ty {
                return Err(Error::new(format!(
                    "received process reference send target has type {}, expected {}",
                    payload.ty(),
                    ty
                )));
            }
            let process_ref = payload
                .process_ref_payload()
                .ok_or_else(|| Error::new("received payload is not a process reference value"))?;
            if process_ref.target() != *target {
                return Err(Error::new(format!(
                    "received process reference targets process id {}, expected {}",
                    process_ref.target().as_u32(),
                    target.as_u32()
                )));
            }
            let pid = u32::try_from(process_ref.pid()).map_err(|_| {
                Error::new(format!(
                    "received process reference pid {} cannot be represented by static validation",
                    process_ref.pid()
                ))
            })?;
            Ok(StaticProcessId(pid))
        }
    }
}

pub(super) fn static_process_index_for_pid(
    instances: &[StaticProcessInstance],
    pid: StaticProcessId,
) -> Result<usize> {
    let raw_index = pid
        .as_u32()
        .checked_sub(1)
        .ok_or_else(|| Error::new("static runtime process id index underflowed"))?;
    let process_index = usize::try_from(raw_index).map_err(|_| {
        Error::new(format!(
            "static runtime process id {} cannot be indexed on this platform",
            pid.as_u32()
        ))
    })?;
    let instance = instances.get(process_index).ok_or_else(|| {
        Error::new(format!(
            "static runtime process id {} is not spawned",
            pid.as_u32()
        ))
    })?;
    if instance.pid != pid {
        return Err(Error::new(format!(
            "static runtime process index for pid {} is inconsistent",
            pid.as_u32()
        )));
    }
    Ok(process_index)
}

pub(super) fn ensure_static_process_capacity(instance_count: usize) -> Result<()> {
    if instance_count >= STATIC_RUNTIME_PROCESS_LIMIT {
        return Err(Error::new(format!(
            "static runtime process instance limit exceeded at {STATIC_RUNTIME_PROCESS_LIMIT} process instance(s)"
        )));
    }
    Ok(())
}

pub(super) fn validate_static_runtime_order(
    processes: &[CheckedProcess],
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
) -> Result<()> {
    let entry_definition = process_by_id(processes, entry_process)?;
    let mut instances = vec![StaticProcessInstance {
        pid: StaticProcessId::FIRST,
        process_id: entry_process,
        state: entry_definition.init_state(),
        status: StaticProcessStatus::Running,
        mailbox: VecDeque::from([StaticMessageEnvelope::new(entry_message, None)]),
    }];
    let mut next_pid = StaticProcessId::FIRST.checked_next()?;
    let mut dispatches = 0usize;

    while let Some(process_index) = next_static_runnable(&instances) {
        if dispatches >= STATIC_RUNTIME_DISPATCH_LIMIT {
            return Err(Error::new(format!(
                "static runtime validation exceeded {STATIC_RUNTIME_DISPATCH_LIMIT} process step(s)"
            )));
        }

        let process_id = instances[process_index].process_id;
        let process = process_by_id(processes, process_id)?;
        let envelope = instances[process_index]
            .mailbox
            .pop_front()
            .ok_or_else(|| Error::new("static runtime mailbox changed during dequeue"))?;
        let current_state = instances[process_index].state;
        let current_state_payload = process
            .state_values()
            .get(current_state.index())
            .and_then(|state| state.payload());
        let transition = transition_for_message(
            process,
            envelope.message,
            current_state,
            envelope.payload.as_ref(),
        )?;
        let mut local_process_refs = BTreeMap::new();
        let mut effect_outcomes = Vec::new();
        let next_state_depends_on_outcome =
            checked_next_state_depends_on_effect_outcome(transition.next_state_ref());
        let pre_action_state = if next_state_depends_on_outcome {
            None
        } else {
            Some(resolve_checked_runtime_next_state(
                process,
                current_state,
                transition.next_state_ref(),
                envelope.payload.as_ref(),
                &local_process_refs,
                &effect_outcomes,
            )?)
        };

        {
            let action_context = StaticActionContext {
                processes,
                process,
                envelope: &envelope,
                current_state_payload,
                loop_elements: &[],
            };
            let mut action_state = StaticActionState {
                instances: &mut instances,
                next_pid: &mut next_pid,
                local_process_refs: &mut local_process_refs,
                effect_outcomes: &mut effect_outcomes,
            };
            for action in transition.actions() {
                execute_static_action(action_context, &mut action_state, action)?;
            }
        }

        let final_state = match pre_action_state {
            Some(state) => state,
            None => resolve_checked_runtime_next_state(
                process,
                current_state,
                transition.next_state_ref(),
                envelope.payload.as_ref(),
                &local_process_refs,
                &effect_outcomes,
            )?,
        };
        instances[process_index].state = final_state;
        match transition.step_result() {
            CheckedStepResult::Continue => {}
            CheckedStepResult::Stop => {
                instances[process_index].status = StaticProcessStatus::Stopped;
            }
            CheckedStepResult::Panic => {
                instances[process_index].status = StaticProcessStatus::Failed;
                return Ok(());
            }
        }
        dispatches += 1;
    }

    for instance in &instances {
        if !instance.mailbox.is_empty() {
            return Err(Error::new(format!(
                "process {} would retain {} unhandled message(s)",
                process_label(processes, instance.process_id)?,
                instance.mailbox.len()
            )));
        }
    }

    Ok(())
}

fn checked_next_state_depends_on_effect_outcome(next_state: &CheckedNextState) -> bool {
    match next_state {
        CheckedNextState::Current | CheckedNextState::Value(_) => false,
        CheckedNextState::Template(template) => {
            super::templates::checked_template_depends_on_effect_outcome(template)
        }
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            super::templates::checked_template_depends_on_effect_outcome(condition)
                || checked_next_state_depends_on_effect_outcome(then_state)
                || checked_next_state_depends_on_effect_outcome(else_state)
        }
    }
}

fn execute_static_action(
    context: StaticActionContext<'_>,
    state: &mut StaticActionState<'_>,
    action: &CheckedAction,
) -> Result<()> {
    match action {
        CheckedAction::Emit { .. } => Ok(()),
        CheckedAction::Spawn {
            target,
            process_ref,
            ..
        } => {
            let target_process = process_by_id(context.processes, *target)?;
            ensure_static_process_capacity(state.instances.len())?;
            let spawned_pid = *state.next_pid;
            *state.next_pid = state.next_pid.checked_next()?;
            bind_static_process_ref(
                context.process,
                state.local_process_refs,
                *process_ref,
                spawned_pid,
            )
            .map_err(|err| Error::new(format!("process {} {err}", context.process.debug_name())))?;
            state.instances.push(StaticProcessInstance {
                pid: spawned_pid,
                process_id: *target,
                state: target_process.init_state(),
                status: StaticProcessStatus::Running,
                mailbox: VecDeque::new(),
            });
            Ok(())
        }
        CheckedAction::SpawnOutcome {
            outcome,
            outcome_ty,
            target,
            ..
        } => {
            let target_process = process_by_id(context.processes, *target)?;
            if state.instances.len() >= STATIC_RUNTIME_PROCESS_LIMIT {
                return bind_static_effect_outcome(
                    context.process,
                    state,
                    *outcome,
                    outcome_ty,
                    spawn_error_outcome(outcome_ty, "Exhausted")?,
                );
            }
            let spawned_pid = *state.next_pid;
            *state.next_pid = state.next_pid.checked_next()?;
            state.instances.push(StaticProcessInstance {
                pid: spawned_pid,
                process_id: *target,
                state: target_process.init_state(),
                status: StaticProcessStatus::Running,
                mailbox: VecDeque::new(),
            });
            bind_static_effect_outcome(
                context.process,
                state,
                *outcome,
                outcome_ty,
                ok_process_ref_outcome(outcome_ty, u64::from(spawned_pid.as_u32()))?,
            )
        }
        CheckedAction::Send {
            target,
            message,
            payload,
        } => execute_static_send(
            context,
            state,
            target,
            *message,
            payload.as_ref().map(Box::as_ref),
        ),
        CheckedAction::SendOutcome {
            outcome,
            outcome_ty,
            target,
            message,
            payload,
        } => {
            let target_pid = resolve_static_send_target(
                context.process,
                state.local_process_refs,
                target,
                context.envelope.payload.as_ref(),
            )
            .map_err(|err| Error::new(format!("process {} {err}", context.process.debug_name())))?;
            let target_index =
                static_process_index_for_pid(state.instances, target_pid).map_err(|err| {
                    Error::new(format!(
                        "process {} sends through process reference to {err}",
                        context.process.debug_name()
                    ))
                })?;
            let target_process =
                process_by_id(context.processes, state.instances[target_index].process_id)?;
            if message.index() >= target_process.message_cases().len() {
                return Err(Error::new(format!(
                    "process {} sends message id {} not accepted by {}",
                    context.process.debug_name(),
                    message.as_u32(),
                    target_process.debug_name()
                )));
            }

            let prepared_payload = match payload {
                Some(payload) => Some(evaluate_checked_runtime_template(
                    payload,
                    context.envelope.payload.as_ref(),
                    context.current_state_payload,
                    context.process,
                    state.local_process_refs,
                    context.loop_elements,
                    state.effect_outcomes,
                )?),
                None => None,
            };
            let failure_variant = match state.instances[target_index].status {
                StaticProcessStatus::Running => None,
                StaticProcessStatus::Stopped => Some("Stopped"),
                StaticProcessStatus::Failed => Some("Crashed"),
            }
            .or_else(|| {
                (state.instances[target_index].mailbox.len() >= target_process.mailbox_bound())
                    .then_some("Full")
            });
            if let Some(error_variant) = failure_variant {
                let original_message =
                    static_original_message(target_process, *message, prepared_payload.as_ref())?;
                return bind_static_effect_outcome(
                    context.process,
                    state,
                    *outcome,
                    outcome_ty,
                    send_error_outcome(outcome_ty, error_variant, original_message)?,
                );
            }
            state.instances[target_index]
                .mailbox
                .push_back(StaticMessageEnvelope::new(*message, prepared_payload));
            bind_static_effect_outcome(
                context.process,
                state,
                *outcome,
                outcome_ty,
                ok_unit_outcome(outcome_ty)?,
            )
        }
        CheckedAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            let selected_actions = if evaluate_checked_bool_condition(
                condition,
                context.envelope.payload.as_ref(),
                context.current_state_payload,
                context.process,
                state.local_process_refs,
                context.loop_elements,
                state.effect_outcomes,
            )? {
                then_actions
            } else {
                else_actions
            };
            for action in selected_actions {
                execute_static_action(context, state, action)?;
            }
            Ok(())
        }
        CheckedAction::ForEach {
            element,
            collection,
            max_items,
            body,
        } => {
            let collection = evaluate_checked_runtime_template(
                collection,
                context.envelope.payload.as_ref(),
                context.current_state_payload,
                context.process,
                state.local_process_refs,
                context.loop_elements,
                state.effect_outcomes,
            )?;
            let collection_label = collection.label();
            let collection_value = checked_payload_value(&collection)?;
            let ArtifactValue::List(items) = collection_value else {
                return Err(Error::new(format!(
                    "process {} for loop collection produced non-list value {}",
                    context.process.debug_name(),
                    collection_label
                )));
            };
            if items.len() > *max_items {
                return Err(Error::new(format!(
                    "process {} for loop collection has {} item(s), max_items is {}",
                    context.process.debug_name(),
                    items.len(),
                    max_items
                )));
            }
            for item in items {
                let item_payload = CheckedPayloadValue::new(element.ty().clone(), item);
                let binding = StaticLoopElementBinding {
                    id: element.id(),
                    value: item_payload,
                };
                let loop_elements = [binding];
                execute_static_action_list(
                    context.with_loop_elements(&loop_elements),
                    state,
                    body,
                )?;
            }
            Ok(())
        }
    }
}

fn execute_static_send(
    context: StaticActionContext<'_>,
    state: &mut StaticActionState<'_>,
    target: &CheckedSendTarget,
    message: CheckedMessageId,
    payload: Option<&CheckedValueTemplate>,
) -> Result<()> {
    let target_pid = resolve_static_send_target(
        context.process,
        state.local_process_refs,
        target,
        context.envelope.payload.as_ref(),
    )
    .map_err(|err| Error::new(format!("process {} {err}", context.process.debug_name())))?;
    let target_index =
        static_process_index_for_pid(state.instances, target_pid).map_err(|err| {
            Error::new(format!(
                "process {} sends through process reference to {err}",
                context.process.debug_name()
            ))
        })?;
    let target_process =
        process_by_id(context.processes, state.instances[target_index].process_id)?;
    if message.index() >= target_process.message_cases().len() {
        return Err(Error::new(format!(
            "process {} sends message id {} not accepted by {}",
            context.process.debug_name(),
            message.as_u32(),
            target_process.debug_name()
        )));
    }

    if state.instances[target_index].status != StaticProcessStatus::Running {
        return Err(Error::new(format!(
            "process {} sends to {}, which is not running",
            context.process.debug_name(),
            target_process.debug_name()
        )));
    }
    if state.instances[target_index].mailbox.len() >= target_process.mailbox_bound() {
        return Err(Error::new(format!(
            "process {} sends to {}, but its mailbox would exceed bound {}",
            context.process.debug_name(),
            target_process.debug_name(),
            target_process.mailbox_bound()
        )));
    }
    let payload = match payload {
        Some(payload) => Some(evaluate_checked_runtime_template(
            payload,
            context.envelope.payload.as_ref(),
            context.current_state_payload,
            context.process,
            state.local_process_refs,
            context.loop_elements,
            state.effect_outcomes,
        )?),
        None => None,
    };
    state.instances[target_index]
        .mailbox
        .push_back(StaticMessageEnvelope::new(message, payload));
    Ok(())
}

fn execute_static_action_list(
    context: StaticActionContext<'_>,
    state: &mut StaticActionState<'_>,
    actions: &[CheckedAction],
) -> Result<()> {
    for action in actions {
        execute_static_action(context, state, action)?;
    }
    Ok(())
}

fn resolve_checked_runtime_next_state(
    process: &CheckedProcess,
    current_state: CheckedStateId,
    next_state: &CheckedNextState,
    received_payload: Option<&CheckedPayloadValue>,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    effect_outcomes: &[StaticEffectOutcomeBinding],
) -> Result<CheckedStateId> {
    let current_state_payload = process
        .state_values()
        .get(current_state.index())
        .and_then(|state| state.payload());
    match next_state {
        CheckedNextState::Current => Ok(current_state),
        CheckedNextState::Value(state) => Ok(*state),
        CheckedNextState::Template(template) => resolve_checked_runtime_template_state(
            process,
            template,
            received_payload,
            current_state_payload,
            process_refs,
            effect_outcomes,
        ),
        CheckedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            let selected_state = match evaluate_checked_bool_condition(
                condition,
                received_payload,
                current_state_payload,
                process,
                process_refs,
                &[],
                effect_outcomes,
            )? {
                true => then_state,
                false => else_state,
            };
            resolve_checked_runtime_next_state(
                process,
                current_state,
                selected_state,
                received_payload,
                process_refs,
                effect_outcomes,
            )
        }
    }
}

fn resolve_checked_runtime_template_state(
    process: &CheckedProcess,
    template: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    effect_outcomes: &[StaticEffectOutcomeBinding],
) -> Result<CheckedStateId> {
    let value = evaluate_checked_runtime_template(
        template,
        received_payload,
        current_state_payload,
        process,
        process_refs,
        &[],
        effect_outcomes,
    )?;
    let state_index = process
        .state_values()
        .iter()
        .position(|state| state.has_same_identity_as_payload(&value))
        .ok_or_else(|| {
            Error::new(format!(
                "process {} next_state template produced value {} not admitted by state table",
                process.debug_name(),
                value.label()
            ))
        })?;
    CheckedStateId::from_index(state_index)
}

fn next_static_runnable(instances: &[StaticProcessInstance]) -> Option<usize> {
    instances.iter().position(|instance| {
        instance.status == StaticProcessStatus::Running && !instance.mailbox.is_empty()
    })
}
