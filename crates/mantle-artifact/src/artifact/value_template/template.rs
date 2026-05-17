use std::collections::BTreeSet;

use super::super::{
    ArtifactStateValue, ArtifactType, ArtifactTypeField, ArtifactTypeKind, ArtifactValueShape,
    MantleArtifact,
};
use super::model::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, ArtifactValueTemplate,
    ArtifactValueTemplateField,
};
use super::payload::ArtifactPayload;
use super::projection::{
    ProjectionKeySetKind, validate_projection_key_set, validate_projection_keys,
};
use crate::validation::{validate_count, validate_ident_field, validate_value_label};
use crate::{Error, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, Result, TypeId};

impl ArtifactValueTemplate {
    pub fn result_type(&self) -> TypeId {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::EnumPayload { ty, .. }
            | Self::RecordField { ty, .. }
            | Self::ListElement { ty, .. }
            | Self::ListPrefixElement { ty, .. }
            | Self::ListRest { ty, .. }
            | Self::MapValue { ty, .. }
            | Self::MapRest { ty, .. }
            | Self::ProcessRef { ty, .. }
            | Self::LoopElement { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. }
            | Self::List { ty, .. }
            | Self::Map { ty, .. } => *ty,
        }
    }

    pub fn evaluate_state_value(
        &self,
        received_payload: Option<&ArtifactPayload>,
        current_state_payload: Option<&ArtifactPayload>,
        type_entry: &dyn Fn(TypeId) -> Result<ArtifactType>,
    ) -> Result<ArtifactStateValue> {
        match self {
            Self::Literal { ty, value } => ArtifactStateValue::from_value(*ty, value.clone()),
            Self::ReceivedPayload { ty } => {
                let payload = received_payload.ok_or_else(|| {
                    Error::new("received payload template requires a payload-bearing message")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                ArtifactStateValue::from_value(payload.ty, payload.value.clone())
            }
            Self::CurrentStatePayload { ty } => {
                let payload = current_state_payload.ok_or_else(|| {
                    Error::new("current state payload template requires a payload-bearing state")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "current state payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                ArtifactStateValue::from_value(payload.ty, payload.value.clone())
            }
            Self::EnumPayload { ty, value, variant } => {
                let value = value.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value_type = type_entry(value.ty)?;
                let variant = enum_variant_label_for_template(value.ty, &value_type, *variant)?;
                let payload = value.value.project_enum_payload(variant)?;
                validate_value_label("enum payload projection value", &payload.label())?;
                ArtifactStateValue::from_value(*ty, payload)
            }
            Self::RecordField { ty, record, field } => {
                let record = record.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value = record.value.project_record_field(field)?;
                validate_value_label("record field projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list.value.project_list_element(*index, *len)?;
                validate_value_label("list element projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list
                    .value
                    .project_list_prefix_element(*index, *prefix_len)?;
                validate_value_label("list prefix projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = list.value.project_list_rest(*prefix_len)?;
                validate_value_label("list rest projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => {
                let map =
                    map.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = map.value.project_map_value(key, keys, *projection)?;
                validate_value_label("map value projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                let map =
                    map.evaluate_state_value(received_payload, current_state_payload, type_entry)?;
                let value = map.value.project_map_rest(excluded_keys)?;
                validate_value_label("map rest projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::ProcessRef { .. } => Err(Error::new(
                "process reference template requires runtime process reference bindings",
            )),
            Self::LoopElement { .. } => Err(Error::new(
                "loop element template requires runtime loop element bindings",
            )),
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                let payload = payload.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_entry,
                )?;
                let value_type = type_entry(*ty)?;
                let variant = enum_variant_label_for_template(*ty, &value_type, *variant)?;
                let value = ArtifactValue::EnumVariant {
                    variant: variant.to_string(),
                    payload: Box::new(payload.value),
                };
                validate_value_label("enum variant template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Record { ty, fields } => {
                let ty_label = type_entry(*ty)?.label;
                let mut values = Vec::with_capacity(fields.len());
                let mut seen = BTreeSet::new();
                for field in fields {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    if !seen.insert(field.name.as_str()) {
                        return Err(Error::new(format!(
                            "record template duplicates field {}",
                            field.name
                        )));
                    }
                    values.push(ArtifactRecordField {
                        name: field.name.clone(),
                        value: value.value,
                    });
                }
                let value = ArtifactValue::Record {
                    constructor: ty_label,
                    fields: values,
                };
                validate_value_label("record template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::List { ty, items } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    let item_value = item.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    values.push(item_value.value);
                }
                let value = ArtifactValue::List(values);
                validate_value_label("list template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Map { ty, entries } => {
                let mut values = Vec::with_capacity(entries.len());
                let mut seen = BTreeSet::new();
                for entry in entries {
                    let key = entry.key.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    let value = entry.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_entry,
                    )?;
                    if !seen.insert(key.value.clone()) {
                        return Err(Error::new(format!(
                            "map template duplicates key {}",
                            key.value.label()
                        )));
                    }
                    values.push(ArtifactMapEntry {
                        key: key.value,
                        value: value.value,
                    });
                }
                let value = ArtifactValue::Map(values);
                validate_value_label("map template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
        }
    }

    pub(in crate::artifact) fn depends_on_received_payload(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => true,
            Self::CurrentStatePayload { .. } => false,
            Self::EnumPayload { value, .. } => value.depends_on_received_payload(),
            Self::RecordField { record, .. } => record.depends_on_received_payload(),
            Self::ListElement { list, .. }
            | Self::ListPrefixElement { list, .. }
            | Self::ListRest { list, .. } => list.depends_on_received_payload(),
            Self::MapValue { map, .. } => map.depends_on_received_payload(),
            Self::MapRest { map, .. } => map.depends_on_received_payload(),
            Self::ProcessRef { .. } => false,
            Self::LoopElement { .. } => false,
            Self::EnumVariant { payload, .. } => payload.depends_on_received_payload(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_received_payload()),
            Self::List { items, .. } => items.iter().any(Self::depends_on_received_payload),
            Self::Map { entries, .. } => entries.iter().any(|entry| {
                entry.key.depends_on_received_payload() || entry.value.depends_on_received_payload()
            }),
        }
    }

    pub(in crate::artifact) fn depends_on_loop_element(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => false,
            Self::CurrentStatePayload { .. } => false,
            Self::EnumPayload { value, .. } => value.depends_on_loop_element(),
            Self::RecordField { record, .. } => record.depends_on_loop_element(),
            Self::ListElement { list, .. }
            | Self::ListPrefixElement { list, .. }
            | Self::ListRest { list, .. } => list.depends_on_loop_element(),
            Self::MapValue { map, .. } => map.depends_on_loop_element(),
            Self::MapRest { map, .. } => map.depends_on_loop_element(),
            Self::ProcessRef { .. } => false,
            Self::LoopElement { .. } => true,
            Self::EnumVariant { payload, .. } => payload.depends_on_loop_element(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_loop_element()),
            Self::List { items, .. } => items.iter().any(Self::depends_on_loop_element),
            Self::Map { entries, .. } => entries.iter().any(|entry| {
                entry.key.depends_on_loop_element() || entry.value.depends_on_loop_element()
            }),
        }
    }

    pub(in crate::artifact) fn validate_for_received_payload(
        &self,
        artifact: &MantleArtifact,
        field: &str,
        expected_type: Option<TypeId>,
        received_payload_type: Option<TypeId>,
        current_state_payload_type: Option<TypeId>,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        artifact.type_entry(self.result_type())?;
        if let Some(expected_type) = expected_type {
            if self.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type id {}, expected {}",
                    self.result_type().as_u32(),
                    expected_type.as_u32()
                )));
            }
        }
        match self {
            Self::Literal { ty, value } => artifact.validate_value_matches_type(field, *ty, value),
            Self::ReceivedPayload { ty } => {
                let Some(received_payload_type) = received_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing transition message"
                    )));
                };
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "{field} has received payload type id {}, expected {}",
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                if expected_type.is_none()
                    && matches!(
                        artifact.type_entry(*ty)?.kind,
                        ArtifactTypeKind::ProcessRef { .. }
                    )
                {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                Ok(())
            }
            Self::CurrentStatePayload { ty } => {
                let Some(current_state_payload_type) = current_state_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing current state"
                    )));
                };
                if *ty != current_state_payload_type {
                    return Err(Error::new(format!(
                        "{field} has current state payload type id {}, expected {}",
                        ty.as_u32(),
                        current_state_payload_type.as_u32()
                    )));
                }
                Ok(())
            }
            Self::EnumPayload { ty, value, variant } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_enum_payload_projection(
                    artifact,
                    field,
                    value.result_type(),
                    *variant,
                    *ty,
                )?;
                value.validate_for_received_payload(
                    artifact,
                    &format!("{field}.value"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::RecordField {
                ty,
                record,
                field: field_name,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_ident_field(&format!("{field}.field_name"), field_name)?;
                validate_record_field_projection_type(
                    artifact,
                    field,
                    record.result_type(),
                    field_name,
                    *ty,
                )?;
                record.validate_for_received_payload(
                    artifact,
                    &format!("{field}.record"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_element_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(&format!("{field}.len"), *len, 1, MAX_VALUE_TEMPLATE_FIELDS)?;
                if *index >= *len {
                    return Err(Error::new(format!(
                        "{field}.index {index} is outside list length {len}"
                    )));
                }
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_element_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(
                    &format!("{field}.prefix_len"),
                    *prefix_len,
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if *index >= *prefix_len {
                    return Err(Error::new(format!(
                        "{field}.index {index} is outside list prefix length {prefix_len}"
                    )));
                }
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_list_rest_projection_type(artifact, field, list.result_type(), *ty)?;
                validate_count(
                    &format!("{field}.prefix_len"),
                    *prefix_len,
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                list.validate_for_received_payload(
                    artifact,
                    &format!("{field}.list"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection: _,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_map_value_projection_type(
                    artifact,
                    field,
                    map.result_type(),
                    key,
                    keys,
                    *ty,
                )?;
                validate_projection_keys(field, key, keys)?;
                map.validate_for_received_payload(
                    artifact,
                    &format!("{field}.map"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_map_rest_projection_type(
                    artifact,
                    field,
                    map.result_type(),
                    excluded_keys,
                    *ty,
                )?;
                validate_projection_key_set(field, excluded_keys, ProjectionKeySetKind::Excluded)?;
                map.validate_for_received_payload(
                    artifact,
                    &format!("{field}.map"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::ProcessRef {
                ty, target_process, ..
            } => {
                if let Some(expected_type) = expected_type
                    && *ty != expected_type
                {
                    return Err(Error::new(format!(
                        "{field} has type id {}, expected {}",
                        ty.as_u32(),
                        expected_type.as_u32()
                    )));
                }
                if expected_type.is_none() {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                artifact.validate_process_ref_type_id_target(
                    &format!("{field}.type_id"),
                    *ty,
                    *target_process,
                )
            }
            Self::LoopElement { ty, .. } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)
            }
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_enum_variant_payload(
                    artifact,
                    field,
                    *ty,
                    *variant,
                    payload.result_type(),
                )?;
                payload.validate_for_received_payload(
                    artifact,
                    &format!("{field}.payload"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::Record { ty, fields } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let expected_fields = validate_record_template_type(artifact, field, *ty, fields)?;
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                let mut seen = BTreeSet::new();
                for record_field in fields {
                    validate_ident_field(&format!("{field}.field"), &record_field.name)?;
                    if !seen.insert(record_field.name.as_str()) {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            record_field.name
                        )));
                    }
                    let expected = expected_fields
                        .iter()
                        .find(|expected| expected.name == record_field.name)
                        .expect("record template type validation should check field names");
                    record_field.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.field.{}", record_field.name),
                        Some(expected.ty),
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::List { ty, items } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let (element, capacity) = validate_list_template_type(artifact, field, *ty)?;
                validate_count(
                    &format!("{field}.item_count"),
                    items.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if items.len() > capacity {
                    return Err(Error::new(format!(
                        "{field}.item_count is {}, capacity is {}",
                        items.len(),
                        capacity
                    )));
                }
                for (index, item) in items.iter().enumerate() {
                    item.validate_for_received_payload(
                        artifact,
                        &format!("{field}.item.{index}"),
                        Some(element),
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::Map { ty, entries } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                let (key_type, value_type, capacity) =
                    validate_map_template_type(artifact, field, *ty)?;
                validate_count(
                    &format!("{field}.entry_count"),
                    entries.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                if entries.len() > capacity {
                    return Err(Error::new(format!(
                        "{field}.entry_count is {}, capacity is {}",
                        entries.len(),
                        capacity
                    )));
                }
                let mut keys = BTreeSet::new();
                for (index, entry) in entries.iter().enumerate() {
                    if !is_static_map_key_template(&entry.key) {
                        return Err(Error::new(format!(
                            "{field}.entry.{index}.key must be a static value template"
                        )));
                    }
                    entry.key.validate_for_received_payload(
                        artifact,
                        &format!("{field}.entry.{index}.key"),
                        Some(key_type),
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                    let key = static_map_key_template_value(artifact, &entry.key)?;
                    if keys.contains(&key) {
                        return Err(Error::new(format!(
                            "{field} duplicates key {}",
                            key.label()
                        )));
                    }
                    keys.insert(key);
                    entry.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.entry.{index}.value"),
                        Some(value_type),
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
        }
    }
}

fn reject_projected_process_ref_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<()> {
    if matches!(
        artifact.type_entry(ty)?.kind,
        ArtifactTypeKind::ProcessRef { .. }
    ) {
        return Err(Error::new(format!(
            "{field} process reference template must be a direct message payload"
        )));
    }
    Ok(())
}

fn validate_record_field_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    record_ty: TypeId,
    field_name: &str,
    projected_ty: TypeId,
) -> Result<()> {
    let record_type = artifact.type_entry(record_ty)?;
    let ArtifactValueShape::Record { fields } = record_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.record type id {} must be a record type",
            record_ty.as_u32()
        )));
    };
    let Some(expected) = fields.iter().find(|expected| expected.name == field_name) else {
        return Err(Error::new(format!(
            "{field}.field_name {field_name} is not declared by type id {}",
            record_ty.as_u32()
        )));
    };
    if expected.ty != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected record field type id {}",
            projected_ty.as_u32(),
            expected.ty.as_u32()
        )));
    }
    Ok(())
}

fn validate_list_element_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    list_ty: TypeId,
    projected_ty: TypeId,
) -> Result<()> {
    let list_type = artifact.type_entry(list_ty)?;
    let ArtifactValueShape::List { element, .. } = list_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.list type id {} must be a list type",
            list_ty.as_u32()
        )));
    };
    if *element != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected list element type id {}",
            projected_ty.as_u32(),
            element.as_u32()
        )));
    }
    Ok(())
}

fn validate_list_rest_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    list_ty: TypeId,
    projected_ty: TypeId,
) -> Result<()> {
    let source_type = artifact.type_entry(list_ty)?;
    let ArtifactValueShape::List { element, .. } = source_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.list type id {} must be a list type",
            list_ty.as_u32()
        )));
    };
    let projected_type = artifact.type_entry(projected_ty)?;
    let ArtifactValueShape::List {
        element: projected_element,
        ..
    } = projected_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a list type",
            projected_ty.as_u32()
        )));
    };
    if element != projected_element {
        return Err(Error::new(format!(
            "{field}.type_id has list element type id {}, expected {}",
            projected_element.as_u32(),
            element.as_u32()
        )));
    }
    Ok(())
}

fn validate_map_value_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    map_ty: TypeId,
    key: &ArtifactValue,
    keys: &[ArtifactValue],
    projected_ty: TypeId,
) -> Result<()> {
    let map_type = artifact.type_entry(map_ty)?;
    let ArtifactValueShape::Map {
        key: key_type,
        value,
        ..
    } = map_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.map type id {} must be a map type",
            map_ty.as_u32()
        )));
    };
    if *value != projected_ty {
        return Err(Error::new(format!(
            "{field}.type_id has type id {}, expected map value type id {}",
            projected_ty.as_u32(),
            value.as_u32()
        )));
    }
    artifact.validate_value_matches_type(&format!("{field}.key"), *key_type, key)?;
    for (index, expected_key) in keys.iter().enumerate() {
        artifact.validate_value_matches_type(
            &format!("{field}.expected_key.{index}"),
            *key_type,
            expected_key,
        )?;
    }
    Ok(())
}

fn validate_map_rest_projection_type(
    artifact: &MantleArtifact,
    field: &str,
    map_ty: TypeId,
    excluded_keys: &[ArtifactValue],
    projected_ty: TypeId,
) -> Result<()> {
    let map_type = artifact.type_entry(map_ty)?;
    let ArtifactValueShape::Map { key, value, .. } = map_type.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.map type id {} must be a map type",
            map_ty.as_u32()
        )));
    };
    let projected_type = artifact.type_entry(projected_ty)?;
    let ArtifactValueShape::Map {
        key: projected_key,
        value: projected_value,
        ..
    } = projected_type.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a map type",
            projected_ty.as_u32()
        )));
    };
    if key != projected_key {
        return Err(Error::new(format!(
            "{field}.type_id has map key type id {}, expected {}",
            projected_key.as_u32(),
            key.as_u32()
        )));
    }
    if value != projected_value {
        return Err(Error::new(format!(
            "{field}.type_id has map value type id {}, expected {}",
            projected_value.as_u32(),
            value.as_u32()
        )));
    }
    for (index, excluded_key) in excluded_keys.iter().enumerate() {
        artifact.validate_value_matches_type(
            &format!("{field}.excluded_key.{index}"),
            *key,
            excluded_key,
        )?;
    }
    Ok(())
}

fn validate_record_template_type<'a>(
    artifact: &'a MantleArtifact,
    field: &str,
    ty: TypeId,
    actual_fields: &[ArtifactValueTemplateField],
) -> Result<&'a [ArtifactTypeField]> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Record { fields } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a record type",
            ty.as_u32()
        )));
    };
    if actual_fields.len() != fields.len() {
        return Err(Error::new(format!(
            "{field}.field_count is {}, expected {}",
            actual_fields.len(),
            fields.len()
        )));
    }
    for actual in actual_fields {
        if fields.iter().all(|expected| expected.name != actual.name) {
            return Err(Error::new(format!(
                "{field}.field {} is not declared by type id {}",
                actual.name,
                ty.as_u32()
            )));
        }
    }
    Ok(fields)
}

fn validate_list_template_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<(TypeId, usize)> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::List { element, capacity } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a list type",
            ty.as_u32()
        )));
    };
    Ok((*element, *capacity))
}

fn validate_map_template_type(
    artifact: &MantleArtifact,
    field: &str,
    ty: TypeId,
) -> Result<(TypeId, TypeId, usize)> {
    let type_entry = artifact.type_entry(ty)?;
    let ArtifactValueShape::Map {
        key,
        value,
        capacity,
    } = type_entry.value_shape()?
    else {
        return Err(Error::new(format!(
            "{field}.type_id {} must be a map type",
            ty.as_u32()
        )));
    };
    Ok((*key, *value, *capacity))
}

fn validate_enum_payload_projection(
    artifact: &MantleArtifact,
    field: &str,
    enum_ty: TypeId,
    variant: crate::EnumVariantId,
    projected_ty: TypeId,
) -> Result<()> {
    artifact.validate_value_type(&format!("{field}.enum_type_id"), enum_ty)?;
    let payload_type = artifact
        .enum_variant_payload_type(enum_ty, variant)
        .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
    match payload_type {
        Some(expected) if expected == projected_ty => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "{field}.type_id has type id {}, expected enum payload type id {}",
            projected_ty.as_u32(),
            expected.as_u32()
        ))),
        None => Err(Error::new(format!(
            "{field}.variant_id {} does not carry a payload",
            variant.as_u32()
        ))),
    }
}

fn validate_enum_variant_payload(
    artifact: &MantleArtifact,
    field: &str,
    enum_ty: TypeId,
    variant: crate::EnumVariantId,
    payload_ty: TypeId,
) -> Result<()> {
    artifact.validate_value_type(&format!("{field}.enum_type_id"), enum_ty)?;
    let expected = artifact
        .enum_variant_payload_type(enum_ty, variant)
        .map_err(|err| Error::new(format!("{field}.variant_id {}", err)))?;
    match expected {
        Some(expected) if expected == payload_ty => Ok(()),
        Some(expected) => Err(Error::new(format!(
            "{field}.payload has type id {}, expected {}",
            payload_ty.as_u32(),
            expected.as_u32()
        ))),
        None => Err(Error::new(format!(
            "{field}.variant_id {} does not carry a payload",
            variant.as_u32()
        ))),
    }
}

fn enum_variant_label_for_template(
    ty: TypeId,
    type_entry: &ArtifactType,
    variant: crate::EnumVariantId,
) -> Result<&str> {
    let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "type id {} is not an enum type",
            ty.as_u32()
        )));
    };
    variants
        .get(variant.index())
        .map(|variant| variant.label.as_str())
        .ok_or_else(|| {
            Error::new(format!(
                "type id {} has no enum variant id {}",
                ty.as_u32(),
                variant.as_u32()
            ))
        })
}

fn static_map_key_template_value(
    artifact: &MantleArtifact,
    template: &ArtifactValueTemplate,
) -> Result<ArtifactValue> {
    template
        .evaluate_state_value(None, None, &|ty| artifact.type_entry(ty).cloned())
        .map(|value| value.value)
}

fn is_static_map_key_template(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } => true,
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::EnumPayload { .. }
        | ArtifactValueTemplate::RecordField { .. }
        | ArtifactValueTemplate::ListElement { .. }
        | ArtifactValueTemplate::ListPrefixElement { .. }
        | ArtifactValueTemplate::ListRest { .. }
        | ArtifactValueTemplate::MapValue { .. }
        | ArtifactValueTemplate::MapRest { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::LoopElement { .. } => false,
        ArtifactValueTemplate::EnumVariant { payload, .. } => is_static_map_key_template(payload),
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .all(|field| is_static_map_key_template(&field.value)),
        ArtifactValueTemplate::List { items, .. } => items.iter().all(is_static_map_key_template),
        ArtifactValueTemplate::Map { entries, .. } => entries.iter().all(|entry| {
            is_static_map_key_template(&entry.key) && is_static_map_key_template(&entry.value)
        }),
    }
}
