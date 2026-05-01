use std::collections::BTreeMap;

use mantle_artifact::{Error, OutputId, Result, MAX_OUTPUT_LITERALS};

pub(super) struct OutputPool {
    values: Vec<String>,
    by_text: BTreeMap<String, OutputId>,
}

impl OutputPool {
    pub(super) fn new() -> Self {
        Self {
            values: Vec::new(),
            by_text: BTreeMap::new(),
        }
    }

    pub(super) fn intern(&mut self, value: &str) -> Result<OutputId> {
        if let Some(id) = self.by_text.get(value) {
            return Ok(*id);
        }
        if self.values.len() >= MAX_OUTPUT_LITERALS {
            return Err(Error::new(format!(
                "program emits more than {MAX_OUTPUT_LITERALS} distinct output literals"
            )));
        }
        let id = OutputId::from_index(self.values.len())?;
        self.values.push(value.to_string());
        self.by_text.insert(value.to_string(), id);
        Ok(id)
    }

    pub(super) fn into_values(self) -> Vec<String> {
        self.values
    }
}
