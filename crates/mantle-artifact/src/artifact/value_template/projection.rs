use std::collections::BTreeSet;

use super::model::{ArtifactValue, MapProjectionMode};
use crate::validation::validate_count;
use crate::{Error, MAX_VALUE_TEMPLATE_FIELDS, Result};

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
            "{field} expected map keys must be sorted canonically"
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
