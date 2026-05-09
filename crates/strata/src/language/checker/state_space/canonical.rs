use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{MAX_STATE_VALUES_PER_PROCESS, validate_state_value_label};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{Identifier, Module, Record, TypeRef, ValueExpr};
use super::super::super::checked::CheckedStateValue;
use super::super::super::diagnostic::{Error, Result};
use super::super::STEP_STATE_PARAMETER_NAME;
use super::super::symbols::SemanticIndex;

pub(in crate::language::checker) struct ValueBinding<'a> {
    pub(in crate::language::checker) name: &'a Identifier,
    pub(in crate::language::checker) ty: &'a TypeRef,
    pub(in crate::language::checker) label: &'a str,
}

#[derive(Clone, Copy)]
pub(super) enum CanonicalValueContext {
    StateValue,
    SourceValue,
}

impl CanonicalValueContext {
    fn process_ref_error(self) -> Error {
        match self {
            Self::StateValue => Error::new("process reference payloads are not valid state values"),
            Self::SourceValue => Error::new("process references must be direct message payloads"),
        }
    }
}

pub(in crate::language::checker) fn canonical_source_value_with_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
) -> Result<String> {
    canonical_value(
        module,
        semantic_index,
        expected_type,
        value,
        bindings,
        CanonicalValueContext::SourceValue,
        0,
    )
}

pub(in crate::language::checker) fn source_value_uses_binding(
    value: &ValueExpr,
    binding: &Identifier,
) -> bool {
    match value {
        ValueExpr::Identifier(name) => name == binding,
        ValueExpr::Call { arg, .. } => source_value_uses_binding(arg, binding),
        ValueExpr::EnumVariant { payload, .. } => source_value_uses_binding(payload, binding),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_uses_binding(&field.value, binding)),
    }
}

pub(super) fn canonical_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<String> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if semantic_index
        .process_ref_target_type(expected_type)?
        .is_some()
    {
        return Err(context.process_ref_error());
    }
    if let ValueExpr::Identifier(name) = value {
        if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
            if semantic_index.same_type(binding.ty, expected_type) {
                return Ok(binding.label.to_string());
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value of type {expected_type}"
        )));
    }
    if let Ok(record) = semantic_index.record_decl(module, expected_type) {
        return canonical_record_value(
            module,
            semantic_index,
            record,
            value,
            bindings,
            context,
            depth,
        );
    }

    canonical_enum_value(
        module,
        semantic_index,
        expected_type,
        value,
        bindings,
        context,
        depth,
    )
}

fn canonical_enum_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<String> {
    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
    match value {
        ValueExpr::Identifier(name) => {
            if let Some(variant) = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name == *name)
            {
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "enum variant {} requires a payload and cannot be used as a fieldless value",
                        variant.name
                    )));
                }
                return Ok(name.to_string());
            }
            Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )))
        }
        ValueExpr::EnumVariant { name, payload } => {
            let variant = enum_decl
                .variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| {
                    Error::new(format!(
                        "value {name} is not a variant of enum {}",
                        enum_decl.name
                    ))
                })?;
            let Some(payload_type) = &variant.payload_type else {
                return Err(Error::new(format!(
                    "enum variant {name} does not accept a payload"
                )));
            };
            let payload = canonical_value(
                module,
                semantic_index,
                payload_type,
                payload,
                bindings,
                context,
                depth + 1,
            )?;
            let value = format!("{name}({payload})");
            validate_state_value_label(&value).map_err(|err| Error::new(err.to_string()))?;
            Ok(value)
        }
        ValueExpr::Call { .. } | ValueExpr::Record(_) => Err(Error::new(format!(
            "expected enum variant value for enum {}",
            enum_decl.name
        ))),
    }
}

fn canonical_record_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    record: &Record,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    context: CanonicalValueContext,
    depth: usize,
) -> Result<String> {
    if let ValueExpr::Record(value) = value {
        if value.fields.is_empty() {
            return Err(Error::new(format!(
                "fieldless record values use `{}`; braced record values must declare at least one field",
                value.name
            )));
        }
    }

    if record.fields.is_empty() {
        return match value {
            ValueExpr::Identifier(name) if name == &record.name => Ok(record.name.to_string()),
            _ => Err(Error::new(format!(
                "provided value is not a value of record {}",
                record.name
            ))),
        };
    }

    let ValueExpr::Record(value) = value else {
        return Err(Error::new(format!(
            "record state type {} must be constructed with {} {{ ... }}",
            record.name, record.name
        )));
    };
    if value.name != record.name {
        return Err(Error::new(format!(
            "record constructor {} does not match expected record {}",
            value.name, record.name
        )));
    }

    let declared_fields = record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut provided = BTreeMap::new();
    for field in &value.fields {
        if provided.insert(field.name.as_str(), &field.value).is_some() {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        if !declared_fields.contains(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} declares unknown field {}",
                record.name, field.name
            )));
        }
    }

    let mut parts = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(value) = provided.get(field.name.as_str()) else {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        };
        let field_value = canonical_value(
            module,
            semantic_index,
            &field.ty,
            value,
            bindings,
            context,
            depth + 1,
        )?;
        parts.push(format!("{}:{field_value}", field.name));
    }
    let label = format!("{}{{{}}}", record.name, parts.join(","));
    validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
    Ok(label)
}

pub(super) fn validate_state_value_count(process_name: &Identifier, count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process_name
        )));
    }
    if count > MAX_STATE_VALUES_PER_PROCESS {
        return Err(Error::new(format!(
            "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
            process_name
        )));
    }
    Ok(())
}

pub(super) fn reject_reserved_state_values(
    process_name: &Identifier,
    state_values: &[CheckedStateValue],
) -> Result<()> {
    if state_values
        .iter()
        .any(|value| value.label() == STEP_STATE_PARAMETER_NAME)
    {
        return Err(Error::new(format!(
            "process {} state value {} conflicts with reserved step state parameter name",
            process_name, STEP_STATE_PARAMETER_NAME
        )));
    }
    Ok(())
}
