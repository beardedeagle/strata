use std::collections::BTreeMap;

use mantle_artifact::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactStateValue, ArtifactValueTemplate,
    ArtifactValueTemplateMapEntry, Error, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS,
    MapProjectionMode, ProcessId, ProcessRefId, Result, TypeId, validate_payload_value_label,
};

use super::validate_loaded_ident_field;
use crate::event::RuntimeProcessId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuntimeValue {
    Atom(String),
    EnumVariant {
        variant: String,
        payload: Box<RuntimeValue>,
    },
    Record {
        constructor: String,
        fields: BTreeMap<String, RuntimeValue>,
    },
    List(Vec<RuntimeValue>),
    Map(BTreeMap<RuntimeValue, RuntimeValue>),
    ProcessRef {
        ty: TypeId,
        pid: RuntimeProcessId,
    },
}

impl RuntimeValue {
    pub(crate) fn parse(label: &str) -> Result<Self> {
        validate_payload_value_label(label)?;
        parse_value(label, 0)
    }

    pub(crate) fn label(&self) -> String {
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
                items.iter().map(Self::label).collect::<Vec<_>>().join(",")
            ),
            Self::Map(entries) => format!(
                "Map[{}]",
                entries
                    .iter()
                    .map(|(key, value)| format!("{}=>{}", key.label(), value.label()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::ProcessRef { ty, pid } => format!("type{}#{}", ty.as_u32(), pid.as_u64()),
        }
    }

    pub(crate) fn project_record_field(&self, field: &str) -> Result<Self> {
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

    pub(crate) fn project_list_element(&self, index: usize, len: usize) -> Result<Self> {
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

    pub(crate) fn project_map_value(
        &self,
        key: &RuntimeValue,
        keys: &[RuntimeValue],
        projection: MapProjectionMode,
    ) -> Result<Self> {
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
pub struct RuntimePayload {
    pub(crate) ty: TypeId,
    pub(crate) value: RuntimeValue,
    pub(crate) process_ref: Option<ArtifactProcessRefPayload>,
}

impl RuntimePayload {
    pub(crate) fn from_artifact(payload: &ArtifactPayload) -> Result<Self> {
        if let Some(process_ref) = payload.process_ref {
            let pid = RuntimeProcessId::from_u64(process_ref.pid)?;
            let value = RuntimeValue::ProcessRef {
                ty: payload.ty,
                pid,
            };
            let expected_label = value.label();
            if payload.value != expected_label {
                return Err(Error::new(format!(
                    "process reference payload value {} does not match metadata label {}",
                    payload.value, expected_label
                )));
            }
            return Ok(Self {
                ty: payload.ty,
                value,
                process_ref: Some(process_ref),
            });
        }
        Ok(Self {
            ty: payload.ty,
            value: RuntimeValue::parse(&payload.value)?,
            process_ref: payload.process_ref,
        })
    }

    pub(crate) fn value(ty: TypeId, value: RuntimeValue) -> Result<Self> {
        validate_payload_value_label(&value.label())?;
        Ok(Self {
            ty,
            value,
            process_ref: None,
        })
    }

    pub(crate) fn from_process_ref(
        ty: TypeId,
        target_process: ProcessId,
        pid: RuntimeProcessId,
    ) -> Result<Self> {
        let value = RuntimeValue::ProcessRef { ty, pid };
        validate_payload_value_label(&value.label())?;
        Ok(Self {
            ty,
            value,
            process_ref: Some(ArtifactProcessRefPayload {
                target_process,
                pid: pid.as_u64(),
            }),
        })
    }

    pub fn type_id(&self) -> TypeId {
        self.ty
    }

    pub fn label(&self) -> String {
        self.value.label()
    }

    pub fn process_ref(&self) -> Option<ArtifactProcessRefPayload> {
        self.process_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedStateValue {
    pub(crate) ty: TypeId,
    pub(crate) value: RuntimeValue,
    pub(crate) label: String,
    pub(crate) payload: Option<RuntimePayload>,
}

impl LoadedStateValue {
    pub(crate) fn from_artifact(state: &ArtifactStateValue) -> Result<Self> {
        Ok(Self {
            ty: state.ty,
            value: RuntimeValue::parse(&state.value)?,
            label: state.label.clone(),
            payload: state
                .payload
                .as_ref()
                .map(RuntimePayload::from_artifact)
                .transpose()?,
        })
    }

    pub(crate) fn from_payload(payload: RuntimePayload) -> Self {
        let label = payload.label();
        Self {
            ty: payload.ty,
            value: payload.value,
            label,
            payload: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedValueTemplate {
    Literal {
        ty: TypeId,
        value: RuntimeValue,
    },
    ReceivedPayload {
        ty: TypeId,
    },
    CurrentStatePayload {
        ty: TypeId,
    },
    RecordField {
        ty: TypeId,
        record: Box<LoadedValueTemplate>,
        field: String,
    },
    ListElement {
        ty: TypeId,
        list: Box<LoadedValueTemplate>,
        index: usize,
        len: usize,
    },
    MapValue {
        ty: TypeId,
        map: Box<LoadedValueTemplate>,
        key: RuntimeValue,
        keys: Vec<RuntimeValue>,
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
        payload: Box<LoadedValueTemplate>,
    },
    Record {
        ty: TypeId,
        fields: Vec<LoadedValueTemplateField>,
    },
    List {
        ty: TypeId,
        items: Vec<LoadedValueTemplate>,
    },
    Map {
        ty: TypeId,
        entries: Vec<LoadedValueTemplateMapEntry>,
    },
}

impl LoadedValueTemplate {
    pub(crate) fn from_artifact(template: &ArtifactValueTemplate) -> Result<Self> {
        match template {
            ArtifactValueTemplate::Literal { ty, value } => Ok(Self::Literal {
                ty: *ty,
                value: RuntimeValue::parse(value)?,
            }),
            ArtifactValueTemplate::ReceivedPayload { ty } => Ok(Self::ReceivedPayload { ty: *ty }),
            ArtifactValueTemplate::CurrentStatePayload { ty } => {
                Ok(Self::CurrentStatePayload { ty: *ty })
            }
            ArtifactValueTemplate::RecordField { ty, record, field } => Ok(Self::RecordField {
                ty: *ty,
                record: Box::new(Self::from_artifact(record)?),
                field: field.clone(),
            }),
            ArtifactValueTemplate::ListElement {
                ty,
                list,
                index,
                len,
            } => Ok(Self::ListElement {
                ty: *ty,
                list: Box::new(Self::from_artifact(list)?),
                index: *index,
                len: *len,
            }),
            ArtifactValueTemplate::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => Ok(Self::MapValue {
                ty: *ty,
                map: Box::new(Self::from_artifact(map)?),
                key: RuntimeValue::parse(key)?,
                keys: keys
                    .iter()
                    .map(|expected_key| RuntimeValue::parse(expected_key))
                    .collect::<Result<Vec<_>>>()?,
                projection: *projection,
            }),
            ArtifactValueTemplate::ProcessRef {
                ty,
                target_process,
                process_ref,
            } => Ok(Self::ProcessRef {
                ty: *ty,
                target_process: *target_process,
                process_ref: *process_ref,
            }),
            ArtifactValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => Ok(Self::EnumVariant {
                ty: *ty,
                variant: variant.clone(),
                payload: Box::new(Self::from_artifact(payload)?),
            }),
            ArtifactValueTemplate::Record { ty, fields } => Ok(Self::Record {
                ty: *ty,
                fields: fields
                    .iter()
                    .map(LoadedValueTemplateField::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
            }),
            ArtifactValueTemplate::List { ty, items } => Ok(Self::List {
                ty: *ty,
                items: items
                    .iter()
                    .map(Self::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
            }),
            ArtifactValueTemplate::Map { ty, entries } => Ok(Self::Map {
                ty: *ty,
                entries: entries
                    .iter()
                    .map(LoadedValueTemplateMapEntry::from_artifact)
                    .collect::<Result<Vec<_>>>()?,
            }),
        }
    }

    pub(crate) const fn result_type(&self) -> TypeId {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedValueTemplateField {
    pub(crate) name: String,
    pub(crate) value: LoadedValueTemplate,
}

impl LoadedValueTemplateField {
    fn from_artifact(field: &mantle_artifact::ArtifactValueTemplateField) -> Result<Self> {
        Ok(Self {
            name: field.name.clone(),
            value: LoadedValueTemplate::from_artifact(&field.value)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedValueTemplateMapEntry {
    pub(crate) key: LoadedValueTemplate,
    pub(crate) value: LoadedValueTemplate,
}

impl LoadedValueTemplateMapEntry {
    fn from_artifact(entry: &ArtifactValueTemplateMapEntry) -> Result<Self> {
        Ok(Self {
            key: LoadedValueTemplate::from_artifact(&entry.key)?,
            value: LoadedValueTemplate::from_artifact(&entry.value)?,
        })
    }
}

fn parse_value(label: &str, depth: usize) -> Result<RuntimeValue> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "runtime value exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
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
        let ty = &label[..open];
        validate_loaded_ident_field("runtime record value type", ty)?;
        return parse_record(ty, &body[open + 1..], depth + 1);
    }
    if let Some(open) = top_level_char(label, '(') {
        let Some(body) = label.strip_suffix(')') else {
            return Err(Error::new(format!("{label} is not an enum payload value")));
        };
        let variant = &label[..open];
        validate_loaded_ident_field("runtime enum variant value", variant)?;
        return Ok(RuntimeValue::EnumVariant {
            variant: variant.to_string(),
            payload: Box::new(parse_value(&body[open + 1..], depth + 1)?),
        });
    }
    validate_loaded_ident_field("runtime atom value", label)?;
    Ok(RuntimeValue::Atom(label.to_string()))
}

fn parse_record(ty: &str, body: &str, depth: usize) -> Result<RuntimeValue> {
    let mut fields = BTreeMap::new();
    if !body.is_empty() {
        let parts = split_top_level(body, ',')?;
        if parts.len() > MAX_VALUE_TEMPLATE_FIELDS {
            return Err(Error::new(format!(
                "record value {ty}{{{body}}} field count exceeds {MAX_VALUE_TEMPLATE_FIELDS}"
            )));
        }
        for part in parts {
            let index = top_level_char(part, ':').ok_or_else(|| {
                Error::new(format!(
                    "record value {ty}{{{body}}} contains malformed field"
                ))
            })?;
            let name = &part[..index];
            validate_loaded_ident_field("runtime record field", name)?;
            if fields
                .insert(name.to_string(), parse_value(&part[index + 1..], depth)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "record value {ty}{{{body}}} duplicates field {name}"
                )));
            }
        }
    }
    Ok(RuntimeValue::Record {
        constructor: ty.to_string(),
        fields,
    })
}

fn parse_list(body: &str, depth: usize) -> Result<RuntimeValue> {
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
    Ok(RuntimeValue::List(items))
}

fn parse_map(label: &str, body: &str, depth: usize) -> Result<RuntimeValue> {
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
    Ok(RuntimeValue::Map(entries))
}

fn labels(values: &[RuntimeValue]) -> String {
    values
        .iter()
        .map(RuntimeValue::label)
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
                    Error::new(format!("runtime value {value} has unbalanced parentheses"))
                })?
            }
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("runtime value {value} has unbalanced brackets"))
                })?
            }
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    Error::new(format!("runtime value {value} has unbalanced braces"))
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
        return Err(Error::new(format!("runtime value {value} is unbalanced")));
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
