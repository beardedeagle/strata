use std::collections::BTreeMap;

use mantle_artifact::MAX_OUTPUT_LITERALS;

use super::super::checked::CheckedOutputId;
use super::super::diagnostic::{Error, Result};

pub(super) struct OutputPool {
    values: Vec<String>,
    by_text: BTreeMap<String, CheckedOutputId>,
}

impl OutputPool {
    pub(super) fn new() -> Self {
        Self {
            values: Vec::new(),
            by_text: BTreeMap::new(),
        }
    }

    pub(super) fn intern(&mut self, value: &str) -> Result<CheckedOutputId> {
        if let Some(id) = self.by_text.get(value) {
            return Ok(*id);
        }
        if self.values.len() >= MAX_OUTPUT_LITERALS {
            return Err(Error::new(format!(
                "program emits more than {MAX_OUTPUT_LITERALS} distinct output literals"
            )));
        }
        let id = CheckedOutputId::from_index(self.values.len())?;
        self.values.push(value.to_string());
        self.by_text.insert(value.to_string(), id);
        Ok(id)
    }

    pub(super) fn into_values(self) -> Vec<String> {
        self.values
    }
}
