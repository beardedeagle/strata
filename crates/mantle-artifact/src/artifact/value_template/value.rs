use std::collections::BTreeSet;

use super::model::{ArtifactValue, MapProjectionMode};
use super::parsing::parse_value;
use super::projection::{labels, validate_projection_keys};
use crate::validation::{validate_count, validate_ident_field, validate_value_label};
use crate::{Error, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, Result, TypeId};

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
                let mut seen = BTreeSet::new();
                for entry in fields {
                    let name = entry.name.as_str();
                    validate_ident_field(&format!("{field}.field"), name)?;
                    if !seen.insert(name) {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            entry.name
                        )));
                    }
                    entry
                        .value
                        .validate_shape(&format!("{field}.field.{name}"), depth + 1)?;
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
                let mut keys = BTreeSet::new();
                for (index, entry) in entries.iter().enumerate() {
                    entry
                        .key
                        .validate_shape(&format!("{field}.entry.{index}.key"), depth + 1)?;
                    if !keys.insert(entry.key.clone()) {
                        return Err(Error::new(format!(
                            "{field} duplicates key {}",
                            entry.key.label()
                        )));
                    }
                    entry
                        .value
                        .validate_shape(&format!("{field}.entry.{index}.value"), depth + 1)?;
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
                    .map(|field| format!("{}:{}", field.name, field.value.label()))
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
                    .map(|entry| format!("{}=>{}", entry.key.label(), entry.value.label()))
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
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.contains_process_ref()),
            Self::List(items) => items.iter().any(ArtifactValue::contains_process_ref),
            Self::Map(entries) => entries.iter().any(|entry| {
                entry.key.contains_process_ref() || entry.value.contains_process_ref()
            }),
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
        fields
            .iter()
            .find(|entry| entry.name == field)
            .map(|entry| entry.value.clone())
            .ok_or_else(|| {
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
        let entry_keys = entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect::<Vec<_>>();
        match projection {
            MapProjectionMode::Exact => {
                if entry_keys.len() != keys.len()
                    || !keys
                        .iter()
                        .all(|expected_key| entries.iter().any(|entry| entry.key == *expected_key))
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
                    if !entries.iter().any(|entry| entry.key == *expected_key) {
                        return Err(Error::new(format!(
                            "map projection expected key {}, found [{}]",
                            expected_key.label(),
                            labels(&entry_keys)
                        )));
                    }
                }
            }
        }
        entries
            .iter()
            .find(|entry| entry.key == *key)
            .map(|entry| entry.value.clone())
            .ok_or_else(|| {
                Error::new(format!(
                    "map projection key {} is not present in {}",
                    key.label(),
                    self.label()
                ))
            })
    }
}
