use std::collections::BTreeSet;

use mantle_artifact::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactStateValue, ArtifactValue,
    ArtifactValueTemplate, ArtifactValueTemplateMapEntry, EnumVariantId, Error, LoopElementId,
    MAX_VALUE_TEMPLATE_FIELDS, MapProjectionMode, ProcessId, ProcessRefId, Result, TypeId,
    validate_state_value_identity_label,
};

use crate::event::RuntimeProcessId;

pub(crate) type RuntimeValue = ArtifactValue;

#[derive(Debug, Clone, Copy)]
enum MapKeySetKind {
    Expected,
    Excluded,
}

impl MapKeySetKind {
    const fn field_name(self) -> &'static str {
        match self {
            Self::Expected => "expected_key",
            Self::Excluded => "excluded_key",
        }
    }

    const fn singular(self) -> &'static str {
        match self {
            Self::Expected => "expected map key",
            Self::Excluded => "excluded map key",
        }
    }

    const fn plural(self) -> &'static str {
        match self {
            Self::Expected => "expected map keys",
            Self::Excluded => "excluded map keys",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePayload {
    pub(crate) ty: TypeId,
    pub(crate) value: RuntimeValue,
    label: String,
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
                label: expected_label,
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
            label: payload.value.label(),
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
            label: value.label(),
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
            label: value.label(),
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

    pub fn label(&self) -> &str {
        self.label.as_str()
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
        validate_state_value_identity_label(&state.value, &state.label)?;
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
        let label = payload.label;
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
    EnumPayload {
        ty: TypeId,
        value: Box<LoadedValueTemplate>,
        variant: EnumVariantId,
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
    ListPrefixElement {
        ty: TypeId,
        list: Box<LoadedValueTemplate>,
        index: usize,
        prefix_len: usize,
    },
    ListRest {
        ty: TypeId,
        list: Box<LoadedValueTemplate>,
        prefix_len: usize,
    },
    MapValue {
        ty: TypeId,
        map: Box<LoadedValueTemplate>,
        key: RuntimeValue,
        keys: Vec<RuntimeValue>,
        projection: MapProjectionMode,
    },
    MapRest {
        ty: TypeId,
        map: Box<LoadedValueTemplate>,
        excluded_keys: Vec<RuntimeValue>,
    },
    ProcessRef {
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    LoopElement {
        ty: TypeId,
        element: LoopElementId,
    },
    EnumVariant {
        ty: TypeId,
        variant: EnumVariantId,
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
            ArtifactValueTemplate::EnumPayload { ty, value, variant } => Ok(Self::EnumPayload {
                ty: *ty,
                value: Box::new(Self::from_artifact(value)?),
                variant: *variant,
            }),
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
            ArtifactValueTemplate::ListPrefixElement {
                ty,
                list,
                index,
                prefix_len,
            } => {
                validate_list_prefix_projection("list prefix projection", *index, *prefix_len)?;
                Ok(Self::ListPrefixElement {
                    ty: *ty,
                    list: Box::new(Self::from_artifact(list)?),
                    index: *index,
                    prefix_len: *prefix_len,
                })
            }
            ArtifactValueTemplate::ListRest {
                ty,
                list,
                prefix_len,
            } => {
                validate_list_rest_prefix_len("list rest projection", *prefix_len)?;
                Ok(Self::ListRest {
                    ty: *ty,
                    list: Box::new(Self::from_artifact(list)?),
                    prefix_len: *prefix_len,
                })
            }
            ArtifactValueTemplate::MapValue {
                ty,
                map,
                key,
                keys,
                projection,
            } => {
                validate_map_projection_keys("map projection", key, keys)?;
                Ok(Self::MapValue {
                    ty: *ty,
                    map: Box::new(Self::from_artifact(map)?),
                    key: key.clone(),
                    keys: keys.clone(),
                    projection: *projection,
                })
            }
            ArtifactValueTemplate::MapRest {
                ty,
                map,
                excluded_keys,
            } => {
                validate_map_rest_keys("map rest projection", excluded_keys)?;
                Ok(Self::MapRest {
                    ty: *ty,
                    map: Box::new(Self::from_artifact(map)?),
                    excluded_keys: excluded_keys.clone(),
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
            ArtifactValueTemplate::LoopElement { ty, element } => Ok(Self::LoopElement {
                ty: *ty,
                element: *element,
            }),
            ArtifactValueTemplate::EnumVariant {
                ty,
                variant,
                payload,
            } => Ok(Self::EnumVariant {
                ty: *ty,
                variant: *variant,
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
}

pub(super) fn validate_map_projection_keys(
    field: &str,
    key: &RuntimeValue,
    keys: &[RuntimeValue],
) -> Result<()> {
    validate_map_key_set(field, keys, MapKeySetKind::Expected)?;
    validate_non_process_ref_value(&format!("{field}.key"), key)?;
    if keys.binary_search(key).is_err() {
        return Err(Error::new(format!(
            "{field} projection key {} is not one of the expected map keys",
            key.label()
        )));
    }
    Ok(())
}

pub(super) fn validate_map_rest_keys(field: &str, keys: &[RuntimeValue]) -> Result<()> {
    validate_map_key_set(field, keys, MapKeySetKind::Excluded)
}

pub(super) fn validate_list_rest_prefix_len(field: &str, prefix_len: usize) -> Result<()> {
    if prefix_len == 0 || prefix_len > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "{field}.prefix_len must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    Ok(())
}

pub(super) fn validate_list_prefix_projection(
    field: &str,
    index: usize,
    prefix_len: usize,
) -> Result<()> {
    validate_list_rest_prefix_len(field, prefix_len)?;
    if index >= prefix_len {
        return Err(Error::new(format!(
            "{field}.index {index} is outside list prefix length {prefix_len}"
        )));
    }
    Ok(())
}

fn validate_map_key_set(field: &str, keys: &[RuntimeValue], kind: MapKeySetKind) -> Result<()> {
    if keys.is_empty() || keys.len() > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "{field}.key_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    let mut seen = BTreeSet::new();
    for map_key in keys {
        validate_non_process_ref_value(&format!("{field}.{}", kind.field_name()), map_key)?;
        if !seen.insert(map_key.clone()) {
            return Err(Error::new(format!(
                "{field} duplicates {} {}",
                kind.singular(),
                map_key.label()
            )));
        }
    }
    if seen.into_iter().collect::<Vec<_>>() != keys {
        return Err(Error::new(format!(
            "{field} {} must be sorted canonically",
            kind.plural()
        )));
    }
    Ok(())
}

pub(super) fn validate_non_process_ref_value(field: &str, value: &RuntimeValue) -> Result<()> {
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
