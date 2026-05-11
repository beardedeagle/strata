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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactValue {
    Atom(String),
    EnumVariant {
        variant: String,
        payload: Box<ArtifactValue>,
    },
    Record {
        constructor: String,
        fields: BTreeMap<String, ArtifactValue>,
    },
    List(Vec<ArtifactValue>),
    Map(BTreeMap<ArtifactValue, ArtifactValue>),
    ProcessRef {
        type_id: TypeId,
        pid: u64,
    },
}

impl ArtifactValue {
    pub fn parse(label: &str) -> Result<Self> {
        Self::parse_field("artifact value", label)
    }

    pub(crate) fn parse_field(field: &str, label: &str) -> Result<Self> {
        validate_value_label(field, label)?;
        let value = parse_value(label, 0)?;
        value.validate(field)?;
        Ok(value)
    }

    pub fn process_ref(type_id: TypeId, pid: u64) -> Self {
        Self::ProcessRef { type_id, pid }
    }

    pub fn validate(&self, field: &str) -> Result<()> {
        self.validate_shape(field, 0)?;
        validate_value_label(field, &self.label())
    }

    pub(crate) fn validate_without_process_ref(&self, field: &str) -> Result<()> {
        self.validate(field)?;
        if self.contains_process_ref() {
            return Err(Error::new(format!(
                "{field} must not contain a process reference value"
            )));
        }
        Ok(())
    }

    fn validate_shape(&self, field: &str, depth: usize) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        match self {
            Self::Atom(value) => validate_ident_field(field, value),
            Self::EnumVariant { variant, payload } => {
                validate_ident_field(&format!("{field}.variant"), variant)?;
                payload.validate_shape(&format!("{field}.payload"), depth + 1)
            }
            Self::Record {
                constructor,
                fields,
            } => {
                validate_ident_field(&format!("{field}.constructor"), constructor)?;
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (name, value) in fields {
                    validate_ident_field(&format!("{field}.field"), name)?;
                    value.validate_shape(&format!("{field}.field.{name}"), depth + 1)?;
                }
                Ok(())
            }
            Self::List(items) => {
                validate_count(
                    &format!("{field}.item_count"),
                    items.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, value) in items.iter().enumerate() {
                    value.validate_shape(&format!("{field}.item.{index}"), depth + 1)?;
                }
                Ok(())
            }
            Self::Map(entries) => {
                validate_count(
                    &format!("{field}.entry_count"),
                    entries.len(),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    key.validate_shape(&format!("{field}.entry.{index}.key"), depth + 1)?;
                    value.validate_shape(&format!("{field}.entry.{index}.value"), depth + 1)?;
                }
                Ok(())
            }
            Self::ProcessRef { pid, .. } => {
                if *pid == 0 {
                    return Err(Error::new(format!(
                        "{field} process reference pid must be greater than zero"
                    )));
                }
                Ok(())
            }
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Atom(value) => value.clone(),
            Self::EnumVariant { variant, payload } => {
                format!("{variant}({})", payload.label())
            }
            Self::Record {
                constructor,
                fields,
            } => format!(
                "{constructor}{{{}}}",
                fields
                    .iter()
                    .map(|(field, value)| format!("{field}:{}", value.label()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::List(items) => format!(
                "List[{}]",
                items
                    .iter()
                    .map(ArtifactValue::label)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Map(entries) => format!(
                "Map[{}]",
                entries
                    .iter()
                    .map(|(key, value)| format!("{}=>{}", key.label(), value.label()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::ProcessRef { type_id, pid } => format!("type{}#{pid}", type_id.as_u32()),
        }
    }

    pub fn contains_process_ref(&self) -> bool {
        match self {
            Self::Atom(_) => false,
            Self::EnumVariant { payload, .. } => payload.contains_process_ref(),
            Self::Record { fields, .. } => fields.values().any(ArtifactValue::contains_process_ref),
            Self::List(items) => items.iter().any(ArtifactValue::contains_process_ref),
            Self::Map(entries) => entries
                .iter()
                .any(|(key, value)| key.contains_process_ref() || value.contains_process_ref()),
            Self::ProcessRef { .. } => true,
        }
    }

    pub fn project_record_field(&self, field: &str) -> Result<Self> {
        let Self::Record { fields, .. } = self else {
            return Err(Error::new(format!(
                "record projection requires a record value, got {}",
                self.label()
            )));
        };
        fields.get(field).cloned().ok_or_else(|| {
            Error::new(format!(
                "record projection field {field} is not present in {}",
                self.label()
            ))
        })
    }

    pub fn project_list_element(&self, index: usize, len: usize) -> Result<Self> {
        let Self::List(items) = self else {
            return Err(Error::new(format!(
                "list projection requires a list value, got {}",
                self.label()
            )));
        };
        if items.len() != len {
            return Err(Error::new(format!(
                "list projection expected length {len}, found {} in {}",
                items.len(),
                self.label()
            )));
        }
        items.get(index).cloned().ok_or_else(|| {
            Error::new(format!(
                "list projection index {index} is outside length {len}"
            ))
        })
    }

    pub fn project_map_value(
        &self,
        key: &ArtifactValue,
        keys: &[ArtifactValue],
        projection: MapProjectionMode,
    ) -> Result<Self> {
        validate_projection_keys("map projection", key, keys)?;
        let Self::Map(entries) = self else {
            return Err(Error::new(format!(
                "map projection requires a map value, got {}",
                self.label()
            )));
        };
        let entry_keys = entries.keys().cloned().collect::<Vec<_>>();
        match projection {
            MapProjectionMode::Exact => {
                if entry_keys.len() != keys.len()
                    || !keys
                        .iter()
                        .all(|expected_key| entries.contains_key(expected_key))
                {
                    return Err(Error::new(format!(
                        "map projection expected exact keys [{}], found [{}]",
                        labels(keys),
                        labels(&entry_keys)
                    )));
                }
            }
            MapProjectionMode::Subset => {
                for expected_key in keys {
                    if !entries.contains_key(expected_key) {
                        return Err(Error::new(format!(
                            "map projection expected key {}, found [{}]",
                            expected_key.label(),
                            labels(&entry_keys)
                        )));
                    }
                }
            }
        }
        entries.get(key).cloned().ok_or_else(|| {
            Error::new(format!(
                "map projection key {} is not present in {}",
                key.label(),
                self.label()
            ))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueTemplate {
    Literal {
        ty: TypeId,
        value: ArtifactValue,
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
        key: ArtifactValue,
        keys: Vec<ArtifactValue>,
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
            Self::RecordField { ty, record, field } => {
                let record = record.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_label,
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
                    list.evaluate_state_value(received_payload, current_state_payload, type_label)?;
                let value = list.value.project_list_element(*index, *len)?;
                validate_value_label("list element projection value", &value.label())?;
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
                    map.evaluate_state_value(received_payload, current_state_payload, type_label)?;
                let value = map.value.project_map_value(key, keys, *projection)?;
                validate_value_label("map value projection value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
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
                let value = ArtifactValue::EnumVariant {
                    variant: variant.clone(),
                    payload: Box::new(payload.value),
                };
                validate_value_label("enum variant template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Record { ty, fields } => {
                let ty_label = type_label(*ty)?;
                let mut values = BTreeMap::new();
                for field in fields {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    if values.insert(field.name.clone(), value.value).is_some() {
                        return Err(Error::new(format!(
                            "record template duplicates field {}",
                            field.name
                        )));
                    }
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
                        type_label,
                    )?;
                    values.push(item_value.value);
                }
                let value = ArtifactValue::List(values);
                validate_value_label("list template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
            }
            Self::Map { ty, entries } => {
                let mut values = BTreeMap::new();
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
                            key.value.label()
                        )));
                    }
                }
                let value = ArtifactValue::Map(values);
                validate_value_label("map template value", &value.label())?;
                ArtifactStateValue::from_value(*ty, value)
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
                value.validate_without_process_ref(field)
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
                        return Err(Error::new(format!(
                            "{field} duplicates key {}",
                            key.label()
                        )));
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
) -> Result<ArtifactValue> {
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

fn validate_projection_keys(
    field: &str,
    key: &ArtifactValue,
    keys: &[ArtifactValue],
) -> Result<()> {
    validate_count(
        &format!("{field}.key_count"),
        keys.len(),
        1,
        MAX_VALUE_TEMPLATE_FIELDS,
    )?;
    key.validate_without_process_ref(&format!("{field}.key"))?;
    let mut seen = BTreeSet::new();
    for expected_key in keys {
        expected_key.validate_without_process_ref(&format!("{field}.expected_key"))?;
        if !seen.insert(expected_key.clone()) {
            return Err(Error::new(format!(
                "{field} duplicates expected map key {}",
                expected_key.label()
            )));
        }
    }
    if !seen.contains(key) {
        return Err(Error::new(format!(
            "{field} projection key {} is not one of the expected map keys",
            key.label()
        )));
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(format!(
            "{field} expected map keys must be sorted"
        )));
    }
    Ok(())
}

fn parse_value(label: &str, depth: usize) -> Result<ArtifactValue> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "artifact value exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
        )));
    }
    if let Some(body) = label.strip_prefix("List[") {
        let Some(body) = body.strip_suffix(']') else {
            return Err(Error::new(format!("{label} is not a list value")));
        };
        return parse_list(body, depth + 1);
    }
    if let Some(body) = label.strip_prefix("Map[") {
        let Some(body) = body.strip_suffix(']') else {
            return Err(Error::new(format!("{label} is not a map value")));
        };
        return parse_map(label, body, depth + 1);
    }
    if let Some(open) = top_level_char(label, '{') {
        let Some(body) = label.strip_suffix('}') else {
            return Err(Error::new(format!("{label} is not a record value")));
        };
        let constructor = &label[..open];
        validate_ident_field("artifact record value type", constructor)?;
        return parse_record(constructor, &body[open + 1..], depth + 1);
    }
    if let Some(open) = top_level_char(label, '(') {
        let Some(body) = label.strip_suffix(')') else {
            return Err(Error::new(format!("{label} is not an enum payload value")));
        };
        let variant = &label[..open];
        validate_ident_field("artifact enum variant value", variant)?;
        return Ok(ArtifactValue::EnumVariant {
            variant: variant.to_string(),
            payload: Box::new(parse_value(&body[open + 1..], depth + 1)?),
        });
    }
    validate_ident_field("artifact atom value", label)?;
    Ok(ArtifactValue::Atom(label.to_string()))
}

fn parse_record(constructor: &str, body: &str, depth: usize) -> Result<ArtifactValue> {
    let mut fields = BTreeMap::new();
    if body.is_empty() {
        return Err(Error::new(format!(
            "fieldless record values use {constructor}; braced record values must declare at least one field"
        )));
    }
    let parts = split_top_level(body, ',')?;
    if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "record value {constructor}{{{body}}} field count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    for part in parts {
        let index = top_level_char(part, ':').ok_or_else(|| {
            Error::new(format!(
                "record value {constructor}{{{body}}} contains malformed field"
            ))
        })?;
        let name = &part[..index];
        validate_ident_field("artifact record field", name)?;
        if fields
            .insert(name.to_string(), parse_value(&part[index + 1..], depth)?)
            .is_some()
        {
            return Err(Error::new(format!(
                "record value {constructor}{{{body}}} duplicates field {name}"
            )));
        }
    }
    Ok(ArtifactValue::Record {
        constructor: constructor.to_string(),
        fields,
    })
}

fn parse_list(body: &str, depth: usize) -> Result<ArtifactValue> {
    let items = if body.is_empty() {
        Vec::new()
    } else {
        let parts = split_top_level(body, ',')?;
        if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "list value item count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        parts
            .into_iter()
            .map(|part| parse_value(part, depth))
            .collect::<Result<Vec<_>>>()?
    };
    Ok(ArtifactValue::List(items))
}

fn parse_map(label: &str, body: &str, depth: usize) -> Result<ArtifactValue> {
    let mut entries = BTreeMap::new();
    if !body.is_empty() {
        let parts = split_top_level(body, ',')?;
        if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "map value {label} entry count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        for part in parts {
            let index = top_level_fat_arrow(part)
                .ok_or_else(|| Error::new(format!("map value {label} contains malformed entry")))?;
            let key = parse_value(&part[..index], depth)?;
            if entries
                .insert(key.clone(), parse_value(&part[index + 2..], depth)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "map value {label} duplicates key {}",
                    key.label()
                )));
            }
        }
    }
    Ok(ArtifactValue::Map(entries))
}

fn labels(values: &[ArtifactValue]) -> String {
    values
        .iter()
        .map(ArtifactValue::label)
        .collect::<Vec<_>>()
        .join(",")
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

fn top_level_char(value: &str, target: char) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for (index, ch) in value.char_indices() {
        if ch == target && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            return Some(index);
        }
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn top_level_fat_arrow(value: &str) -> Option<usize> {
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
    pub value: ArtifactValue,
    pub process_ref: Option<ArtifactProcessRefPayload>,
}

impl ArtifactPayload {
    pub fn value(ty: TypeId, value: ArtifactValue) -> Result<Self> {
        value.validate_without_process_ref("payload value")?;
        Ok(Self {
            ty,
            value,
            process_ref: None,
        })
    }

    pub fn process_ref(ty: TypeId, target_process: ProcessId, pid: u64) -> Result<Self> {
        let value = ArtifactValue::process_ref(ty, pid);
        value.validate("process reference payload value")?;
        Ok(Self {
            ty,
            value,
            process_ref: Some(ArtifactProcessRefPayload {
                target_process,
                pid,
            }),
        })
    }

    pub fn label(&self) -> String {
        self.value.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProcessRefPayload {
    pub target_process: ProcessId,
    pub pid: u64,
}
