use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField, ArtifactValue};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{
    Identifier, ListValue, MapValue, Module, Record, TypeRef, ValueBooleanOperator,
    ValueEqualityOperator, ValueExpr,
};
use super::super::super::checked::{
    CheckedEnumVariantId, CheckedLoopElementId, CheckedPayloadValue, CheckedTypeRef,
    CheckedValueBooleanOperator, CheckedValueEqualityOperator, CheckedValueTemplate,
    CheckedValueTemplateField, CheckedValueTemplateMapEntry,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::CheckedTypeInterner;
use super::super::symbols::{CollectionType, SemanticIndex};
use super::super::{PayloadBindingPath, PayloadProjectionSegmentKind};
use super::canonical::{CanonicalValueContext, canonical_value, source_value_uses_binding};

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
    if matches!(value, ValueExpr::IfElse { .. }) {
        return Err(Error::new(format!(
            "if expression must be resolved before checking value template of type {expected_type}"
        )));
    }
    if let ValueExpr::Equality {
        operator,
        left,
        right,
    } = value
    {
        return checked_equality_template(
            module,
            semantic_index,
            types,
            expected_type,
            *operator,
            left,
            right,
            bindings,
            depth + 1,
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
            module,
            semantic_index,
            types,
            expected_type,
            *operator,
            left,
            right,
            bindings,
            depth + 1,
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
        record,
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

#[allow(clippy::too_many_arguments)]
fn checked_equality_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operator: ValueEqualityOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    if !semantic_index.same_type(expected_type, &bool_type) {
        return Err(Error::new(format!(
            "equality expression produces {bool_type}, expected {expected_type}"
        )));
    }
    let operand_type =
        equality_template_operand_pair_type(module, semantic_index, left, right, bindings)?;
    validate_equality_template_operand_type(module, semantic_index, &operand_type)?;
    let operand_ty = types.intern(&operand_type)?;
    Ok(CheckedValueTemplate::Equality {
        ty: types.intern(&bool_type)?,
        operand_ty,
        operator: checked_equality_operator(operator),
        left: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &operand_type,
            left,
            bindings,
            depth + 1,
        )?),
        right: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &operand_type,
            right,
            bindings,
            depth + 1,
        )?),
    })
}

fn checked_equality_operator(operator: ValueEqualityOperator) -> CheckedValueEqualityOperator {
    match operator {
        ValueEqualityOperator::Equal => CheckedValueEqualityOperator::Equal,
        ValueEqualityOperator::NotEqual => CheckedValueEqualityOperator::NotEqual,
    }
}

fn checked_boolean_not_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operand: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    let ty = types.intern(&bool_type)?;
    Ok(CheckedValueTemplate::BooleanNot {
        ty,
        operand: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            operand,
            bindings,
            depth + 1,
        )?),
    })
}

#[allow(clippy::too_many_arguments)]
fn checked_boolean_binary_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    operator: ValueBooleanOperator,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    let ty = types.intern(&bool_type)?;
    Ok(CheckedValueTemplate::BooleanBinary {
        ty,
        operator: checked_boolean_operator(operator),
        left: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            left,
            bindings,
            depth + 1,
        )?),
        right: Box::new(checked_value_template(
            module,
            semantic_index,
            types,
            &bool_type,
            right,
            bindings,
            depth + 1,
        )?),
    })
}

fn validate_boolean_template_result_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    bool_type: &TypeRef,
) -> Result<()> {
    if semantic_index.same_type(expected_type, bool_type) {
        return Ok(());
    }
    Err(Error::new(format!(
        "boolean predicate expression produces {bool_type}, expected {expected_type}"
    )))
}

fn checked_boolean_operator(operator: ValueBooleanOperator) -> CheckedValueBooleanOperator {
    match operator {
        ValueBooleanOperator::And => CheckedValueBooleanOperator::And,
        ValueBooleanOperator::Or => CheckedValueBooleanOperator::Or,
    }
}

fn checked_grouped_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let bool_type = semantic_index.bool_type(module)?;
    validate_boolean_template_result_type(semantic_index, expected_type, &bool_type)?;
    checked_value_template(
        module,
        semantic_index,
        types,
        &bool_type,
        value,
        bindings,
        depth + 1,
    )
}

fn equality_template_operand_pair_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    left: &ValueExpr,
    right: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<TypeRef> {
    let left_type = equality_template_operand_type(module, semantic_index, left, bindings, None);
    let right_type = equality_template_operand_type(module, semantic_index, right, bindings, None);
    match (left_type, right_type) {
        (Ok(left_type), Ok(right_type)) => validate_matching_equality_template_operand_types(
            module,
            semantic_index,
            left_type,
            right_type,
        ),
        (Ok(left_type), Err(_)) => {
            validate_equality_template_operand_type(module, semantic_index, &left_type)?;
            let right_type = equality_template_operand_type(
                module,
                semantic_index,
                right,
                bindings,
                Some(&left_type),
            )?;
            validate_matching_equality_template_operand_types(
                module,
                semantic_index,
                left_type,
                right_type,
            )
        }
        (Err(_), Ok(right_type)) => {
            validate_equality_template_operand_type(module, semantic_index, &right_type)?;
            let left_type = equality_template_operand_type(
                module,
                semantic_index,
                left,
                bindings,
                Some(&right_type),
            )?;
            validate_matching_equality_template_operand_types(
                module,
                semantic_index,
                left_type,
                right_type,
            )
        }
        (Err(left_error), Err(_)) => Err(left_error),
    }
}

fn validate_matching_equality_template_operand_types(
    module: &Module,
    semantic_index: &SemanticIndex,
    left_type: TypeRef,
    right_type: TypeRef,
) -> Result<TypeRef> {
    validate_equality_template_operand_type(module, semantic_index, &left_type)?;
    validate_equality_template_operand_type(module, semantic_index, &right_type)?;
    if !semantic_index.same_type(&left_type, &right_type) {
        return Err(Error::new(format!(
            "equality operands must have the same type; left has {left_type}, right has {right_type}"
        )));
    }
    Ok(left_type)
}

fn equality_template_operand_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    expected_type: Option<&TypeRef>,
) -> Result<TypeRef> {
    match value {
        ValueExpr::Identifier(name) => {
            if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
                return Ok(binding.ty.clone());
            }
            if let Some(expected_type) = expected_type
                && equality_template_fieldless_variant_matches_type(
                    module,
                    semantic_index,
                    expected_type,
                    name,
                )?
            {
                return Ok(expected_type.clone());
            }
            semantic_index
                .equality_fieldless_enum_variant_type(module, name)
                .map_err(|err| {
                    Error::new(format!(
                        "equality operand {name} must be a Bool or fieldless enum value: {err}"
                    ))
                })
        }
        ValueExpr::EnumVariant { name, .. } => {
            if let Some(expected_type) = expected_type
                && let Some(variant) =
                    enum_variant_for_expected_type(module, semantic_index, expected_type, name)?
            {
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "equality operand enum variant {name} carries a payload"
                    )));
                }
                return Ok(expected_type.clone());
            }
            let ty = semantic_index.enum_variant_type(module, name)?;
            let enum_decl = semantic_index.enum_decl(module, &ty)?;
            let variant_index = semantic_index.enum_variant_index(module, &ty, name)?;
            let variant = enum_decl.variants.get(variant_index).ok_or_else(|| {
                Error::new(format!(
                    "enum {} variant index {variant_index} is not declared",
                    enum_decl.name
                ))
            })?;
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "equality operand enum variant {name} carries a payload"
                )));
            }
            Ok(ty)
        }
        ValueExpr::Call { .. }
        | ValueExpr::Record(_)
        | ValueExpr::List(_)
        | ValueExpr::Map(_)
        | ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => Err(Error::new(
            "equality operands must be Bool or fieldless enum values",
        )),
    }
}

fn equality_template_fieldless_variant_matches_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<bool> {
    let Some(variant) =
        enum_variant_for_expected_type(module, semantic_index, expected_type, name)?
    else {
        return Ok(false);
    };
    if variant.payload_type.is_some() {
        return Err(Error::new(format!(
            "equality operand enum variant {name} carries a payload"
        )));
    }
    Ok(true)
}

fn enum_variant_for_expected_type<'module>(
    module: &'module Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<&'module crate::language::ast::EnumVariant>> {
    let Ok(enum_decl) = semantic_index.enum_decl(module, expected_type) else {
        return Ok(None);
    };
    Ok(enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name))
}

fn validate_equality_template_operand_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    operand_type: &TypeRef,
) -> Result<()> {
    let bool_type = semantic_index.bool_type(module)?;
    if semantic_index.same_type(operand_type, &bool_type) {
        return Ok(());
    }
    if semantic_index
        .process_ref_target_type(operand_type)?
        .is_some()
    {
        return Err(Error::new("process-reference equality is not supported"));
    }
    if semantic_index.collection_type(operand_type)?.is_some() {
        return Err(Error::new(
            "list and map equality are not supported in this source slice",
        ));
    }
    if semantic_index.record_decl(module, operand_type).is_ok() {
        return Err(Error::new(
            "record equality is not supported in this source slice",
        ));
    }
    let enum_decl = semantic_index.enum_decl(module, operand_type)?;
    if enum_decl
        .variants
        .iter()
        .any(|variant| variant.payload_type.is_some())
    {
        return Err(Error::new(format!(
            "equality type {operand_type} must not declare payload-bearing enum variants"
        )));
    }
    Ok(())
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
    let variant_index = semantic_index.enum_variant_index(module, expected_type, name)?;
    let variant = enum_decl.variants.get(variant_index).ok_or_else(|| {
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
        variant: CheckedEnumVariantId::from_index(variant_index)?,
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
        .map(|field| (field.name.as_str(), &field.ty))
        .collect::<BTreeMap<_, _>>();
    let mut provided = BTreeSet::new();
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &value.fields {
        if !provided.insert(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        let Some(field_ty) = declared_fields.get(field.name.as_str()) else {
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
                field_ty,
                &field.value,
                bindings,
                depth + 1,
            )?,
        ));
    }
    for field in &record.fields {
        if !provided.contains(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        }
    }

    Ok(CheckedValueTemplate::Record {
        ty: types.intern(&TypeRef::Named(record.name.clone()))?,
        fields,
    })
}

fn checked_collection_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let collection_type = semantic_index
        .collection_type(expected_type)?
        .ok_or_else(|| Error::new(format!("type {expected_type} is not a collection type")))?;
    match collection_type {
        CollectionType::List { element, capacity } => {
            let ValueExpr::List(list) = value else {
                return Err(Error::new(format!(
                    "list value type {expected_type} must be constructed with List<T,N>[...]"
                )));
            };
            validate_list_value_type(semantic_index, expected_type, list, element, capacity)?;
            Ok(CheckedValueTemplate::List {
                ty: types.intern(expected_type)?,
                items: list
                    .items
                    .iter()
                    .map(|item| {
                        checked_value_template(
                            module,
                            semantic_index,
                            types,
                            element,
                            item,
                            bindings,
                            depth + 1,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        CollectionType::Map {
            key,
            value: item,
            capacity,
        } => {
            let ValueExpr::Map(map) = value else {
                return Err(Error::new(format!(
                    "map value type {expected_type} must be constructed with Map<K,V,N>[...]"
                )));
            };
            validate_map_value_type(semantic_index, expected_type, map, key, item, capacity)?;
            let mut seen_keys = BTreeSet::new();
            Ok(CheckedValueTemplate::Map {
                ty: types.intern(expected_type)?,
                entries: map
                    .entries
                    .iter()
                    .map(|entry| {
                        let key_template = checked_value_template(
                            module,
                            semantic_index,
                            types,
                            key,
                            &entry.key,
                            bindings,
                            depth + 1,
                        )?;
                        let key_label =
                            validate_static_map_key_template(expected_type, &key_template)?;
                        if !seen_keys.insert(key_label.clone()) {
                            return Err(Error::new(format!(
                                "map value type {expected_type} duplicates key {}",
                                key_label.label()
                            )));
                        }
                        Ok(CheckedValueTemplateMapEntry::new(
                            key_template,
                            checked_value_template(
                                module,
                                semantic_index,
                                types,
                                item,
                                &entry.value,
                                bindings,
                                depth + 1,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        }
    }
}

fn validate_list_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    list: &ListValue,
    element_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_element) = &list.element_type
        && !semantic_index.same_type(declared_element, element_type)
    {
        return Err(Error::new(format!(
            "list value has element type {declared_element}, expected {element_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = list.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "list value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if list.items.len() > capacity {
        return Err(Error::new(format!(
            "list value length {} exceeds capacity {capacity} for {expected_type}",
            list.items.len()
        )));
    }
    Ok(())
}

fn validate_map_value_type(
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    map: &MapValue,
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
) -> Result<()> {
    if let Some(declared_key) = &map.key_type
        && !semantic_index.same_type(declared_key, key_type)
    {
        return Err(Error::new(format!(
            "map value has key type {declared_key}, expected {key_type} for {expected_type}"
        )));
    }
    if let Some(declared_value) = &map.value_type
        && !semantic_index.same_type(declared_value, value_type)
    {
        return Err(Error::new(format!(
            "map value has value type {declared_value}, expected {value_type} for {expected_type}"
        )));
    }
    if let Some(declared_capacity) = map.capacity
        && declared_capacity != capacity
    {
        return Err(Error::new(format!(
            "map value has capacity {declared_capacity}, expected {capacity} for {expected_type}"
        )));
    }
    if map.entries.len() > capacity {
        return Err(Error::new(format!(
            "map value entry count {} exceeds capacity {capacity} for {expected_type}",
            map.entries.len()
        )));
    }
    Ok(())
}

fn validate_static_map_key_template(
    expected_type: &TypeRef,
    template: &CheckedValueTemplate,
) -> Result<ArtifactValue> {
    checked_static_source_value(template).ok_or_else(|| {
        Error::new(format!(
            "map value type {expected_type} keys must be static source values in this source slice"
        ))
    })
}

fn checked_static_source_value(template: &CheckedValueTemplate) -> Option<ArtifactValue> {
    match template {
        CheckedValueTemplate::Literal(value) => value.value().cloned(),
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::EnumPayload { .. }
        | CheckedValueTemplate::RecordField { .. }
        | CheckedValueTemplate::ListElement { .. }
        | CheckedValueTemplate::ListPrefixElement { .. }
        | CheckedValueTemplate::ListRest { .. }
        | CheckedValueTemplate::MapValue { .. }
        | CheckedValueTemplate::MapRest { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::LoopElement { .. }
        | CheckedValueTemplate::Equality { .. }
        | CheckedValueTemplate::BooleanNot { .. }
        | CheckedValueTemplate::BooleanBinary { .. } => None,
        CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => Some(ArtifactValue::EnumVariant {
            variant: ty.enum_variant_label(*variant).ok()?.to_string(),
            payload: Box::new(checked_static_source_value(payload)?),
        }),
        CheckedValueTemplate::Record { ty, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            let mut seen = BTreeSet::new();
            for field in fields {
                if !seen.insert(field.name()) {
                    return None;
                }
                values.push(ArtifactRecordField {
                    name: field.name().to_string(),
                    value: checked_static_source_value(field.value())?,
                });
            }
            Some(ArtifactValue::Record {
                constructor: ty.label().to_string(),
                fields: values,
            })
        }
        CheckedValueTemplate::List { items, .. } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(checked_static_source_value(item)?);
            }
            Some(ArtifactValue::List(values))
        }
        CheckedValueTemplate::Map { entries, .. } => {
            let mut values = Vec::with_capacity(entries.len());
            let mut seen = BTreeSet::new();
            for entry in entries {
                let key = checked_static_source_value(entry.key())?;
                let value = checked_static_source_value(entry.value())?;
                if !seen.insert(key.clone()) {
                    return None;
                }
                values.push(ArtifactMapEntry { key, value });
            }
            Some(ArtifactValue::Map(values))
        }
    }
}
