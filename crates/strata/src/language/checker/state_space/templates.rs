use std::collections::{BTreeMap, BTreeSet};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{Identifier, Module, Record, TypeRef, ValueExpr};
use super::super::super::checked::{
    CheckedPayloadValue, CheckedTypeRef, CheckedValueTemplate, CheckedValueTemplateField,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::CheckedTypeInterner;
use super::super::symbols::SemanticIndex;
use super::canonical::{CanonicalValueContext, canonical_value, source_value_uses_binding};

#[derive(Clone, Copy)]
pub(in crate::language::checker) struct ValueTemplateBinding<'a> {
    pub(in crate::language::checker) name: &'a Identifier,
    pub(in crate::language::checker) ty: &'a TypeRef,
    pub(in crate::language::checker) checked_ty: &'a CheckedTypeRef,
    pub(in crate::language::checker) source: ValueTemplateSource,
}

#[derive(Clone, Copy)]
pub(in crate::language::checker) enum ValueTemplateSource {
    ReceivedPayload,
    CurrentStatePayload,
}

pub(in crate::language::checker) fn checked_value_template_with_binding(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedValueTemplate> {
    checked_value_template(
        module,
        semantic_index,
        types,
        expected_type,
        value,
        bindings,
        0,
    )
}

fn checked_value_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }

    if let ValueExpr::Identifier(name) = value {
        if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
            if semantic_index.same_type(binding.ty, expected_type) {
                return Ok(match binding.source {
                    ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
                        ty: binding.checked_ty.clone(),
                    },
                    ValueTemplateSource::CurrentStatePayload => {
                        CheckedValueTemplate::CurrentStatePayload {
                            ty: binding.checked_ty.clone(),
                        }
                    }
                });
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value template of type {expected_type}"
        )));
    }

    if !bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
    {
        let label = canonical_value(
            module,
            semantic_index,
            expected_type,
            value,
            &[],
            CanonicalValueContext::SourceValue,
            depth,
        )?;
        return Ok(CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            types.intern(expected_type)?,
            label,
        )));
    }

    if matches!(value, ValueExpr::EnumVariant { .. }) {
        return checked_enum_variant_template(
            module,
            semantic_index,
            types,
            expected_type,
            value,
            bindings,
            depth,
        );
    }

    let record = semantic_index.record_decl(module, expected_type)?;
    checked_record_template(
        module,
        semantic_index,
        types,
        record,
        value,
        bindings,
        depth,
    )
}

fn checked_enum_variant_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let ValueExpr::EnumVariant { name, payload } = value else {
        return Err(Error::new(format!(
            "expected enum variant value for enum {expected_type}"
        )));
    };
    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
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
    Ok(CheckedValueTemplate::EnumVariant {
        ty: types.intern(expected_type)?,
        variant: name.clone(),
        payload: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            payload_type,
            payload,
            bindings,
            depth + 1,
        )?),
    })
}

fn checked_record_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    record: &Record,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let ValueExpr::Record(value) = value else {
        return Err(Error::new(format!(
            "record type {} must be constructed with {} {{ ... }}",
            record.name, record.name
        )));
    };
    if value.fields.is_empty() {
        return Err(Error::new(format!(
            "fieldless record values use `{}`; braced record values must declare at least one field",
            value.name
        )));
    }
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

    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(value) = provided.get(field.name.as_str()) else {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        };
        fields.push(CheckedValueTemplateField::new(
            field.name.clone(),
            checked_value_template(
                module,
                semantic_index,
                types,
                &field.ty,
                value,
                bindings,
                depth + 1,
            )?,
        ));
    }

    Ok(CheckedValueTemplate::Record {
        ty: types.intern(&TypeRef::Named(record.name.clone()))?,
        fields,
    })
}
