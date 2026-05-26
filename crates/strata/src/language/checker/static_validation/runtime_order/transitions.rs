use crate::language::checked::{
    CheckedMessageId, CheckedPayloadValue, CheckedProcess, CheckedStateId, CheckedTransition,
};
use crate::language::diagnostic::{Error, Result};

pub(super) fn transition_for_message<'a>(
    process: &'a CheckedProcess,
    message: CheckedMessageId,
    current_state: CheckedStateId,
    payload: Option<&CheckedPayloadValue>,
) -> Result<&'a CheckedTransition> {
    let message_is_state_specific = process
        .transitions()
        .iter()
        .any(|transition| transition.message() == message && transition.current_state().is_some());
    let expected_state = message_is_state_specific.then_some(current_state);
    let base_has_payload_guard = process.transitions().iter().any(|transition| {
        transition.message() == message
            && transition.current_state() == expected_state
            && transition.payload_guard().is_some()
    });

    if base_has_payload_guard {
        let payload = payload.ok_or_else(|| {
            Error::new(format!(
                "process {} has payload-specific transition(s) for message id {}, but the queued message has no payload",
                process.debug_name(),
                message.as_u32()
            ))
        })?;
        return process
            .transitions()
            .iter()
            .find(|transition| {
                transition.message() == message
                    && transition.current_state() == expected_state
                    && transition
                        .payload_guard()
                        .is_some_and(|guard| payload_guard_matches(guard, payload))
            })
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} has no transition for message id {} current_state id {} payload {}",
                    process.debug_name(),
                    message.as_u32(),
                    current_state.as_u32(),
                    payload.label()
                ))
            });
    }

    process
        .transitions()
        .iter()
        .find(|transition| {
            transition.message() == message
                && transition.current_state() == expected_state
                && transition.payload_guard().is_none()
        })
        .ok_or_else(|| {
            Error::new(format!(
                "process {} has no transition for message id {} current_state id {}",
                process.debug_name(),
                message.as_u32(),
                current_state.as_u32()
            ))
        })
}

fn payload_guard_matches(guard: &CheckedPayloadValue, payload: &CheckedPayloadValue) -> bool {
    guard.ty() == payload.ty()
        && guard
            .value()
            .zip(payload.value())
            .is_some_and(|(guard_value, payload_value)| guard_value == payload_value)
}
