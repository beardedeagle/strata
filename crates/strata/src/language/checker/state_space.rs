use std::collections::BTreeMap;

use mantle_artifact::{
    validate_state_value_label, Error, Result, StateId, MAX_STATE_VALUES_PER_PROCESS,
};

use super::super::ast::{Identifier, Module, Process, Record, TypeRef, ValueExpr};
use super::symbols::SemanticIndex;
use super::STEP_STATE_PARAMETER_NAME;

pub(super) struct StateSpace<'module> {
    module: &'module Module,
    process_name: &'module Identifier,
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
                process_name: &process.name,
                state_type: &process.state_type,
                values,
            });
        }

        let enum_decl = semantic_index.enum_decl(module, &process.state_type)?;
        if enum_decl.variants.is_empty() {
            return Err(Error::new(format!(
                "enum {} must declare at least one variant",
                enum_decl.name
            )));
        }
        let values = enum_decl.variants.iter().map(ToString::to_string).collect();
        Ok(Self {
            module,
            process_name: &process.name,
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
                "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
                self.process_name
            )));
        }
        self.values.push(label);
        StateId::from_index(self.values.len() - 1)
    }

    pub(super) fn into_values(self) -> Result<Vec<String>> {
        validate_state_value_count(self.process_name, self.values.len())?;
        reject_reserved_state_values(self.process_name, &self.values)?;
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

fn validate_state_value_count(process_name: &Identifier, count: usize) -> Result<()> {
    if count == 0 {
        return Err(Error::new(format!(
            "process {} state_value_count must be greater than zero",
            process_name
        )));
    }
    if count > MAX_STATE_VALUES_PER_PROCESS {
        return Err(Error::new(format!(
            "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
            process_name
        )));
    }
    Ok(())
}

fn reject_reserved_state_values(process_name: &Identifier, state_values: &[String]) -> Result<()> {
    if state_values
        .iter()
        .any(|value| value == STEP_STATE_PARAMETER_NAME)
    {
        return Err(Error::new(format!(
            "process {} state value {} conflicts with reserved step state parameter name",
            process_name, STEP_STATE_PARAMETER_NAME
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{
        Determinism, Enum, Function, Identifier, Module, Process, Record, TypeRef, ValueExpr,
    };
    use super::super::symbols::SemanticIndex;
    use super::*;

    #[test]
    fn state_value_limit_reports_process_context() {
        let module = test_module();
        let semantic_index =
            SemanticIndex::build(&module).expect("test module should index successfully");
        let process = &module.processes[0];
        let mut state_space =
            StateSpace::new(&module, &semantic_index, process).expect("state space should build");
        state_space.values = (0..MAX_STATE_VALUES_PER_PROCESS)
            .map(|index| format!("State{index}"))
            .collect();

        let err = state_space
            .resolve_state_value(&semantic_index, &ValueExpr::Identifier(ident("MainState")))
            .expect_err("state value limit should fail");

        assert!(err.to_string().contains(&format!(
            "process Main state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}"
        )));
    }

    fn test_module() -> Module {
        let state_type = TypeRef::Named(ident("MainState"));
        Module {
            name: ident("limit_context"),
            records: vec![Record {
                name: ident("MainState"),
                fields: Vec::new(),
            }],
            enums: vec![Enum {
                name: ident("MainMsg"),
                variants: vec![ident("Start")],
            }],
            processes: vec![Process {
                name: ident("Main"),
                mailbox_bound: 1,
                state_type: state_type.clone(),
                msg_type: TypeRef::Named(ident("MainMsg")),
                init: function("init", state_type.clone()),
                step: function("step", state_type),
            }],
        }
    }

    fn function(name: &str, return_type: TypeRef) -> Function {
        Function {
            name: ident(name),
            params: Vec::new(),
            return_type,
            effects: Vec::new(),
            may: Vec::new(),
            determinism: Determinism::Det,
            body: None,
        }
    }

    fn ident(value: &str) -> Identifier {
        Identifier::new(value).expect("test identifier should be valid")
    }
}
