use std::collections::BTreeMap;

use super::*;

impl ArtifactProcess {
    pub(super) fn validate_effect_outcome_templates(
        &self,
        transition: &ArtifactTransition,
    ) -> Result<()> {
        let mut seen = BTreeMap::new();
        let mut prestate_prefix_open = true;
        for action in &transition.actions {
            self.validate_action_effect_outcome_templates(transition, action, &seen)?;
            match action {
                ArtifactAction::SpawnOutcome {
                    outcome,
                    outcome_ty,
                    ..
                }
                | ArtifactAction::SendOutcome {
                    outcome,
                    outcome_ty,
                    ..
                } => {
                    if !prestate_prefix_open {
                        return Err(Error::new(format!(
                            "process {} transition {} effect outcome id {} appears after ordinary effects",
                            self.debug_name,
                            transition.message.as_u32(),
                            outcome.as_u32()
                        )));
                    }
                    if outcome.index() >= MAX_EFFECT_OUTCOMES_PER_TRANSITION {
                        return Err(Error::new(format!(
                            "process {} transition {} effect outcome id {} must be less than {}",
                            self.debug_name,
                            transition.message.as_u32(),
                            outcome.as_u32(),
                            MAX_EFFECT_OUTCOMES_PER_TRANSITION
                        )));
                    }
                    if seen.len() >= MAX_EFFECT_OUTCOMES_PER_TRANSITION {
                        return Err(Error::new(format!(
                            "process {} transition {} binds more than {} effect outcomes",
                            self.debug_name,
                            transition.message.as_u32(),
                            MAX_EFFECT_OUTCOMES_PER_TRANSITION
                        )));
                    }
                    let previous_outcome_ty = seen.insert(*outcome, *outcome_ty);
                    if previous_outcome_ty.is_some() {
                        return Err(Error::new(format!(
                            "process {} transition {} duplicates effect outcome id {}",
                            self.debug_name,
                            transition.message.as_u32(),
                            outcome.as_u32()
                        )));
                    }
                }
                ArtifactAction::Spawn { .. } => {}
                ArtifactAction::Emit { .. }
                | ArtifactAction::Send { .. }
                | ArtifactAction::IfElse { .. }
                | ArtifactAction::ForEach { .. } => {
                    prestate_prefix_open = false;
                }
            }
        }
        self.validate_next_state_effect_outcome_templates(transition, &transition.next_state, &seen)
    }

    fn validate_next_state_effect_outcome_templates(
        &self,
        transition: &ArtifactTransition,
        next_state: &NextState,
        outcomes: &BTreeMap<EffectOutcomeId, TypeId>,
    ) -> Result<()> {
        match next_state {
            NextState::Current | NextState::Value(_) => Ok(()),
            NextState::Template(template) => self.validate_template_effect_outcome_refs(
                transition,
                "next_state template",
                template,
                outcomes,
            ),
            NextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                self.validate_template_effect_outcome_refs(
                    transition,
                    "next_state if condition",
                    condition,
                    outcomes,
                )?;
                self.validate_next_state_effect_outcome_templates(
                    transition, then_state, outcomes,
                )?;
                self.validate_next_state_effect_outcome_templates(transition, else_state, outcomes)
            }
        }
    }

    fn validate_action_effect_outcome_templates(
        &self,
        transition: &ArtifactTransition,
        action: &ArtifactAction,
        outcomes: &BTreeMap<EffectOutcomeId, TypeId>,
    ) -> Result<()> {
        match action {
            ArtifactAction::Emit { .. } | ArtifactAction::Spawn { .. } => Ok(()),
            ArtifactAction::SpawnOutcome { .. } => Ok(()),
            ArtifactAction::Send { payload, .. } | ArtifactAction::SendOutcome { payload, .. } => {
                if let Some(payload) = payload {
                    self.validate_template_effect_outcome_refs(
                        transition,
                        "send payload",
                        payload,
                        outcomes,
                    )?;
                }
                Ok(())
            }
            ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                self.validate_template_effect_outcome_refs(
                    transition,
                    "runtime if condition",
                    condition,
                    outcomes,
                )?;
                for action in then_actions {
                    self.validate_action_effect_outcome_templates(transition, action, outcomes)?;
                }
                for action in else_actions {
                    self.validate_action_effect_outcome_templates(transition, action, outcomes)?;
                }
                Ok(())
            }
            ArtifactAction::ForEach {
                collection, body, ..
            } => {
                self.validate_template_effect_outcome_refs(
                    transition,
                    "for collection",
                    collection,
                    outcomes,
                )?;
                for action in body {
                    self.validate_action_effect_outcome_templates(transition, action, outcomes)?;
                }
                Ok(())
            }
        }
    }

    fn validate_template_effect_outcome_refs(
        &self,
        transition: &ArtifactTransition,
        field: &str,
        template: &ArtifactValueTemplate,
        outcomes: &BTreeMap<EffectOutcomeId, TypeId>,
    ) -> Result<()> {
        match template {
            ArtifactValueTemplate::EffectOutcome { ty, outcome } => {
                let Some(expected_ty) = outcomes.get(outcome) else {
                    return Err(Error::new(format!(
                        "process {} transition {} {field} references unbound effect outcome id {}",
                        self.debug_name,
                        transition.message.as_u32(),
                        outcome.as_u32()
                    )));
                };
                if expected_ty != ty {
                    return Err(Error::new(format!(
                        "process {} transition {} {field} effect outcome id {} has type id {}, expected {}",
                        self.debug_name,
                        transition.message.as_u32(),
                        outcome.as_u32(),
                        ty.as_u32(),
                        expected_ty.as_u32()
                    )));
                }
                Ok(())
            }
            ArtifactValueTemplate::Literal { .. }
            | ArtifactValueTemplate::ReceivedPayload { .. }
            | ArtifactValueTemplate::CurrentStatePayload { .. }
            | ArtifactValueTemplate::ProcessRef { .. }
            | ArtifactValueTemplate::LoopElement { .. } => Ok(()),
            ArtifactValueTemplate::EnumPayload { value, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, value, outcomes)
            }
            ArtifactValueTemplate::RecordField { record, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, record, outcomes)
            }
            ArtifactValueTemplate::ListElement { list, .. }
            | ArtifactValueTemplate::ListPrefixElement { list, .. }
            | ArtifactValueTemplate::ListRest { list, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, list, outcomes)
            }
            ArtifactValueTemplate::MapValue { map, .. }
            | ArtifactValueTemplate::MapRest { map, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, map, outcomes)
            }
            ArtifactValueTemplate::EnumVariant { payload, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, payload, outcomes)
            }
            ArtifactValueTemplate::IfElse {
                condition,
                then_value,
                else_value,
                ..
            } => {
                self.validate_template_effect_outcome_refs(transition, field, condition, outcomes)?;
                self.validate_template_effect_outcome_refs(
                    transition, field, then_value, outcomes,
                )?;
                self.validate_template_effect_outcome_refs(transition, field, else_value, outcomes)
            }
            ArtifactValueTemplate::Record { fields, .. } => {
                for field_template in fields {
                    self.validate_template_effect_outcome_refs(
                        transition,
                        field,
                        &field_template.value,
                        outcomes,
                    )?;
                }
                Ok(())
            }
            ArtifactValueTemplate::List { items, .. } => {
                for item in items {
                    self.validate_template_effect_outcome_refs(transition, field, item, outcomes)?;
                }
                Ok(())
            }
            ArtifactValueTemplate::Map { entries, .. } => {
                for entry in entries {
                    self.validate_template_effect_outcome_refs(
                        transition, field, &entry.key, outcomes,
                    )?;
                    self.validate_template_effect_outcome_refs(
                        transition,
                        field,
                        &entry.value,
                        outcomes,
                    )?;
                }
                Ok(())
            }
            ArtifactValueTemplate::Equality { left, right, .. }
            | ArtifactValueTemplate::ScalarArithmetic { left, right, .. }
            | ArtifactValueTemplate::ScalarOrdering { left, right, .. }
            | ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, left, outcomes)?;
                self.validate_template_effect_outcome_refs(transition, field, right, outcomes)
            }
            ArtifactValueTemplate::BooleanNot { operand, .. } => {
                self.validate_template_effect_outcome_refs(transition, field, operand, outcomes)
            }
        }
    }
}
