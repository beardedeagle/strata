use std::collections::BTreeSet;

use super::model::{ArtifactValue, MapProjectionMode};
use crate::validation::validate_count;
use crate::{Error, MAX_VALUE_TEMPLATE_FIELDS, Result};

#[derive(Debug, Clone, Copy)]
pub(super) enum ProjectionKeySetKind {
    Expected,
    Excluded,
}

impl ProjectionKeySetKind {
    fn field_name(self) -> &'static str {
        match self {
            Self::Expected => "expected_key",
            Self::Excluded => "excluded_key",
        }
    }

    fn singular(self) -> &'static str {
        match self {
            Self::Expected => "expected map key",
            Self::Excluded => "excluded map key",
        }
    }

    fn plural(self) -> &'static str {
        match self {
            Self::Expected => "expected map keys",
            Self::Excluded => "excluded map keys",
        }
    }
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

pub(super) fn validate_projection_keys(
    field: &str,
    key: &ArtifactValue,
    keys: &[ArtifactValue],
) -> Result<()> {
    validate_projection_key_set(field, keys, ProjectionKeySetKind::Expected)?;
    key.validate_without_process_ref(&format!("{field}.key"))?;
    if keys.binary_search(key).is_err() {
        return Err(Error::new(format!(
            "{field} projection key {} is not one of the expected map keys",
            key.label()
        )));
    }
    Ok(())
}

pub(super) fn validate_projection_key_set(
    field: &str,
    keys: &[ArtifactValue],
    kind: ProjectionKeySetKind,
) -> Result<()> {
    validate_count(
        &format!("{field}.key_count"),
        keys.len(),
        1,
        MAX_VALUE_TEMPLATE_FIELDS,
    )?;
    let mut seen = BTreeSet::new();
    for map_key in keys {
        map_key.validate_without_process_ref(&format!("{field}.{}", kind.field_name()))?;
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

pub(super) fn labels(values: &[ArtifactValue]) -> String {
    values
        .iter()
        .map(ArtifactValue::label)
        .collect::<Vec<_>>()
        .join(",")
}
