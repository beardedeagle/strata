use std::collections::BTreeMap;

use crate::artifact::StepResult;
use crate::validation::validate_count;
use crate::{
    ARTIFACT_MAGIC, EnumVariantId, Error, LoopElementId, MAX_ARTIFACT_BYTES, MAX_ARTIFACT_FIELDS,
    MAX_ENUM_VARIANTS_PER_TYPE, MAX_FIELD_VALUE_BYTES, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT, MAX_PROCESS_REFS_PER_PROCESS,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_FIELDS, MessageId, OutputId,
    ProcessId, ProcessRefId, Result, StateId, TypeId,
};

pub(crate) struct ArtifactFields {
    fields: BTreeMap<String, String>,
}

impl ArtifactFields {
    pub(crate) fn parse(contents: &str) -> Result<Self> {
        if contents.len() > MAX_ARTIFACT_BYTES {
            return Err(Error::new(format!(
                "artifact is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes"
            )));
        }

        let mut lines = contents.lines();
        match lines.next() {
            Some(ARTIFACT_MAGIC) => {}
            Some(other) => {
                return Err(Error::new(format!(
                    "invalid Mantle artifact magic {other:?}; expected {ARTIFACT_MAGIC:?}"
                )));
            }
            None => return Err(Error::new("empty Mantle artifact")),
        }

        let mut fields = BTreeMap::new();
        for line in lines {
            if fields.len() >= MAX_ARTIFACT_FIELDS {
                return Err(Error::new(format!(
                    "artifact declares too many fields; maximum supported count is {MAX_ARTIFACT_FIELDS}"
                )));
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::new(format!("invalid artifact field line {line:?}")));
            };
            if value.len() > MAX_FIELD_VALUE_BYTES {
                return Err(Error::new(format!(
                    "artifact field {key:?} exceeds maximum value length of {MAX_FIELD_VALUE_BYTES} bytes"
                )));
            }
            if fields.insert(key.to_string(), value.to_string()).is_some() {
                return Err(Error::new(format!("duplicate artifact field {key:?}")));
            }
        }
        Ok(Self { fields })
    }

    pub(crate) fn take_required(&mut self, key: &str) -> Result<String> {
        self.fields
            .remove(key)
            .ok_or_else(|| Error::new(format!("missing artifact field {key}")))
    }

    pub(crate) fn take_optional(&mut self, key: &str) -> Option<String> {
        self.fields.remove(key)
    }

    pub(crate) fn take_bounded_usize(
        &mut self,
        key: &str,
        min: usize,
        max: usize,
    ) -> Result<usize> {
        let value = self.take_usize(key)?;
        validate_count(key, value, min, max)?;
        Ok(value)
    }

    pub(crate) fn take_process_id(&mut self, key: &str) -> Result<ProcessId> {
        ProcessId::from_index(self.take_bounded_usize(key, 0, MAX_PROCESS_COUNT - 1)?)
    }

    pub(crate) fn take_process_ref_id(&mut self, key: &str) -> Result<ProcessRefId> {
        ProcessRefId::from_index(self.take_bounded_usize(
            key,
            0,
            MAX_PROCESS_REFS_PER_PROCESS - 1,
        )?)
    }

    pub(crate) fn take_state_id(&mut self, key: &str) -> Result<StateId> {
        StateId::from_index(self.take_bounded_usize(key, 0, MAX_STATE_VALUES_PER_PROCESS - 1)?)
    }

    pub(crate) fn take_optional_state_id(&mut self, key: &str) -> Result<Option<StateId>> {
        let Some(value) = self.take_optional(key) else {
            return Ok(None);
        };
        let index = value
            .parse::<usize>()
            .map_err(|_| Error::new(format!("invalid {key} value {value:?}")))?;
        validate_count(key, index, 0, MAX_STATE_VALUES_PER_PROCESS - 1)?;
        Ok(Some(StateId::from_index(index)?))
    }

    pub(crate) fn take_message_id(&mut self, key: &str) -> Result<MessageId> {
        MessageId::from_index(self.take_bounded_usize(
            key,
            0,
            MAX_MESSAGE_VARIANTS_PER_PROCESS - 1,
        )?)
    }

    pub(crate) fn take_output_id(&mut self, key: &str) -> Result<OutputId> {
        OutputId::from_index(self.take_bounded_usize(key, 0, MAX_OUTPUT_LITERALS - 1)?)
    }

    pub(crate) fn take_type_id(&mut self, key: &str) -> Result<TypeId> {
        TypeId::from_index(self.take_bounded_usize(key, 0, MAX_TYPE_COUNT - 1)?)
    }

    pub(crate) fn take_enum_variant_id(&mut self, key: &str) -> Result<EnumVariantId> {
        EnumVariantId::from_index(self.take_bounded_usize(
            key,
            0,
            MAX_ENUM_VARIANTS_PER_TYPE - 1,
        )?)
    }

    pub(crate) fn take_loop_element_id(&mut self, key: &str) -> Result<LoopElementId> {
        LoopElementId::from_index(self.take_bounded_usize(key, 0, MAX_VALUE_TEMPLATE_FIELDS - 1)?)
    }

    pub(crate) fn take_optional_type_id(&mut self, key: &str) -> Result<Option<TypeId>> {
        let Some(value) = self.take_optional(key) else {
            return Ok(None);
        };
        let index = value
            .parse::<usize>()
            .map_err(|_| Error::new(format!("invalid {key} value {value:?}")))?;
        validate_count(key, index, 0, MAX_TYPE_COUNT - 1)?;
        Ok(Some(TypeId::from_index(index)?))
    }

    pub(crate) fn take_step_result(&mut self, key: &str) -> Result<StepResult> {
        StepResult::parse(&self.take_required(key)?)
    }

    pub(crate) fn finish(self) -> Result<()> {
        if let Some(key) = self.fields.keys().next() {
            return Err(Error::new(format!("unknown artifact field {key:?}")));
        }
        Ok(())
    }

    fn take_usize(&mut self, key: &str) -> Result<usize> {
        let value = self.take_required(key)?;
        value
            .parse::<usize>()
            .map_err(|_| Error::new(format!("invalid {key} value {value:?}")))
    }
}
