use crate::language::ast::{ListValue, MapValue};
use crate::language::checked::CheckedValueTemplateMapEntry;
use crate::language::checker::symbols::CollectionType;
use mantle_artifact::{ArtifactMapEntry, ArtifactRecordField};

use super::*;

pub(super) fn checked_collection_template(
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
            let mut key_values = Vec::with_capacity(map.entries.len());
            let mut entries = Vec::with_capacity(map.entries.len());
            for entry in &map.entries {
                let key_template = checked_value_template(
                    module,
                    semantic_index,
                    types,
                    key,
                    &entry.key,
                    bindings,
                    depth + 1,
                )?;
                let key_value = validate_static_map_key_template(expected_type, &key_template)?;
                if key_values.iter().any(|previous| previous == &key_value) {
                    return Err(Error::new(format!(
                        "map value type {expected_type} duplicates key {}",
                        key_value.label()
                    )));
                }
                key_values.push(key_value);
                entries.push(CheckedValueTemplateMapEntry::new(
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
                ));
            }
            Ok(CheckedValueTemplate::Map {
                ty: types.intern(expected_type)?,
                entries,
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
            "map value type {expected_type} keys must be static source values"
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
        | CheckedValueTemplate::EffectOutcome { .. }
        | CheckedValueTemplate::Equality { .. }
        | CheckedValueTemplate::ScalarArithmetic { .. }
        | CheckedValueTemplate::ScalarOrdering { .. }
        | CheckedValueTemplate::IfElse { .. }
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
            for (index, field) in fields.iter().enumerate() {
                if fields[..index]
                    .iter()
                    .any(|previous| previous.name() == field.name())
                {
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
            let mut values: Vec<ArtifactMapEntry> = Vec::with_capacity(entries.len());
            for entry in entries {
                let key = checked_static_source_value(entry.key())?;
                let value = checked_static_source_value(entry.value())?;
                if values.iter().any(|previous| previous.key == key) {
                    return None;
                }
                values.push(ArtifactMapEntry { key, value });
            }
            Some(ArtifactValue::Map(values))
        }
    }
}
