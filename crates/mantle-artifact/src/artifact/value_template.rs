use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapProjectionMode {
    Exact,
    Subset,
}

impl MapProjectionMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Subset => "subset",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "exact" => Ok(Self::Exact),
            "subset" => Ok(Self::Subset),
            _ => Err(Error::new(format!("invalid map projection mode {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueTemplate {
    Literal {
        ty: TypeId,
        value: String,
    },
    ReceivedPayload {
        ty: TypeId,
    },
    CurrentStatePayload {
        ty: TypeId,
    },
    RecordField {
        ty: TypeId,
        record: Box<ArtifactValueTemplate>,
        field: String,
    },
    ListElement {
        ty: TypeId,
        list: Box<ArtifactValueTemplate>,
        index: usize,
        len: usize,
    },
    MapValue {
        ty: TypeId,
        map: Box<ArtifactValueTemplate>,
        key: String,
        keys: Vec<String>,
        projection: MapProjectionMode,
    },
    ProcessRef {
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    EnumVariant {
        ty: TypeId,
        variant: String,
        payload: Box<ArtifactValueTemplate>,
    },
    Record {
        ty: TypeId,
        fields: Vec<ArtifactValueTemplateField>,
    },
    List {
        ty: TypeId,
        items: Vec<ArtifactValueTemplate>,
    },
    Map {
        ty: TypeId,
        entries: Vec<ArtifactValueTemplateMapEntry>,
    },
}

impl ArtifactValueTemplate {
    pub fn result_type(&self) -> TypeId {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::RecordField { ty, .. }
            | Self::ListElement { ty, .. }
            | Self::MapValue { ty, .. }
            | Self::ProcessRef { ty, .. }
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
        type_label: &dyn Fn(TypeId) -> Result<String>,
    ) -> Result<ArtifactStateValue> {
        match self {
            Self::Literal { ty, value } => Ok(ArtifactStateValue::new(*ty, value.clone())),
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
                if payload.process_ref.is_some() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                Ok(ArtifactStateValue::new(payload.ty, payload.value.clone()))
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
                if payload.process_ref.is_some() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                Ok(ArtifactStateValue::new(payload.ty, payload.value.clone()))
            }
            Self::RecordField { ty, record, field } => {
                let record = record.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_label,
                )?;
                let value = project_canonical_record_field(&record.value, field)?;
                let label = project_canonical_record_field(&record.label, field)?;
                validate_value_label("record field projection value", &value)?;
                validate_value_label("record field projection label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::ListElement {
                ty,
                list,
                index,
                len,
            } => {
                let list =
                    list.evaluate_state_value(received_payload, current_state_payload, type_label)?;
                let value = project_canonical_list_element(&list.value, *index, *len)?;
                let label = project_canonical_list_element(&list.label, *index, *len)?;
                validate_value_label("list element projection value", &value)?;
                validate_value_label("list element projection label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => {
                let map =
                    map.evaluate_state_value(received_payload, current_state_payload, type_label)?;
                let value = project_canonical_map_value(&map.value, key, keys, *projection)?;
                let label = project_canonical_map_value(&map.label, key, keys, *projection)?;
                validate_value_label("map value projection value", &value)?;
                validate_value_label("map value projection label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::ProcessRef { .. } => Err(Error::new(
                "process reference template requires runtime process reference bindings",
            )),
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                let payload = payload.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_label,
                )?;
                let value = format!("{variant}({})", payload.value);
                let label = format!("{variant}({})", payload.label);
                validate_value_label("enum variant template value", &value)?;
                validate_value_label("enum variant template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::Record { ty, fields } => {
                let ty_label = type_label(*ty)?;
                let mut parts = Vec::with_capacity(fields.len());
                let mut labels = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    parts.push(format!("{}:{}", field.name, value.value));
                    labels.push(format!("{}:{}", field.name, value.label));
                }
                let value = format!("{ty_label}{{{}}}", parts.join(","));
                let label = format!("{ty_label}{{{}}}", labels.join(","));
                validate_value_label("record template value", &value)?;
                validate_value_label("record template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::List { ty, items } => {
                let mut values = Vec::with_capacity(items.len());
                let mut labels = Vec::with_capacity(items.len());
                for item in items {
                    let item_value = item.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    values.push(item_value.value);
                    labels.push(item_value.label);
                }
                let value = format!("List[{}]", values.join(","));
                let label = format!("List[{}]", labels.join(","));
                validate_value_label("list template value", &value)?;
                validate_value_label("list template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::Map { ty, entries } => {
                let mut values = BTreeMap::new();
                let mut labels = BTreeMap::new();
                for entry in entries {
                    let key = entry.key.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    let value = entry.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    if values.insert(key.value.clone(), value.value).is_some() {
                        return Err(Error::new(format!(
                            "map template duplicates key {}",
                            key.value
                        )));
                    }
                    if labels.insert(key.label.clone(), value.label).is_some() {
                        return Err(Error::new(format!(
                            "map template duplicates key {}",
                            key.label
                        )));
                    }
                }
                let value = format!(
                    "Map[{}]",
                    values
                        .into_iter()
                        .map(|(key, value)| format!("{key}=>{value}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let label = format!(
                    "Map[{}]",
                    labels
                        .into_iter()
                        .map(|(key, value)| format!("{key}=>{value}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                validate_value_label("map template value", &value)?;
                validate_value_label("map template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
        }
    }

    pub(super) fn depends_on_received_payload(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => true,
            Self::CurrentStatePayload { .. } => false,
            Self::RecordField { record, .. } => record.depends_on_received_payload(),
            Self::ListElement { list, .. } => list.depends_on_received_payload(),
            Self::MapValue { map, .. } => map.depends_on_received_payload(),
            Self::ProcessRef { .. } => false,
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

    pub(super) fn validate_for_received_payload(
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
            Self::Literal { ty, value } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_value_label(field, value)
            }
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
            Self::RecordField {
                ty,
                record,
                field: field_name,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_ident_field(&format!("{field}.field_name"), field_name)?;
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
            Self::MapValue {
                ty,
                map,
                key,
                keys,
                projection: _,
            } => {
                reject_projected_process_ref_type(artifact, field, *ty)?;
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
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
            Self::ProcessRef {
                ty, target_process, ..
            } => {
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
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_ident_field(&format!("{field}.variant"), variant)?;
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
                    record_field.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.field.{}", record_field.name),
                        None,
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::List { ty, items } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_count(
                    &format!("{field}.item_count"),
                    items.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, item) in items.iter().enumerate() {
                    item.validate_for_received_payload(
                        artifact,
                        &format!("{field}.item.{index}"),
                        None,
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            Self::Map { ty, entries } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_count(
                    &format!("{field}.entry_count"),
                    entries.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
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
                        None,
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                    let key = static_map_key_template_value(artifact, &entry.key)?;
                    if !keys.insert(key.clone()) {
                        return Err(Error::new(format!("{field} duplicates key {key}")));
                    }
                    entry.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.entry.{index}.value"),
                        None,
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

fn static_map_key_template_value(
    artifact: &MantleArtifact,
    template: &ArtifactValueTemplate,
) -> Result<String> {
    template
        .evaluate_state_value(None, None, &|ty| artifact.type_label(ty).map(str::to_owned))
        .map(|value| value.value)
}

fn is_static_map_key_template(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::Literal { .. } => true,
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::RecordField { .. }
        | ArtifactValueTemplate::ListElement { .. }
        | ArtifactValueTemplate::MapValue { .. }
        | ArtifactValueTemplate::ProcessRef { .. } => false,
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

pub fn project_canonical_record_field(value: &str, field: &str) -> Result<String> {
    let fields = record_label_fields(value)?;
    fields.get(field).cloned().ok_or_else(|| {
        Error::new(format!(
            "record projection field {field} is not present in {value}"
        ))
    })
}

pub fn project_canonical_list_element(value: &str, index: usize, len: usize) -> Result<String> {
    let items = list_label_items(value)?;
    if items.len() != len {
        return Err(Error::new(format!(
            "list projection expected length {len}, found {} in {value}",
            items.len()
        )));
    }
    items.get(index).cloned().ok_or_else(|| {
        Error::new(format!(
            "list projection index {index} is outside length {len}"
        ))
    })
}

pub fn project_canonical_map_value(
    value: &str,
    key: &str,
    keys: &[String],
    projection: MapProjectionMode,
) -> Result<String> {
    let entries = map_label_entries(value)?;
    let entry_keys = entries.keys().cloned().collect::<Vec<_>>();
    match projection {
        MapProjectionMode::Exact => {
            if entry_keys != keys {
                return Err(Error::new(format!(
                    "map projection expected exact keys [{}], found [{}]",
                    keys.join(","),
                    entry_keys.join(",")
                )));
            }
        }
        MapProjectionMode::Subset => {
            for expected_key in keys {
                if !entries.contains_key(expected_key) {
                    return Err(Error::new(format!(
                        "map projection expected key {expected_key}, found [{}]",
                        entry_keys.join(",")
                    )));
                }
            }
        }
    }
    entries.get(key).cloned().ok_or_else(|| {
        Error::new(format!(
            "map projection key {key} is not present in {value}"
        ))
    })
}

fn validate_projection_keys(field: &str, key: &str, keys: &[String]) -> Result<()> {
    validate_count(
        &format!("{field}.key_count"),
        keys.len(),
        1,
        MAX_VALUE_TEMPLATE_FIELDS,
    )?;
    validate_value_label(&format!("{field}.key"), key)?;
    let mut seen = BTreeSet::new();
    for expected_key in keys {
        validate_value_label(&format!("{field}.expected_key"), expected_key)?;
        if !seen.insert(expected_key.clone()) {
            return Err(Error::new(format!(
                "{field} duplicates expected map key {expected_key}"
            )));
        }
    }
    if !seen.contains(key) {
        return Err(Error::new(format!(
            "{field} projection key {key} is not one of the expected map keys"
        )));
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(format!(
            "{field} expected map keys must be sorted"
        )));
    }
    Ok(())
}

fn record_label_fields(value: &str) -> Result<BTreeMap<String, String>> {
    let Some(open) = value.find('{') else {
        return Err(Error::new(format!("{value} is not a record value")));
    };
    if !value.ends_with('}') {
        return Err(Error::new(format!("{value} is not a record value")));
    }
    let body = &value[open + 1..value.len() - 1];
    let mut fields = BTreeMap::new();
    if body.is_empty() {
        return Ok(fields);
    }
    for part in split_top_level(body, ',')? {
        let index = find_top_level_char(part, ':')
            .ok_or_else(|| Error::new(format!("record value {value} contains malformed field")))?;
        let field = part[..index].to_string();
        if fields
            .insert(field.clone(), part[index + 1..].to_string())
            .is_some()
        {
            return Err(Error::new(format!(
                "record value {value} duplicates field {field}"
            )));
        }
    }
    Ok(fields)
}

fn list_label_items(value: &str) -> Result<Vec<String>> {
    let Some(body) = value.strip_prefix("List[") else {
        return Err(Error::new(format!("{value} is not a list value")));
    };
    let Some(body) = body.strip_suffix(']') else {
        return Err(Error::new(format!("{value} is not a list value")));
    };
    if body.is_empty() {
        return Ok(Vec::new());
    }
    split_top_level(body, ',').map(|items| items.into_iter().map(str::to_string).collect())
}

fn map_label_entries(value: &str) -> Result<BTreeMap<String, String>> {
    let Some(body) = value.strip_prefix("Map[") else {
        return Err(Error::new(format!("{value} is not a map value")));
    };
    let Some(body) = body.strip_suffix(']') else {
        return Err(Error::new(format!("{value} is not a map value")));
    };
    let mut entries = BTreeMap::new();
    if body.is_empty() {
        return Ok(entries);
    }
    for part in split_top_level(body, ',')? {
        let index = find_top_level_fat_arrow(part)
            .ok_or_else(|| Error::new(format!("map value {value} contains malformed entry")))?;
        let key = part[..index].to_string();
        if entries
            .insert(key.clone(), part[index + 2..].to_string())
            .is_some()
        {
            return Err(Error::new(format!(
                "map value {value} duplicates key {key}"
            )));
        }
    }
    Ok(entries)
}

fn split_top_level(value: &str, separator: char) -> Result<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced parentheses"))
                })?
            }
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced brackets"))
                })?
            }
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("value label {value} has unbalanced braces"))
                })?
            }
            _ => {}
        }
        if ch == separator && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            parts.push(&value[start..index]);
            start = index + ch.len_utf8();
        }
    }
    if paren_depth != 0 || bracket_depth != 0 || brace_depth != 0 {
        return Err(Error::new(format!("value label {value} is unbalanced")));
    }
    parts.push(&value[start..]);
    Ok(parts)
}

fn find_top_level_char(value: &str, target: char) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
        if ch == target && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            return Some(index);
        }
    }
    None
}

fn find_top_level_fat_arrow(value: &str) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '=' if paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && value[index..].starts_with("=>") =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateField {
    pub name: String,
    pub value: ArtifactValueTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateMapEntry {
    pub key: ArtifactValueTemplate,
    pub value: ArtifactValueTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub ty: TypeId,
    pub value: String,
    pub process_ref: Option<ArtifactProcessRefPayload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProcessRefPayload {
    pub target_process: ProcessId,
    pub pid: u64,
}
