use std::collections::BTreeMap;

use mantle_artifact::{
    Error, Result, StateId, MAX_FIELD_VALUE_BYTES, MAX_STATE_VALUES_PER_PROCESS,
};

use super::super::ast::{Module, Process, Record, TypeRef, ValueExpr};
use super::symbols::SemanticIndex;

const STEP_STATE_PARAMETER_NAME: &str = "state";

pub(super) struct StateSpace<'module> {
    module: &'module Module,
    state_type: &'module TypeRef,
    values: Vec<String>,
}

impl<'module> StateSpace<'module> {
    pub(super) fn new(
        module: &'module Module,
        semantic_index: &SemanticIndex,
        process: &'module Process,
    ) -> Result<Self> {
        if let Ok(record) = semantic_index.record_decl(module, &process.state_type) {
            let values = if record.fields.is_empty() {
                vec![record.name.to_string()]
            } else {
                Vec::new()
            };
            return Ok(Self {
                module,
                state_type: &process.state_type,
                values,
            });
        }

        let enum_decl = semantic_index.enum_decl(module, &process.state_type)?;
        let values = enum_decl.variants.iter().map(ToString::to_string).collect();
        Ok(Self {
            module,
            state_type: &process.state_type,
            values,
        })
    }

    pub(super) fn resolve_state_value(
        &mut self,
        semantic_index: &SemanticIndex,
        value: &ValueExpr,
    ) -> Result<StateId> {
        let label = self.canonical_value(semantic_index, self.state_type, value)?;
        if let Some(index) = self.values.iter().position(|candidate| candidate == &label) {
            return StateId::from_index(index);
        }
        if self.values.len() >= MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}"
            )));
        }
        self.values.push(label);
        StateId::from_index(self.values.len() - 1)
    }

    pub(super) fn into_values(self, process: &Process) -> Result<Vec<String>> {
        validate_state_value_count(process, self.values.len())?;
        reject_reserved_state_values(process, &self.values)?;
        Ok(self.values)
    }

    fn canonical_value(
        &self,
        semantic_index: &SemanticIndex,
        expected_type: &TypeRef,
        value: &ValueExpr,
    ) -> Result<String> {
        if let Ok(record) = semantic_index.record_decl(self.module, expected_type) {
            return self.canonical_record_value(semantic_index, record, value);
        }

        let enum_decl = semantic_index.enum_decl(self.module, expected_type)?;
        let ValueExpr::Identifier(name) = value else {
            return Err(Error::new(format!(
                "value {value} is not a variant of enum {}",
                enum_decl.name
            )));
        };
        if enum_decl.variants.iter().any(|variant| variant == name) {
            Ok(name.to_string())
        } else {
            Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )))
        }
    }

    fn canonical_record_value(
        &self,
        semantic_index: &SemanticIndex,
        record: &Record,
        value: &ValueExpr,
    ) -> Result<String> {
        if record.fields.is_empty() {
            return match value {
                ValueExpr::Identifier(name) if name == &record.name => Ok(record.name.to_string()),
                ValueExpr::Record(value)
                    if value.name == record.name && value.fields.is_empty() =>
                {
                    Ok(record.name.to_string())
                }
                _ => Err(Error::new(format!(
                    "value {value} is not a value of record {}",
                    record.name
                ))),
            };
        }

        let ValueExpr::Record(value) = value else {
            return Err(Error::new(format!(
                "record state type {} must be constructed with {} {{ ... }}",
                record.name, record.name
            )));
        };
        if value.name != record.name {
            return Err(Error::new(format!(
                "record constructor {} does not match expected record {}",
                value.name, record.name
            )));
        }

        let mut provided = BTreeMap::new();
        for field in &value.fields {
            if provided.insert(field.name.as_str(), &field.value).is_some() {
                return Err(Error::new(format!(
                    "record value {} duplicates field {}",
                    record.name, field.name
                )));
            }
            if !record
                .fields
                .iter()
                .any(|declared| declared.name == field.name)
            {
                return Err(Error::new(format!(
                    "record value {} declares unknown field {}",
                    record.name, field.name
                )));
            }
        }

        let mut parts = Vec::with_capacity(record.fields.len());
        for field in &record.fields {
            let Some(value) = provided.get(field.name.as_str()) else {
                return Err(Error::new(format!(
                    "record value {} is missing field {}",
                    record.name, field.name
                )));
            };
            let field_value = self.canonical_value(semantic_index, &field.ty, value)?;
            parts.push(format!("{}:{field_value}", field.name));
        }
        let label = format!("{}{{{}}}", record.name, parts.join(","));
        validate_state_value_label(&label)?;
        Ok(label)
    }
}

fn validate_state_value_count(process: &Process, count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process.name
        )));
    }
    if count > MAX_STATE_VALUES_PER_PROCESS {
        return Err(Error::new(format!(
            "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
            process.name
        )));
    }
    Ok(())
}

fn reject_reserved_state_values(process: &Process, state_values: &[String]) -> Result<()> {
    if state_values
        .iter()
        .any(|value| value == STEP_STATE_PARAMETER_NAME)
    {
        return Err(Error::new(format!(
            "process {} state value {} conflicts with reserved step state parameter name",
            process.name, STEP_STATE_PARAMETER_NAME
        )));
    }
    Ok(())
}

fn validate_state_value_label(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::new("state value must not be empty"));
    }
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "state value exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::new(
            "state value must not contain control characters",
        ));
    }
    Ok(())
}
