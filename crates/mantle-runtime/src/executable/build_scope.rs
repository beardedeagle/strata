use mantle_artifact::{EffectOutcomeId, Error, MessageId, ProcessRefId, Result, TypeId};

use super::templates::ExecutableTemplateScope;
use crate::program::{LoadedLoopElement, LoadedProcess, LoadedTransition};

#[derive(Debug)]
pub(super) struct ExecutableTemplateBindings {
    received_payload_type: Option<TypeId>,
    current_state_payload_type: Option<TypeId>,
    spawned_refs: Vec<bool>,
    effect_outcomes: Vec<(EffectOutcomeId, TypeId)>,
}

impl ExecutableTemplateBindings {
    #[cfg(test)]
    pub(super) fn for_test(process: &LoadedProcess) -> Self {
        Self {
            received_payload_type: process
                .message_variants
                .first()
                .and_then(|variant| variant.payload_type),
            current_state_payload_type: process
                .state_values
                .get(process.init_state.index())
                .and_then(|state| state.payload.as_ref().map(|payload| payload.ty)),
            spawned_refs: vec![false; process.process_refs.len()],
            effect_outcomes: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test_with_spawned_refs(
        process: &LoadedProcess,
        spawned_refs: &[bool],
    ) -> Result<Self> {
        if spawned_refs.len() != process.process_refs.len() {
            return Err(Error::new(format!(
                "test executable scope has {} process refs, expected {}",
                spawned_refs.len(),
                process.process_refs.len()
            )));
        }
        let mut bindings = Self::for_test(process);
        bindings.spawned_refs.copy_from_slice(spawned_refs);
        Ok(bindings)
    }

    pub(super) fn new(process: &LoadedProcess, transition: &LoadedTransition) -> Result<Self> {
        let received_payload_type = process
            .message_variants
            .get(transition.message.index())
            .ok_or_else(|| unloaded_message_error(process, transition.message))?
            .payload_type;
        let current_state_payload_type = match transition.current_state {
            Some(current_state) => process
                .state_values
                .get(current_state.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} executable current_state id {} is not loaded",
                        process.debug_name,
                        current_state.as_u32()
                    ))
                })?
                .payload
                .as_ref()
                .map(|payload| payload.ty),
            None => None,
        };

        Ok(Self {
            received_payload_type,
            current_state_payload_type,
            spawned_refs: vec![false; process.process_refs.len()],
            effect_outcomes: Vec::new(),
        })
    }

    pub(super) fn template_scope<'scope>(
        &'scope self,
        loop_elements: &'scope [LoadedLoopElement],
        allow_direct_process_ref: bool,
    ) -> ExecutableTemplateScope<'scope> {
        ExecutableTemplateScope::new(
            self.received_payload_type,
            self.current_state_payload_type,
            loop_elements,
            &self.effect_outcomes,
            &self.spawned_refs,
            allow_direct_process_ref,
        )
    }

    pub(super) fn bind_process_ref(
        &mut self,
        process: &LoadedProcess,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        let Some(is_spawned) = self.spawned_refs.get_mut(process_ref.index()) else {
            return Err(Error::new(format!(
                "process {} executable spawn references unloaded process reference id {}",
                process.debug_name,
                process_ref.as_u32()
            )));
        };
        if *is_spawned {
            return Err(Error::new(format!(
                "process {} executable spawn duplicates process reference id {}",
                process.debug_name,
                process_ref.as_u32()
            )));
        }
        *is_spawned = true;
        Ok(())
    }

    pub(super) fn bind_effect_outcome(
        &mut self,
        process: &LoadedProcess,
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
    ) -> Result<()> {
        if self.effect_outcomes.iter().any(|(id, _)| *id == outcome) {
            return Err(Error::new(format!(
                "process {} executable transition duplicates effect outcome id {}",
                process.debug_name,
                outcome.as_u32()
            )));
        }
        self.effect_outcomes.push((outcome, outcome_ty));
        Ok(())
    }
}

fn unloaded_message_error(process: &LoadedProcess, message: MessageId) -> Error {
    Error::new(format!(
        "process {} executable message id {} is not loaded",
        process.debug_name,
        message.as_u32()
    ))
}
