use mantle_artifact::{EffectOutcomeId, Error, LoopElementId, ProcessRefId, Result, TypeId};

use crate::program::{LoadedLoopElement, LoadedProcess};

#[derive(Debug, Clone, Copy)]
pub(in crate::executable) struct ExecutableTemplateScope<'scope> {
    received_payload_type: Option<TypeId>,
    current_state_payload_type: Option<TypeId>,
    loop_elements: &'scope [LoadedLoopElement],
    effect_outcomes: &'scope [(EffectOutcomeId, TypeId)],
    spawned_refs: &'scope [bool],
    allow_direct_process_ref: bool,
}

impl<'scope> ExecutableTemplateScope<'scope> {
    pub(in crate::executable) const fn new(
        received_payload_type: Option<TypeId>,
        current_state_payload_type: Option<TypeId>,
        loop_elements: &'scope [LoadedLoopElement],
        effect_outcomes: &'scope [(EffectOutcomeId, TypeId)],
        spawned_refs: &'scope [bool],
        allow_direct_process_ref: bool,
    ) -> Self {
        Self {
            received_payload_type,
            current_state_payload_type,
            loop_elements,
            effect_outcomes,
            spawned_refs,
            allow_direct_process_ref,
        }
    }

    pub(in crate::executable) const fn nested(self) -> Self {
        Self {
            allow_direct_process_ref: false,
            ..self
        }
    }

    pub(in crate::executable) const fn allows_direct_process_ref(self) -> bool {
        self.allow_direct_process_ref
    }

    pub(in crate::executable) fn validate_received_payload(self, ty: TypeId) -> Result<()> {
        let Some(received_payload_type) = self.received_payload_type else {
            return Err(Error::new(
                "executable received payload template requires a payload-bearing message",
            ));
        };
        if ty != received_payload_type {
            return Err(Error::new(format!(
                "executable received payload template has type id {}, expected {}",
                ty.as_u32(),
                received_payload_type.as_u32()
            )));
        }
        Ok(())
    }

    pub(in crate::executable) fn validate_current_state_payload(self, ty: TypeId) -> Result<()> {
        let Some(current_state_payload_type) = self.current_state_payload_type else {
            return Err(Error::new(
                "executable current state payload template requires a payload-bearing state",
            ));
        };
        if ty != current_state_payload_type {
            return Err(Error::new(format!(
                "executable current state payload template has type id {}, expected {}",
                ty.as_u32(),
                current_state_payload_type.as_u32()
            )));
        }
        Ok(())
    }

    pub(in crate::executable) fn validate_process_ref(
        self,
        process: &LoadedProcess,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        if !self.allow_direct_process_ref {
            return Err(Error::new(format!(
                "process {} executable process reference template id {} must be a direct message payload",
                process.debug_name,
                process_ref.as_u32()
            )));
        }
        let is_spawned = self
            .spawned_refs
            .get(process_ref.index())
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} executable process reference template id {} is not loaded",
                    process.debug_name,
                    process_ref.as_u32()
                ))
            })?;
        if !is_spawned {
            return Err(Error::new(format!(
                "process {} executable process reference template id {} is unbound",
                process.debug_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    pub(in crate::executable) fn validate_loop_element(
        self,
        ty: TypeId,
        element: LoopElementId,
    ) -> Result<()> {
        let Some(active) = self
            .loop_elements
            .iter()
            .find(|active| active.id == element)
        else {
            return Err(Error::new(format!(
                "executable template references inactive loop element id {}",
                element.as_u32()
            )));
        };
        if active.ty != ty {
            return Err(Error::new(format!(
                "executable template loop element id {} has type id {}, expected {}",
                element.as_u32(),
                active.ty.as_u32(),
                ty.as_u32()
            )));
        }
        Ok(())
    }

    pub(in crate::executable) fn validate_effect_outcome(
        self,
        ty: TypeId,
        outcome: EffectOutcomeId,
    ) -> Result<()> {
        let Some((_, expected_ty)) = self.effect_outcomes.iter().find(|(id, _)| *id == outcome)
        else {
            return Err(Error::new(format!(
                "executable template references unbound effect outcome id {}",
                outcome.as_u32()
            )));
        };
        if *expected_ty != ty {
            return Err(Error::new(format!(
                "executable template effect outcome id {} has type id {}, expected {}",
                outcome.as_u32(),
                expected_ty.as_u32(),
                ty.as_u32()
            )));
        }
        Ok(())
    }
}
