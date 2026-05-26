use mantle_artifact::ArtifactValue;

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{Identifier, Module, Record, TypeRef, ValueExpr};
use super::super::super::checked::{
    CheckedEffectOutcomeId, CheckedEnumVariantId, CheckedLoopElementId, CheckedPayloadValue,
    CheckedTypeRef, CheckedValueTemplate, CheckedValueTemplateField,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::CheckedTypeInterner;
use super::super::symbols::SemanticIndex;
use super::super::{PayloadBindingPath, PayloadProjectionSegmentKind};
use super::canonical::{CanonicalValueContext, canonical_value, source_value_uses_binding};

mod collections;
mod equality;
mod scalars;

use collections::checked_collection_template;
use equality::{
    CheckedBinaryTemplate, CheckedEqualityTemplate, CheckedTemplateInput,
    checked_boolean_binary_template, checked_boolean_not_template, checked_equality_template,
    checked_grouped_template,
};
use scalars::{
    CheckedScalarArithmeticTemplate, CheckedScalarOrderingTemplate,
    checked_scalar_arithmetic_template, checked_scalar_ordering_template,
};

#[derive(Clone, Copy)]
pub(in crate::language::checker) struct ValueTemplateBinding<'a> {
    pub(in crate::language::checker) name: &'a Identifier,
    pub(in crate::language::checker) ty: &'a TypeRef,
    pub(in crate::language::checker) checked_ty: &'a CheckedTypeRef,
    pub(in crate::language::checker) root_checked_ty: &'a CheckedTypeRef,
    pub(in crate::language::checker) source: ValueTemplateSource,
    pub(in crate::language::checker) path: &'a PayloadBindingPath,
}

#[derive(Clone, Copy)]
pub(in crate::language::checker) enum ValueTemplateSource {
    ReceivedPayload,
    CurrentStatePayload,
    LoopElement(CheckedLoopElementId),
    EffectOutcome(CheckedEffectOutcomeId),
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
                return checked_binding_value_template(types, binding);
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
    if let ValueExpr::IfElse {
        condition,
        then_branch,
        else_branch,
    } = value
    {
        let bool_type = semantic_index.bool_type(module)?;
        return Ok(CheckedValueTemplate::IfElse {
            ty: types.intern(expected_type)?,
            condition: Box::new(checked_value_template(
                module,
                semantic_index,
                types,
                &bool_type,
                condition,
                bindings,
                depth + 1,
            )?),
            then_value: Box::new(checked_value_template(
                module,
                semantic_index,
                types,
                expected_type,
                then_branch,
                bindings,
                depth + 1,
            )?),
            else_value: Box::new(checked_value_template(
                module,
                semantic_index,
                types,
                expected_type,
                else_branch,
                bindings,
                depth + 1,
            )?),
        });
    }
    if let ValueExpr::Equality {
        operator,
        left,
        right,
    } = value
    {
        return checked_equality_template(
            types,
            CheckedTemplateInput {
                module,
                semantic_index,
                expected_type,
                bindings,
                depth: depth + 1,
            },
            CheckedEqualityTemplate {
                operator: *operator,
                left,
                right,
            },
        );
    }
    if let ValueExpr::ScalarArithmetic {
        operator,
        left,
        right,
    } = value
    {
        return checked_scalar_arithmetic_template(
            types,
            CheckedTemplateInput {
                module,
                semantic_index,
                expected_type,
                bindings,
                depth: depth + 1,
            },
            CheckedScalarArithmeticTemplate {
                operator: *operator,
                left,
                right,
            },
        );
    }
    if let ValueExpr::ScalarOrdering {
        operator,
        left,
        right,
    } = value
    {
        return checked_scalar_ordering_template(
            types,
            CheckedTemplateInput {
                module,
                semantic_index,
                expected_type,
                bindings,
                depth: depth + 1,
            },
            CheckedScalarOrderingTemplate {
                operator: *operator,
                left,
                right,
            },
        );
    }
    if let ValueExpr::BooleanNot { operand } = value {
        return checked_boolean_not_template(
            module,
            semantic_index,
            types,
            expected_type,
            operand,
            bindings,
            depth + 1,
        );
    }
    if let ValueExpr::BooleanBinary {
        operator,
        left,
        right,
    } = value
    {
        return checked_boolean_binary_template(
            types,
            CheckedTemplateInput {
                module,
                semantic_index,
                expected_type,
                bindings,
                depth: depth + 1,
            },
            CheckedBinaryTemplate {
                operator: *operator,
                left,
                right,
            },
        );
    }
    if let ValueExpr::Grouped { value } = value {
        return checked_grouped_template(
            module,
            semantic_index,
            types,
            expected_type,
            value,
            bindings,
            depth + 1,
        );
    }

    if !bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
    {
        let value = canonical_value(
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
            value,
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
    if semantic_index.collection_type(expected_type)?.is_some() {
        return checked_collection_template(
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
        RecordTemplateTarget {
            ty: expected_type,
            decl: record,
        },
        value,
        bindings,
        depth,
    )
}

fn checked_binding_value_template(
    types: &mut CheckedTypeInterner<'_>,
    binding: &ValueTemplateBinding<'_>,
) -> Result<CheckedValueTemplate> {
    let mut template = match binding.source {
        ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
            ty: binding.root_checked_ty.clone(),
        },
        ValueTemplateSource::CurrentStatePayload => CheckedValueTemplate::CurrentStatePayload {
            ty: binding.root_checked_ty.clone(),
        },
        ValueTemplateSource::LoopElement(element) => CheckedValueTemplate::LoopElement {
            ty: binding.root_checked_ty.clone(),
            element,
        },
        ValueTemplateSource::EffectOutcome(outcome) => CheckedValueTemplate::EffectOutcome {
            ty: binding.root_checked_ty.clone(),
            outcome,
        },
    };
    for segment in binding.path.segments() {
        let checked_ty = types.intern(&segment.ty)?;
        template = match &segment.kind {
            PayloadProjectionSegmentKind::EnumPayload { variant, .. } => {
                CheckedValueTemplate::EnumPayload {
                    ty: checked_ty,
                    value: Box::new(template),
                    variant: *variant,
                }
            }
            PayloadProjectionSegmentKind::RecordField { field } => {
                CheckedValueTemplate::RecordField {
                    ty: checked_ty,
                    record: Box::new(template),
                    field: field.clone(),
                }
            }
            PayloadProjectionSegmentKind::ListIndex { index, len } => {
                CheckedValueTemplate::ListElement {
                    ty: checked_ty,
                    list: Box::new(template),
                    index: *index,
                    len: *len,
                }
            }
            PayloadProjectionSegmentKind::ListPrefixIndex { index, prefix_len } => {
                CheckedValueTemplate::ListPrefixElement {
                    ty: checked_ty,
                    list: Box::new(template),
                    index: *index,
                    prefix_len: *prefix_len,
                }
            }
            PayloadProjectionSegmentKind::ListRest { prefix_len } => {
                CheckedValueTemplate::ListRest {
                    ty: checked_ty,
                    list: Box::new(template),
                    prefix_len: *prefix_len,
                }
            }
            PayloadProjectionSegmentKind::MapValue {
                key,
                keys,
                projection,
            } => CheckedValueTemplate::MapValue {
                ty: checked_ty,
                map: Box::new(template),
                key: key.clone(),
                keys: keys.clone(),
                projection: *projection,
            },
            PayloadProjectionSegmentKind::MapRest { excluded_keys } => {
                CheckedValueTemplate::MapRest {
                    ty: checked_ty,
                    map: Box::new(template),
                    excluded_keys: excluded_keys.clone(),
                }
            }
        };
    }
    Ok(template)
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
    let variant = semantic_index.value_enum_variant(module, expected_type, name)?;
    let Some(payload_type) = variant.payload_type.as_ref() else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    Ok(CheckedValueTemplate::EnumVariant {
        ty: types.intern(expected_type)?,
        variant: CheckedEnumVariantId::from_index(variant.index)?,
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

struct RecordTemplateTarget<'a> {
    ty: &'a TypeRef,
    decl: &'a Record,
}

fn checked_record_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    target: RecordTemplateTarget<'_>,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let record = target.decl;
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

    let mut fields = Vec::with_capacity(record.fields.len());
    for (index, field) in value.fields.iter().enumerate() {
        if value.fields[..index]
            .iter()
            .any(|previous| previous.name == field.name)
        {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        let Some(record_field) = record
            .fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            return Err(Error::new(format!(
                "record value {} declares unknown field {}",
                record.name, field.name
            )));
        };
        fields.push(CheckedValueTemplateField::new(
            field.name.clone(),
            checked_value_template(
                module,
                semantic_index,
                types,
                &record_field.ty,
                &field.value,
                bindings,
                depth + 1,
            )?,
        ));
    }
    for field in &record.fields {
        if !value
            .fields
            .iter()
            .any(|provided| provided.name == field.name)
        {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        }
    }

    Ok(CheckedValueTemplate::Record {
        ty: types.intern(target.ty)?,
        fields,
    })
}
