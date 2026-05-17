use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField, ArtifactValue};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{
    Identifier, ListValue, MapValue, Module, Record, TypeRef, ValueExpr,
};
use super::super::super::checked::{
    CheckedEnumVariantId, CheckedLoopElementId, CheckedPayloadValue, CheckedTypeRef,
    CheckedValueTemplate, CheckedValueTemplateField, CheckedValueTemplateMapEntry,
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
        | CheckedValueTemplate::LoopElement { .. } => None,
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
