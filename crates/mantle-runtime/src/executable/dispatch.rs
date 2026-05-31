use mantle_artifact::{MessageId, StateId};

use super::compact::{CompactList, CompactListBuilder};
use super::{ExecutableTransition, ExecutableTransitionId};
use crate::program::RuntimePayload;

#[derive(Debug, Clone)]
pub(super) struct ExecutableDispatchTable {
    unguarded_by_key: CompactList<ExecutableDispatchEntry>,
    guarded_ids: CompactList<ExecutableTransitionId>,
    state_specific_messages: CompactList<u32>,
    payload_specific_bases: CompactList<ExecutableTransitionKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExecutableDispatchEntry {
    key: ExecutableTransitionKey,
    transition: ExecutableTransitionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ExecutableTransitionKey {
    message: u32,
    current_state: Option<u32>,
}

impl ExecutableDispatchTable {
    pub(super) fn from_transitions(transitions: &[ExecutableTransition<'_>]) -> Self {
        let counts = ExecutableDispatchCounts::from_transitions(transitions);
        let mut unguarded_by_key = CompactListBuilder::with_expected_len(counts.unguarded_by_key);
        let mut guarded_ids = CompactListBuilder::with_expected_len(counts.guarded_ids);
        let mut state_specific_messages =
            CompactListBuilder::with_expected_len(counts.state_specific_messages);
        let mut payload_specific_bases =
            CompactListBuilder::with_expected_len(counts.payload_specific_bases);

        for transition in transitions {
            let message = transition.message.as_u32();
            let current_state = transition.current_state.map(StateId::as_u32);
            let key = ExecutableTransitionKey {
                message,
                current_state,
            };
            if current_state.is_some() {
                state_specific_messages.push(message);
            }
            if transition.payload_guard.is_some() {
                payload_specific_bases.push(key);
                guarded_ids.push(transition.id);
            } else {
                unguarded_by_key.push(ExecutableDispatchEntry {
                    key,
                    transition: transition.id,
                });
            }
        }
        let mut unguarded_by_key = unguarded_by_key.finish();
        unguarded_by_key
            .as_mut_slice()
            .sort_unstable_by_key(|entry| entry.key);
        let mut state_specific_messages = state_specific_messages.finish();
        state_specific_messages.as_mut_slice().sort_unstable();
        let mut payload_specific_bases = payload_specific_bases.finish();
        payload_specific_bases.as_mut_slice().sort_unstable();

        Self {
            unguarded_by_key,
            guarded_ids: guarded_ids.finish(),
            state_specific_messages,
            payload_specific_bases,
        }
    }

    pub(super) fn for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
        transitions: &[ExecutableTransition<'_>],
    ) -> Option<ExecutableTransitionId> {
        let current_state = self
            .is_state_specific_message(message)
            .then_some(current_state);
        let key = ExecutableTransitionKey {
            message: message.as_u32(),
            current_state: current_state.map(StateId::as_u32),
        };
        if self.is_payload_specific_key(key) {
            let payload = payload?;
            self.guarded_ids.iter().copied().find(|id| {
                let Some(transition) = transitions.get(id.index()) else {
                    return false;
                };
                transition.message == message
                    && transition.current_state == current_state
                    && transition.payload_matches(payload)
            })
        } else {
            self.unguarded_by_key
                .binary_search_by_key(&key, |entry| entry.key)
                .ok()
                .map(|index| self.unguarded_by_key[index].transition)
        }
    }

    pub(super) fn is_state_specific_message(&self, message: MessageId) -> bool {
        self.state_specific_messages
            .binary_search(&message.as_u32())
            .is_ok()
    }

    pub(super) fn is_payload_specific_base(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
    ) -> bool {
        self.is_payload_specific_key(ExecutableTransitionKey {
            message: message.as_u32(),
            current_state: current_state.map(StateId::as_u32),
        })
    }

    fn is_payload_specific_key(&self, key: ExecutableTransitionKey) -> bool {
        self.payload_specific_bases.binary_search(&key).is_ok()
    }
}

#[derive(Debug, Default)]
struct ExecutableDispatchCounts {
    unguarded_by_key: usize,
    guarded_ids: usize,
    state_specific_messages: usize,
    payload_specific_bases: usize,
}

impl ExecutableDispatchCounts {
    fn from_transitions(transitions: &[ExecutableTransition<'_>]) -> Self {
        transitions
            .iter()
            .fold(Self::default(), |mut counts, transition| {
                if transition.current_state.is_some() {
                    counts.state_specific_messages += 1;
                }
                if transition.payload_guard.is_some() {
                    counts.guarded_ids += 1;
                    counts.payload_specific_bases += 1;
                } else {
                    counts.unguarded_by_key += 1;
                }
                counts
            })
    }
}
