use std::collections::BTreeSet;

use mantle_artifact::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactStateValue, ArtifactValue,
    ArtifactValueTemplate, ArtifactValueTemplateMapEntry, Error, MAX_VALUE_TEMPLATE_FIELDS,
    MapProjectionMode, ProcessId, ProcessRefId, Result, TypeId,
};

use crate::event::RuntimeProcessId;

pub(crate) type RuntimeValue = ArtifactValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePayload {
    pub(crate) ty: TypeId,
    pub(crate) value: RuntimeValue,
    pub(crate) process_ref: Option<ArtifactProcessRefPayload>,
}

impl RuntimePayload {
    pub(crate) fn from_artifact(payload: &ArtifactPayload) -> Result<Self> {
        payload.value.validate("payload value")?;
        if let Some(process_ref) = payload.process_ref {
            let pid = RuntimeProcessId::from_u64(process_ref.pid)?;
            let value = RuntimeValue::process_ref(payload.ty, pid.as_u64());
            let expected_label = value.label();
            if payload.value != value {
                return Err(Error::new(format!(
                    "process reference payload value {} does not match metadata label {}",
                    payload.value.label(),
                    expected_label
                )));
            }
            return Ok(Self {
                ty: payload.ty,
                value,
                process_ref: Some(process_ref),
            });
        }
        if payload.value.contains_process_ref() {
            return Err(Error::new(
                "process reference payloads require process reference metadata",
            ));
        }
        Ok(Self {
            ty: payload.ty,
            value: payload.value.clone(),
            process_ref: payload.process_ref,
        })
    }

    pub(crate) fn value(ty: TypeId, value: RuntimeValue) -> Result<Self> {
        value.validate("payload value")?;
        if value.contains_process_ref() {
            return Err(Error::new(
                "process reference payloads require process reference metadata",
            ));
        }
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
        let value = RuntimeValue::process_ref(ty, pid.as_u64());
        value.validate("process reference payload value")?;
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
        state.value.validate("state value")?;
        if state.value.contains_process_ref() {
            return Err(Error::new(
                "process reference payloads are not valid state values",
            ));
        }
        Ok(Self {
            ty: state.ty,
            value: state.value.clone(),
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
            ArtifactValueTemplate::Literal { ty, value } => {
                value.validate("literal value template")?;
                if value.contains_process_ref() {
                    return Err(Error::new(
                        "process reference template requires runtime process reference bindings",
                    ));
                }
                Ok(Self::Literal {
                    ty: *ty,
                    value: value.clone(),
                })
            }
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
            } => {
                validate_map_projection_keys(key, keys)?;
                Ok(Self::MapValue {
                    ty: *ty,
                    map: Box::new(Self::from_artifact(map)?),
                    key: key.clone(),
                    keys: keys.clone(),
                    projection: *projection,
                })
            }
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

fn validate_map_projection_keys(key: &RuntimeValue, keys: &[RuntimeValue]) -> Result<()> {
    if keys.is_empty() {
        return Err(Error::new(
            "map projection key_count must be greater than zero",
        ));
    }
    if keys.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "map projection key_count must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    validate_non_process_ref_value("map projection key", key)?;
    let mut seen = BTreeSet::new();
    for expected_key in keys {
        validate_non_process_ref_value("map projection expected key", expected_key)?;
        if !seen.insert(expected_key.clone()) {
            return Err(Error::new(format!(
                "map projection duplicates expected map key {}",
                expected_key.label()
            )));
        }
    }
    if !seen.contains(key) {
        return Err(Error::new(format!(
            "map projection key {} is not one of the expected map keys",
            key.label()
        )));
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(
            "map projection expected keys must be sorted canonically",
        ));
    }
    Ok(())
}

fn validate_non_process_ref_value(field: &str, value: &RuntimeValue) -> Result<()> {
    value.validate(field)?;
    if value.contains_process_ref() {
        return Err(Error::new(format!(
            "{field} must not contain a process reference value"
        )));
    }
    Ok(())
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
