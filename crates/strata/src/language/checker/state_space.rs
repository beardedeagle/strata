mod canonical;
mod templates;
#[cfg(test)]
mod tests;

use mantle_artifact::{ArtifactValue, MAX_STATE_VALUES_PER_PROCESS};

use super::super::ast::{Identifier, Module, Process, TypeRef, ValueExpr};
use super::super::checked::{
    CheckedPayloadValue, CheckedStateId, CheckedStateValue, CheckedTypeRef,
};
use super::super::diagnostic::{Error, Result};
use super::CheckedTypeInterner;
use super::STEP_STATE_PARAMETER_NAME;
use super::symbols::SemanticIndex;
use canonical::{CanonicalValueContext, canonical_value};
pub(super) use canonical::{
    ValueBinding, canonical_source_value_with_bindings, source_value_uses_binding,
};
use canonical::{reject_reserved_state_values, validate_state_value_count};
pub(super) use templates::{
    ValueTemplateBinding, ValueTemplateSource, checked_value_template_with_binding,
};

pub(super) struct StateSpace<'module> {
    module: &'module Module,
    process_name: &'module Identifier,
    state_type: &'module TypeRef,
    checked_state_type: CheckedTypeRef,
    values: Vec<CheckedStateValue>,
}

impl<'module> StateSpace<'module> {
    pub(super) fn new(
        module: &'module Module,
        semantic_index: &SemanticIndex,
        process: &'module Process,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Self> {
        let checked_state_type = types.intern(&process.state_type)?;
        if semantic_index.is_unit_type(&process.state_type)? {
            return Ok(Self {
                module,
                process_name: &process.name,
                state_type: &process.state_type,
                checked_state_type: checked_state_type.clone(),
                values: vec![CheckedStateValue::new(
                    checked_state_type,
                    ArtifactValue::Atom("Unit".to_string()),
                )],
            });
        }
        if let Ok(record) = semantic_index.record_decl(module, &process.state_type) {
            let values = if record.fields.is_empty() {
                vec![CheckedStateValue::new(
                    checked_state_type.clone(),
                    ArtifactValue::Atom(record.name.to_string()),
                )]
            } else {
                Vec::new()
            };
            return Ok(Self {
                module,
                process_name: &process.name,
                state_type: &process.state_type,
                checked_state_type,
                values,
            });
        }
        if semantic_index
            .collection_type(&process.state_type)?
            .is_some()
        {
            return Ok(Self {
                module,
                process_name: &process.name,
                state_type: &process.state_type,
                checked_state_type,
                values: Vec::new(),
            });
        }

        let enum_decl = semantic_index.enum_decl(module, &process.state_type)?;
        if enum_decl.variants.is_empty() {
            return Err(Error::new(format!(
                "enum {} must declare at least one variant",
                enum_decl.name
            )));
        }
        for variant in &enum_decl.variants {
            if variant.name.as_str() == STEP_STATE_PARAMETER_NAME {
                return Err(Error::new(format!(
                    "process {} state value {} conflicts with reserved step state parameter name",
                    process.name, STEP_STATE_PARAMETER_NAME
                )));
            }
        }
        let values = enum_decl
            .variants
            .iter()
            .filter(|variant| variant.payload_type.is_none())
            .map(|variant| {
                CheckedStateValue::enum_variant(
                    checked_state_type.clone(),
                    ArtifactValue::Atom(variant.name.to_string()),
                    None,
                )
            })
            .collect();
        Ok(Self {
            module,
            process_name: &process.name,
            state_type: &process.state_type,
            checked_state_type,
            values,
        })
    }

    pub(super) fn resolve_state_value(
        &mut self,
        semantic_index: &SemanticIndex,
        types: &mut CheckedTypeInterner<'_>,
        value: &ValueExpr,
    ) -> Result<CheckedStateId> {
        self.resolve_state_value_with_bindings(semantic_index, types, value, &[])
    }

    pub(super) fn resolve_state_value_with_bindings(
        &mut self,
        semantic_index: &SemanticIndex,
        types: &mut CheckedTypeInterner<'_>,
        value: &ValueExpr,
        bindings: &[ValueBinding<'_>],
    ) -> Result<CheckedStateId> {
        let resolved_value = canonical_value(
            self.module,
            semantic_index,
            self.state_type,
            value,
            bindings,
            CanonicalValueContext::StateValue,
            0,
        )?;
        let state_value =
            self.checked_state_value(semantic_index, types, value, bindings, resolved_value)?;
        if let Some(index) = self.values.iter().position(|candidate| {
            candidate.ty() == state_value.ty() && candidate.value() == state_value.value()
        }) {
            return CheckedStateId::from_index(index);
        }
        if self.values.len() >= MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
                self.process_name
            )));
        }
        self.values.push(state_value);
        CheckedStateId::from_index(self.values.len() - 1)
    }

    pub(super) fn values(&self) -> &[CheckedStateValue] {
        &self.values
    }

    fn checked_state_value(
        &self,
        semantic_index: &SemanticIndex,
        types: &mut CheckedTypeInterner<'_>,
        value: &ValueExpr,
        bindings: &[ValueBinding<'_>],
        state_value: ArtifactValue,
    ) -> Result<CheckedStateValue> {
        let Ok(enum_decl) = semantic_index.enum_decl(self.module, self.state_type) else {
            return Ok(CheckedStateValue::new(
                self.checked_state_type.clone(),
                state_value,
            ));
        };

        match value {
            ValueExpr::Identifier(name) => {
                let Some(variant) = enum_decl
                    .variants
                    .iter()
                    .find(|variant| variant.name == *name)
                else {
                    return Ok(CheckedStateValue::new(
                        self.checked_state_type.clone(),
                        state_value,
                    ));
                };
                if variant.payload_type.is_some() {
                    return Ok(CheckedStateValue::new(
                        self.checked_state_type.clone(),
                        state_value,
                    ));
                }
                Ok(CheckedStateValue::enum_variant(
                    self.checked_state_type.clone(),
                    state_value,
                    None,
                ))
            }
            ValueExpr::EnumVariant { name, payload } => {
                let Some(variant) = enum_decl
                    .variants
                    .iter()
                    .find(|variant| variant.name == *name)
                else {
                    return Ok(CheckedStateValue::new(
                        self.checked_state_type.clone(),
                        state_value,
                    ));
                };
                let Some(payload_type) = &variant.payload_type else {
                    return Ok(CheckedStateValue::enum_variant(
                        self.checked_state_type.clone(),
                        state_value,
                        None,
                    ));
                };
                let payload_value = canonical_value(
                    self.module,
                    semantic_index,
                    payload_type,
                    payload,
                    bindings,
                    CanonicalValueContext::StateValue,
                    0,
                )?;
                Ok(CheckedStateValue::enum_variant(
                    self.checked_state_type.clone(),
                    state_value,
                    Some(CheckedPayloadValue::new(
                        types.intern(payload_type)?,
                        payload_value,
                    )),
                ))
            }
            ValueExpr::Call { .. }
            | ValueExpr::StringLiteral(_)
            | ValueExpr::BytesLiteral(_)
            | ValueExpr::ScalarLiteral(_)
            | ValueExpr::Record(_)
            | ValueExpr::List(_)
            | ValueExpr::Map(_) => Ok(CheckedStateValue::new(
                self.checked_state_type.clone(),
                state_value,
            )),
            ValueExpr::Equality { .. } => Err(Error::new(
                "equality expression must be resolved before checking state value",
            )),
            ValueExpr::ScalarArithmetic { .. } | ValueExpr::ScalarOrdering { .. } => Err(
                Error::new("scalar expression must be resolved before checking state value"),
            ),
            ValueExpr::BooleanNot { .. } | ValueExpr::BooleanBinary { .. } => Err(Error::new(
                "boolean predicate expression must be resolved before checking state value",
            )),
            ValueExpr::Grouped { .. } => Err(Error::new(
                "parenthesized value expression must be resolved before checking state value",
            )),
            ValueExpr::IfElse { .. } => Err(Error::new(
                "if expression must be resolved before checking state value",
            )),
        }
    }

    pub(super) fn into_values(self) -> Result<Vec<CheckedStateValue>> {
        validate_state_value_count(self.process_name, self.values.len())?;
        reject_reserved_state_values(self.process_name, &self.values)?;
        Ok(self.values)
    }
}
