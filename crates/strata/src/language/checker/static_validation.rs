use std::collections::BTreeSet;

use super::super::checked::{
    CheckedAction, CheckedLoopElementId, CheckedMessageId, CheckedPayloadValue, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedTransition, CheckedTypeKind, CheckedTypeRef,
    CheckedValueTemplate,
};
use super::super::diagnostic::{Error, Result};
use mantle_artifact::{MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH, MAX_VALUE_TEMPLATE_FIELDS};

mod process_refs;
mod runtime_order;
mod templates;
mod transition_coverage;

use process_refs::{
    message_payload_type, process_by_id, process_label, process_ref_target, validate_send_target,
};
use runtime_order::validate_static_runtime_order;
use templates::{
    current_state_payload_type, validate_bool_condition_template, validate_next_state,
    validate_static_bool_condition_value, validate_value_template_binding_types,
    validate_value_template_payload_labels, validate_value_template_process_refs,
};
use transition_coverage::validate_transition_coverage;

#[derive(Clone)]
struct ActiveCheckedLoopElement {
    id: CheckedLoopElementId,
    ty: CheckedTypeRef,
}

#[derive(Clone, Copy)]
struct ActionReferenceScope<'a> {
    active_loop_elements: &'a [ActiveCheckedLoopElement],
    inside_loop: bool,
    runtime_if_depth: usize,
    loop_runtime_if_depth: usize,
}

impl<'a> ActionReferenceScope<'a> {
    const fn root() -> Self {
        Self {
            active_loop_elements: &[],
            inside_loop: false,
            runtime_if_depth: 0,
            loop_runtime_if_depth: 0,
        }
    }

    const fn loop_body(active_loop_elements: &'a [ActiveCheckedLoopElement]) -> Self {
        Self {
            active_loop_elements,
            inside_loop: true,
            runtime_if_depth: 0,
            loop_runtime_if_depth: 0,
        }
    }

    const fn if_branch(self) -> Self {
        Self {
            active_loop_elements: self.active_loop_elements,
            inside_loop: self.inside_loop,
            runtime_if_depth: if self.inside_loop {
                self.runtime_if_depth
            } else {
                self.runtime_if_depth.saturating_add(1)
            },
            loop_runtime_if_depth: if self.inside_loop {
                self.loop_runtime_if_depth.saturating_add(1)
            } else {
                self.loop_runtime_if_depth
            },
        }
    }

    fn is_inside_runtime_if_branch(self) -> bool {
        self.runtime_if_depth > 0 || self.loop_runtime_if_depth > 0
    }

    fn validate_runtime_if_allowed(self, process: &CheckedProcess) -> Result<()> {
        if self.inside_loop && self.loop_runtime_if_depth > 0 {
            return Err(Error::new(format!(
                "process {} for loop branch cannot contain nested runtime if actions in this source slice",
                process.debug_name()
            )));
        }
        if self.runtime_if_depth >= MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH {
            return Err(Error::new(format!(
                "process {} runtime if action nesting exceeds maximum depth of {MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH} in this source slice",
                process.debug_name()
            )));
        }
        Ok(())
    }
}

struct ActionValidationContext<'a> {
    processes: &'a [CheckedProcess],
    process: &'a CheckedProcess,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    transition: &'a CheckedTransition,
}

pub(super) fn validate_action_references(
    processes: &[CheckedProcess],
    entry_process: &CheckedProcessId,
    entry_message: &CheckedMessageId,
) -> Result<()> {
    for (process_index, process) in processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(process_index)?;
        validate_checked_state_table(process)?;
        for transition in process.transitions() {
            validate_transition(processes, process, process_id, *entry_process, transition)?;
        }
        validate_transition_coverage(process)?;
    }
    validate_static_runtime_order(processes, *entry_process, *entry_message)?;
    Ok(())
}

fn validate_checked_state_table(process: &CheckedProcess) -> Result<()> {
    if process.state_values().is_empty() {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process.debug_name()
        )));
    }
    if process.init_state().index() >= process.state_values().len() {
        return Err(Error::new(format!(
            "process {} init_state id {} is not a valid state value",
            process.debug_name(),
            process.init_state().as_u32()
        )));
    }

    let mut states = BTreeSet::new();
    for state in process.state_values() {
        if state.ty() != process.state_type() {
            return Err(Error::new(format!(
                "process {} state value {} has type {}, expected {}",
                process.debug_name(),
                state.label(),
                state.ty(),
                process.state_type()
            )));
        }
        state
            .value()
            .validate("state value")
            .map_err(|err| Error::new(err.to_string()))?;
        if state.value().contains_process_ref() {
            return Err(Error::new(format!(
                "process {} state value {} carries a process reference value",
                process.debug_name(),
                state.label()
            )));
        }
        if let Some(payload) = state.payload() {
            if payload.process_ref_payload().is_some() {
                return Err(Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                )));
            }
            let value = payload.value().ok_or_else(|| {
                Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                ))
            })?;
            value
                .validate("state value payload")
                .map_err(|err| Error::new(err.to_string()))?;
            if value.contains_process_ref() {
                return Err(Error::new(format!(
                    "process {} state value {} carries a process reference payload",
                    process.debug_name(),
                    state.label()
                )));
            }
        }
        if !states.insert((state.ty().id(), state.value().clone())) {
            return Err(Error::new(format!(
                "process {} declares duplicate state value {}",
                process.debug_name(),
                state.label()
            )));
        }
    }
    Ok(())
}

fn validate_transition(
    processes: &[CheckedProcess],
    process: &CheckedProcess,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    transition: &CheckedTransition,
) -> Result<()> {
    if transition.message().index() >= process.message_cases().len() {
        return Err(Error::new(format!(
            "process {} transition message id {} is not accepted",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    validate_transition_current_state(process, transition)?;
    validate_next_state(
        process,
        transition.message(),
        transition.current_state(),
        transition.next_state_ref(),
    )?;
    validate_transition_payload_guard(process, transition)?;
    validate_transition_effects(process, transition)?;
    let mut spawned_refs = BTreeSet::new();
    let context = ActionValidationContext {
        processes,
        process,
        process_id,
        entry_process,
        transition,
    };

    for action in transition.actions() {
        validate_action_reference(
            &context,
            &mut spawned_refs,
            action,
            ActionReferenceScope::root(),
        )?;
    }
    Ok(())
}

fn validate_action_reference(
    context: &ActionValidationContext<'_>,
    spawned_refs: &mut BTreeSet<CheckedProcessRefId>,
    action: &CheckedAction,
    scope: ActionReferenceScope<'_>,
) -> Result<()> {
    let process = context.process;
    let transition = context.transition;
    match action {
        CheckedAction::Emit { .. } => {}
        CheckedAction::Spawn {
            target,
            process_ref,
        } => {
            if scope.is_inside_runtime_if_branch() {
                return Err(Error::new(format!(
                    "process {} runtime if branch cannot bind process references in this source slice",
                    process.debug_name()
                )));
            }
            if scope.inside_loop {
                return Err(Error::new(format!(
                    "process {} for loop body cannot bind process references",
                    process.debug_name()
                )));
            }
            if target.index() >= context.processes.len() {
                return Err(Error::new(format!(
                    "process {} spawns undefined process id {}",
                    process.debug_name(),
                    target.as_u32()
                )));
            }
            if *target == context.entry_process {
                return Err(Error::new(format!(
                    "process {} spawns entry process {}, which is already started",
                    process.debug_name(),
                    process_label(context.processes, *target)?
                )));
            }
            if *target == context.process_id {
                return Err(Error::new(format!(
                    "process {} spawns itself, which is not supported",
                    process.debug_name()
                )));
            }
            let declared_target = process_ref_target(process, *process_ref)?;
            if declared_target != *target {
                return Err(Error::new(format!(
                    "process {} spawn process reference id {} targets process id {}, expected {}",
                    process.debug_name(),
                    process_ref.as_u32(),
                    target.as_u32(),
                    declared_target.as_u32()
                )));
            }
            if !spawned_refs.insert(*process_ref) {
                return Err(Error::new(format!(
                    "process {} duplicates process reference id {} within message transition {}",
                    process.debug_name(),
                    process_ref.as_u32(),
                    transition.message().as_u32()
                )));
            }
        }
        CheckedAction::Send {
            target,
            message,
            payload,
        } => {
            let target_process_id = validate_send_target(
                context.processes,
                process,
                transition.message(),
                target,
                spawned_refs,
            )?;
            let target_process = process_by_id(context.processes, target_process_id)?;
            if message.index() >= target_process.message_cases().len() {
                return Err(Error::new(format!(
                    "process {} sends message id {} not accepted by {}",
                    process.debug_name(),
                    message.as_u32(),
                    target_process.debug_name()
                )));
            }
            validate_send_payload_shape(
                context,
                target_process,
                *message,
                payload.as_deref(),
                spawned_refs,
                scope.active_loop_elements,
            )?;
        }
        CheckedAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            scope.validate_runtime_if_allowed(process)?;
            validate_bool_condition_template(process, condition)?;
            validate_value_template_binding_types(
                condition,
                message_payload_type(process, transition.message())?,
                current_state_payload_type(process, transition.current_state())?,
            )?;
            validate_value_template_payload_labels(condition)?;
            validate_value_template_process_refs(
                context.processes,
                process,
                condition,
                spawned_refs,
                false,
            )?;
            validate_value_template_loop_elements(condition, scope.active_loop_elements)?;
            validate_static_bool_condition_value(
                process,
                condition,
                transition_current_state_payload(process, transition)?,
            )?;
            if then_actions.is_empty() && else_actions.is_empty() {
                return Err(Error::new(format!(
                    "process {} runtime if action branches cannot both be empty",
                    process.debug_name()
                )));
            }

            let branch_scope = scope.if_branch();
            for action in then_actions {
                validate_action_reference(context, spawned_refs, action, branch_scope)?;
            }
            for action in else_actions {
                validate_action_reference(context, spawned_refs, action, branch_scope)?;
            }
        }
        CheckedAction::ForEach {
            element,
            collection,
            body,
            ..
        } => {
            if scope.inside_loop {
                return Err(Error::new(format!(
                    "process {} nested for loops are not supported in this source slice",
                    process.debug_name()
                )));
            }
            if matches!(element.ty().kind(), CheckedTypeKind::ProcessRef { .. }) {
                return Err(Error::new(format!(
                    "process {} for loop element cannot have process reference type",
                    process.debug_name()
                )));
            }
            if element.id().as_u32() >= MAX_VALUE_TEMPLATE_FIELDS as u32 {
                return Err(Error::new(format!(
                    "process {} for loop element id {} must be no greater than {}",
                    process.debug_name(),
                    element.id().as_u32(),
                    MAX_VALUE_TEMPLATE_FIELDS - 1
                )));
            }
            validate_value_template_binding_types(
                collection,
                message_payload_type(process, transition.message())?,
                current_state_payload_type(process, transition.current_state())?,
            )?;
            validate_value_template_payload_labels(collection)?;
            validate_value_template_process_refs(
                context.processes,
                process,
                collection,
                spawned_refs,
                false,
            )?;
            validate_value_template_loop_elements(collection, scope.active_loop_elements)?;
            let active = [ActiveCheckedLoopElement {
                id: element.id(),
                ty: element.ty().clone(),
            }];
            for action in body {
                validate_action_reference(
                    context,
                    spawned_refs,
                    action,
                    ActionReferenceScope::loop_body(&active),
                )?;
            }
        }
    }
    Ok(())
}

fn validate_transition_effects(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    let mut declared_effects = BTreeSet::new();
    for &effect in transition.effects() {
        if !declared_effects.insert(effect) {
            return Err(Error::new(format!(
                "process {} transition {} declares duplicate effect {effect}",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
    }

    let mut used_effects = BTreeSet::new();
    for action in transition.actions() {
        action.collect_effects(&mut used_effects);
    }
    for effect in &used_effects {
        if !declared_effects.contains(effect) {
            return Err(Error::new(format!(
                "process {} transition {} uses effect {effect} but does not declare it",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
    }

    for effect in &declared_effects {
        if !used_effects.contains(effect) {
            return Err(Error::new(format!(
                "process {} transition {} declares effect {effect} but no action uses it",
                process.debug_name(),
                transition.message().as_u32()
            )));
        }
    }
    Ok(())
}

fn validate_transition_payload_guard(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    let Some(payload_guard) = transition.payload_guard() else {
        return Ok(());
    };
    if payload_guard.process_ref_payload().is_some() || payload_guard.value().is_none() {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard cannot be a process reference payload",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    let Some(expected_type) = message_payload_type(process, transition.message())? else {
        return Err(Error::new(format!(
            "process {} transition message id {} has a payload guard, but the message accepts no payload",
            process.debug_name(),
            transition.message().as_u32()
        )));
    };
    if payload_guard.ty() != expected_type {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard has type {}, expected {}",
            process.debug_name(),
            transition.message().as_u32(),
            payload_guard.ty(),
            expected_type
        )));
    }
    let value = payload_guard.value().ok_or_else(|| {
        Error::new(format!(
            "process {} transition message id {} payload guard cannot be a process reference payload",
            process.debug_name(),
            transition.message().as_u32()
        ))
    })?;
    value
        .validate("transition payload guard")
        .map_err(|err| Error::new(err.to_string()))?;
    if value.contains_process_ref() {
        return Err(Error::new(format!(
            "process {} transition message id {} payload guard contains a process reference value",
            process.debug_name(),
            transition.message().as_u32()
        )));
    }
    Ok(())
}

fn validate_send_payload_shape(
    context: &ActionValidationContext<'_>,
    target_process: &CheckedProcess,
    target_message: CheckedMessageId,
    payload: Option<&CheckedValueTemplate>,
    spawned_refs: &BTreeSet<CheckedProcessRefId>,
    active_loop_elements: &[ActiveCheckedLoopElement],
) -> Result<()> {
    let process = context.process;
    let transition = context.transition;
    let current_payload_type = message_payload_type(process, transition.message())?;
    let current_state_payload_type =
        current_state_payload_type(process, transition.current_state())?;
    let target_payload_type = message_payload_type(target_process, target_message)?;
    match (target_payload_type, payload) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(Error::new(format!(
            "process {} sends payload to message id {}, which does not accept one",
            process.debug_name(),
            target_message.as_u32()
        ))),
        (Some(_), None) => Err(Error::new(format!(
            "process {} sends message id {} without required payload",
            process.debug_name(),
            target_message.as_u32()
        ))),
        (Some(expected_type), Some(payload)) => {
            let allow_direct_process_ref = active_loop_elements.is_empty();
            validate_value_template_binding_types(
                payload,
                current_payload_type,
                current_state_payload_type,
            )?;
            validate_value_template_payload_labels(payload)?;
            validate_value_template_process_refs(
                context.processes,
                process,
                payload,
                spawned_refs,
                allow_direct_process_ref,
            )?;
            validate_value_template_loop_elements(payload, active_loop_elements)?;
            if payload.result_type() != expected_type {
                return Err(Error::new(format!(
                    "process {} sends payload of type {}, expected {}",
                    process.debug_name(),
                    payload.result_type(),
                    expected_type
                )));
            }
            Ok(())
        }
    }
}

fn validate_value_template_loop_elements(
    template: &CheckedValueTemplate,
    active_loop_elements: &[ActiveCheckedLoopElement],
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. } => Ok(()),
        CheckedValueTemplate::LoopElement { ty, element } => {
            let Some(active) = active_loop_elements
                .iter()
                .find(|active| active.id == *element)
            else {
                return Err(Error::new(format!(
                    "references inactive loop element id {}",
                    element.as_u32()
                )));
            };
            if active.ty != *ty {
                return Err(Error::new(format!(
                    "loop element id {} has type {}, expected {}",
                    element.as_u32(),
                    active.ty,
                    ty
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::EnumPayload { value, .. } => {
            validate_value_template_loop_elements(value, active_loop_elements)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            validate_value_template_loop_elements(record, active_loop_elements)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            validate_value_template_loop_elements(list, active_loop_elements)
        }
        CheckedValueTemplate::MapValue { map, .. } => {
            validate_value_template_loop_elements(map, active_loop_elements)
        }
        CheckedValueTemplate::MapRest { map, .. } => {
            validate_value_template_loop_elements(map, active_loop_elements)
        }
        CheckedValueTemplate::Equality { left, right, .. } => {
            validate_value_template_loop_elements(left, active_loop_elements)?;
            validate_value_template_loop_elements(right, active_loop_elements)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            validate_value_template_loop_elements(operand, active_loop_elements)
        }
        CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            validate_value_template_loop_elements(left, active_loop_elements)?;
            validate_value_template_loop_elements(right, active_loop_elements)
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            validate_value_template_loop_elements(payload, active_loop_elements)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_loop_elements(field.value(), active_loop_elements)?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_loop_elements(item, active_loop_elements)?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_loop_elements(entry.key(), active_loop_elements)?;
                validate_value_template_loop_elements(entry.value(), active_loop_elements)?;
            }
            Ok(())
        }
    }
}

fn validate_transition_current_state(
    process: &CheckedProcess,
    transition: &CheckedTransition,
) -> Result<()> {
    if let Some(current_state) = transition.current_state() {
        if current_state.index() >= process.state_values().len() {
            return Err(Error::new(format!(
                "process {} message id {} current_state id {} is not a valid state value",
                process.debug_name(),
                transition.message().as_u32(),
                current_state.as_u32()
            )));
        }
    }
    Ok(())
}

fn transition_current_state_payload<'a>(
    process: &'a CheckedProcess,
    transition: &CheckedTransition,
) -> Result<Option<&'a CheckedPayloadValue>> {
    let Some(current_state) = transition.current_state() else {
        return Ok(None);
    };
    let state = process
        .state_values()
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} current_state id {} is not a valid state value",
                process.debug_name(),
                transition.message().as_u32(),
                current_state.as_u32()
            ))
        })?;
    Ok(state.payload())
}

#[cfg(test)]
mod tests;
