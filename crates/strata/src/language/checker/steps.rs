use super::message_cases::{DiscoveredMessageCase, MessageCaseTable};
use super::*;

mod clauses;
mod discovery;
mod process_refs;
mod transition;

pub(in crate::language::checker) use clauses::collect_concrete_state_payload_domains;
pub(in crate::language::checker) use discovery::{
    check_step_shape, matching_message_cases, pattern_binding_subject, payload_value_bindings,
    resolve_send_target_process_for_discovery, step_discovery_clauses,
    validate_pattern_binding_name,
};
pub(in crate::language::checker) use process_refs::collect_message_case_process_refs;

use clauses::check_step_clauses;
use process_refs::collect_process_refs;
use transition::check_step_transition;

pub(in crate::language::checker) fn check_step(
    context: &ProcessCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<(Vec<CheckedProcessRef>, Vec<CheckedTransition>)> {
    let step_clauses = check_step_clauses(
        context.module,
        context.process,
        context.process_id,
        context.semantic_index,
        context.message_cases,
        state_space,
        types,
    )?;
    let (process_refs, process_ref_index) = collect_process_refs(
        context.process,
        context.process_id,
        context.entry_process,
        context.semantic_index,
        &step_clauses,
    )?;
    let mut step_context = StepCheckContext {
        module: context.module,
        process: context.process,
        process_id: context.process_id,
        semantic_index: context.semantic_index,
        process_ref_index: &process_ref_index,
        message_cases: context.message_cases,
    };

    let mut transitions = Vec::with_capacity(step_clauses.len());
    for clause in step_clauses {
        let transition = check_step_transition(
            &mut step_context,
            state_space,
            outputs,
            types,
            StepTransitionInput {
                current_state: clause.current_state,
                variant: clause.variant,
                message: clause.message,
                payload_guard: clause.payload_guard.as_ref(),
                payload_bindings: &clause.payload_bindings,
                state_payload_bindings: &clause.state_payload_bindings,
                body: clause.body,
                declared_effects: &clause.step.effects,
            },
        )?;
        let used_effects =
            transition
                .actions()
                .iter()
                .fold(BTreeSet::new(), |mut effects, action| {
                    effects.insert(action.effect());
                    effects
                });
        validate_effects("step", &clause.step.effects, used_effects)?;
        transitions.push(transition);
    }

    let action_count = total_action_count(&transitions)?;
    validate_count(
        &format!("process {} action_count", context.process.name),
        action_count,
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;

    Ok((process_refs, transitions))
}
