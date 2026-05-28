use super::effect_outcomes::{
    bind_static_effect_outcome, ok_unit_outcome, send_error_outcome, static_original_message,
};
use super::send_targets::{StaticSendOutcomeTarget, resolve_static_send_outcome_target};
use super::templates::evaluate_checked_runtime_template;
use super::{
    StaticActionContext, StaticActionState, StaticMessageEnvelope, StaticProcessStatus,
    static_process_index_for_pid,
};
use crate::language::checked::{
    CheckedEffectOutcomeId, CheckedMessageId, CheckedSendTarget, CheckedTypeRef,
    CheckedValueTemplate,
};
use crate::language::checker::static_validation::process_refs::process_by_id;
use crate::language::diagnostic::{Error, Result};

pub(super) fn execute_static_send_outcome(
    context: StaticActionContext<'_>,
    state: &mut StaticActionState<'_>,
    outcome: CheckedEffectOutcomeId,
    outcome_ty: &CheckedTypeRef,
    target: &CheckedSendTarget,
    message: CheckedMessageId,
    payload: Option<&CheckedValueTemplate>,
) -> Result<()> {
    let target_resolution = resolve_static_send_outcome_target(
        context.process,
        context.current_pid,
        state.local_process_refs,
        state.supervisor_children,
        state.instances,
        target,
        context.envelope.payload.as_ref(),
    )
    .map_err(|err| Error::new(format!("process {} {err}", context.process.debug_name())))?;
    let target_index = match target_resolution {
        StaticSendOutcomeTarget::Active(target_pid) => Some(
            static_process_index_for_pid(state.instances, target_pid).map_err(|err| {
                Error::new(format!(
                    "process {} sends through process reference to {err}",
                    context.process.debug_name()
                ))
            })?,
        ),
        StaticSendOutcomeTarget::InactiveSupervisorChild { .. } => None,
    };
    let target_process_id = match (target_resolution, target_index) {
        (_, Some(index)) => state.instances[index].process_id,
        (StaticSendOutcomeTarget::InactiveSupervisorChild { target_process, .. }, None) => {
            target_process
        }
        (StaticSendOutcomeTarget::Active(_), None) => {
            return Err(Error::new(format!(
                "process {} active send target did not resolve to a static process",
                context.process.debug_name()
            )));
        }
    };
    let target_process = process_by_id(context.processes, target_process_id)?;
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
    let failure_variant = match target_index {
        Some(index) => match state.instances[index].status {
            StaticProcessStatus::Running => None,
            StaticProcessStatus::Stopped => Some("Stopped"),
            StaticProcessStatus::Failed => Some("Crashed"),
        }
        .or_else(|| {
            (state.instances[index].mailbox.len() >= target_process.mailbox_bound())
                .then_some("Full")
        }),
        None => match target_resolution {
            StaticSendOutcomeTarget::InactiveSupervisorChild { status, .. } => {
                Some(static_status_send_error_variant(status)?)
            }
            StaticSendOutcomeTarget::Active(_) => {
                return Err(Error::new(format!(
                    "process {} active send target lost its static process",
                    context.process.debug_name()
                )));
            }
        },
    };
    if let Some(error_variant) = failure_variant {
        let original_message =
            static_original_message(target_process, message, prepared_payload.as_ref())?;
        return bind_static_effect_outcome(
            context.process,
            state,
            outcome,
            outcome_ty,
            send_error_outcome(outcome_ty, error_variant, original_message)?,
        );
    }
    let target_index = target_index.ok_or_else(|| {
        Error::new(format!(
            "process {} inactive send target cannot accept message id {}",
            context.process.debug_name(),
            message.as_u32()
        ))
    })?;
    state.instances[target_index]
        .mailbox
        .push_back(StaticMessageEnvelope::new(message, prepared_payload));
    bind_static_effect_outcome(
        context.process,
        state,
        outcome,
        outcome_ty,
        ok_unit_outcome(outcome_ty)?,
    )
}

fn static_status_send_error_variant(status: StaticProcessStatus) -> Result<&'static str> {
    match status {
        StaticProcessStatus::Stopped => Ok("Stopped"),
        StaticProcessStatus::Failed => Ok("Crashed"),
        StaticProcessStatus::Running => Err(Error::new(
            "running static send target cannot be represented as a send error",
        )),
    }
}
