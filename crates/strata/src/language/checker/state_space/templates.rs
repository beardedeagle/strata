use std::collections::{BTreeMap, BTreeSet};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{
    Identifier, ListValue, MapValue, Module, Record, TypeRef, ValueExpr,
};
use super::super::super::checked::{
    CheckedPayloadValue, CheckedTypeRef, CheckedValueTemplate, CheckedValueTemplateField,
    CheckedValueTemplateMapEntry,
};
use super::super::super::diagnostic::{Error, Result};
use super::super::CheckedTypeInterner;
use super::super::PayloadBindingPath;
use super::super::symbols::{CollectionType, SemanticIndex};
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
                return Ok(checked_binding_value_template(binding));
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

fn checked_binding_value_template(binding: &ValueTemplateBinding<'_>) -> CheckedValueTemplate {
    let root = match binding.source {
        ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
            ty: binding.root_checked_ty.clone(),
        },
        ValueTemplateSource::CurrentStatePayload => CheckedValueTemplate::CurrentStatePayload {
            ty: binding.root_checked_ty.clone(),
        },
    };
    match binding.path {
        PayloadBindingPath::Whole => root,
        PayloadBindingPath::RecordField { field } => CheckedValueTemplate::RecordField {
            ty: binding.checked_ty.clone(),
            record: Box::new(root),
            field: field.clone(),
        },
        PayloadBindingPath::ListIndex { index, len } => CheckedValueTemplate::ListElement {
            ty: binding.checked_ty.clone(),
            list: Box::new(root),
            index: *index,
            len: *len,
        },
        PayloadBindingPath::MapValue {
            key,
            keys,
            projection,
        } => CheckedValueTemplate::MapValue {
            ty: binding.checked_ty.clone(),
            map: Box::new(root),
            key: key.clone(),
            keys: keys.clone(),
            projection: *projection,
        },
    }
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
                                "map value type {expected_type} duplicates key {key_label}"
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
) -> Result<String> {
    checked_static_source_value_label(template).ok_or_else(|| {
        Error::new(format!(
            "map value type {expected_type} keys must be static source values in this source slice"
        ))
    })
}

fn checked_static_source_value_label(template: &CheckedValueTemplate) -> Option<String> {
    match template {
        CheckedValueTemplate::Literal(value) => Some(value.label().to_string()),
        CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::RecordField { .. }
        | CheckedValueTemplate::ListElement { .. }
        | CheckedValueTemplate::MapValue { .. }
        | CheckedValueTemplate::ProcessRef { .. } => None,
        CheckedValueTemplate::EnumVariant {
            variant, payload, ..
        } => Some(format!(
            "{variant}({})",
            checked_static_source_value_label(payload)?
        )),
        CheckedValueTemplate::Record { ty, fields } => {
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                parts.push(format!(
                    "{}:{}",
                    field.name(),
                    checked_static_source_value_label(field.value())?
                ));
            }
            Some(format!("{ty}{{{}}}", parts.join(",")))
        }
        CheckedValueTemplate::List { items, .. } => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(checked_static_source_value_label(item)?);
            }
            Some(format!("List[{}]", parts.join(",")))
        }
        CheckedValueTemplate::Map { entries, .. } => {
            let mut parts = BTreeMap::new();
            for entry in entries {
                let key = checked_static_source_value_label(entry.key())?;
                let value = checked_static_source_value_label(entry.value())?;
                if parts.insert(key, value).is_some() {
                    return None;
                }
            }
            Some(format!(
                "Map[{}]",
                parts
                    .into_iter()
                    .map(|(key, value)| format!("{key}=>{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
    }
}
