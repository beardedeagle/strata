use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{MAX_STATE_VALUES_PER_PROCESS, validate_state_value_label};

use super::super::MAX_VALUE_NESTING;
use super::super::ast::{Identifier, Module, Process, Record, TypeRef, ValueExpr};
use super::super::checked::{
    CheckedPayloadValue, CheckedStateId, CheckedStateValue, CheckedTypeRef, CheckedValueTemplate,
    CheckedValueTemplateField,
};
use super::super::diagnostic::{Error, Result};
use super::CheckedTypeInterner;
use super::STEP_STATE_PARAMETER_NAME;
use super::symbols::SemanticIndex;

pub(super) struct StateSpace<'module> {
    module: &'module Module,
    process_name: &'module Identifier,
    state_type: &'module TypeRef,
    checked_state_type: CheckedTypeRef,
    values: Vec<CheckedStateValue>,
}

pub(super) struct ValueBinding<'a> {
    pub(super) name: &'a Identifier,
    pub(super) ty: &'a TypeRef,
    pub(super) label: &'a str,
}

pub(super) struct ValueTemplateBinding<'a> {
    pub(super) name: &'a Identifier,
    pub(super) ty: &'a TypeRef,
    pub(super) checked_ty: &'a CheckedTypeRef,
}

impl<'module> StateSpace<'module> {
    pub(super) fn new(
        module: &'module Module,
        semantic_index: &SemanticIndex,
        process: &'module Process,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Self> {
        let checked_state_type = types.intern(&process.state_type)?;
        if let Ok(record) = semantic_index.record_decl(module, &process.state_type) {
            let values = if record.fields.is_empty() {
                vec![CheckedStateValue::new(
                    checked_state_type.clone(),
                    record.name.to_string(),
                )]
            } else {
                Vec::new()
            };
            return Ok(Self {
                module,
                process_name: &process.name,
                state_type: &process.state_type,
                checked_state_type,
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
        for variant in &enum_decl.variants {
            if variant.payload_type.is_some() {
                return Err(Error::new(format!(
                    "state enum {} variant {} must not declare a payload in this slice",
                    enum_decl.name, variant.name
                )));
            }
        }
        let values = enum_decl
            .variants
            .iter()
            .map(|variant| {
                CheckedStateValue::new(checked_state_type.clone(), variant.name.to_string())
            })
            .collect();
        Ok(Self {
            module,
            process_name: &process.name,
            state_type: &process.state_type,
            checked_state_type,
            values,
        })
    }

    pub(super) fn resolve_state_value(
        &mut self,
        semantic_index: &SemanticIndex,
        value: &ValueExpr,
    ) -> Result<CheckedStateId> {
        self.resolve_state_value_with_bindings(semantic_index, value, &[])
    }

    pub(super) fn resolve_state_value_with_bindings(
        &mut self,
        semantic_index: &SemanticIndex,
        value: &ValueExpr,
        bindings: &[ValueBinding<'_>],
    ) -> Result<CheckedStateId> {
        let label = canonical_value(
            self.module,
            semantic_index,
            self.state_type,
            value,
            bindings,
            0,
        )?;
        let state_value = CheckedStateValue::new(self.checked_state_type.clone(), label);
        if let Some(index) = self.values.iter().position(|candidate| {
            candidate.ty() == state_value.ty() && candidate.value() == state_value.value()
        }) {
            return CheckedStateId::from_index(index);
        }
        if self.values.len() >= MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}",
                self.process_name
            )));
        }
        self.values.push(state_value);
        CheckedStateId::from_index(self.values.len() - 1)
    }

    pub(super) fn into_values(self) -> Result<Vec<CheckedStateValue>> {
        validate_state_value_count(self.process_name, self.values.len())?;
        reject_reserved_state_values(self.process_name, &self.values)?;
        Ok(self.values)
    }
}

pub(super) fn canonical_source_value_with_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
) -> Result<String> {
    canonical_value(module, semantic_index, expected_type, value, bindings, 0)
}

pub(super) fn source_value_uses_binding(value: &ValueExpr, binding: &Identifier) -> bool {
    match value {
        ValueExpr::Identifier(name) => name == binding,
        ValueExpr::Call { arg, .. } => source_value_uses_binding(arg, binding),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_uses_binding(&field.value, binding)),
    }
}

pub(super) fn checked_value_template_with_binding(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    binding: Option<&ValueTemplateBinding<'_>>,
) -> Result<CheckedValueTemplate> {
    checked_value_template(
        module,
        semantic_index,
        types,
        expected_type,
        value,
        binding,
        0,
    )
}

fn canonical_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    depth: usize,
) -> Result<String> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }
    if let ValueExpr::Identifier(name) = value {
        if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
            if semantic_index.same_type(binding.ty, expected_type) {
                return Ok(binding.label.to_string());
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value of type {expected_type}"
        )));
    }
    if let Ok(record) = semantic_index.record_decl(module, expected_type) {
        return canonical_record_value(module, semantic_index, record, value, bindings, depth);
    }

    let enum_decl = semantic_index.enum_decl(module, expected_type)?;
    let ValueExpr::Identifier(name) = value else {
        return Err(Error::new(format!(
            "expected enum variant identifier for enum {}",
            enum_decl.name
        )));
    };
    if let Some(variant) = enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name)
    {
        if variant.payload_type.is_some() {
            return Err(Error::new(format!(
                "enum variant {} requires a payload and cannot be used as a fieldless value",
                variant.name
            )));
        }
        Ok(name.to_string())
    } else {
        Err(Error::new(format!(
            "value {name} is not a variant of enum {}",
            enum_decl.name
        )))
    }
}

fn checked_value_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    binding: Option<&ValueTemplateBinding<'_>>,
    depth: usize,
) -> Result<CheckedValueTemplate> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }

    if let (Some(binding), ValueExpr::Identifier(name)) = (binding, value) {
        if name == binding.name {
            if semantic_index.same_type(binding.ty, expected_type) {
                return Ok(CheckedValueTemplate::ReceivedPayload {
                    ty: binding.checked_ty.clone(),
                });
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
    }
    if let ValueExpr::Call { name, .. } = value {
        return Err(Error::new(format!(
            "function call {name} must be resolved before checking value template of type {expected_type}"
        )));
    }

    if binding.is_none_or(|binding| !source_value_uses_binding(value, binding.name)) {
        let label = canonical_value(module, semantic_index, expected_type, value, &[], depth)?;
        return Ok(CheckedValueTemplate::Literal(CheckedPayloadValue::new(
            types.intern(expected_type)?,
            label,
        )));
    }

    let record = semantic_index.record_decl(module, expected_type)?;
    checked_record_template(module, semantic_index, types, record, value, binding, depth)
}

fn checked_record_template(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
    record: &Record,
    value: &ValueExpr,
    binding: Option<&ValueTemplateBinding<'_>>,
    depth: usize,
) -> Result<CheckedValueTemplate> {
    let ValueExpr::Record(value) = value else {
        return Err(Error::new(format!(
            "record type {} must be constructed with {} {{ ... }}",
            record.name, record.name
        )));
    };
    if value.fields.is_empty() {
        return Err(Error::new(format!(
            "fieldless record values use `{}`; braced record values must declare at least one field",
            value.name
        )));
    }
    if value.name != record.name {
        return Err(Error::new(format!(
            "record constructor {} does not match expected record {}",
            value.name, record.name
        )));
    }

    let declared_fields = record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut provided = BTreeMap::new();
    for field in &value.fields {
        if provided.insert(field.name.as_str(), &field.value).is_some() {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        if !declared_fields.contains(field.name.as_str()) {
            return Err(Error::new(format!(
                "record value {} declares unknown field {}",
                record.name, field.name
            )));
        }
    }

    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(value) = provided.get(field.name.as_str()) else {
            return Err(Error::new(format!(
                "record value {} is missing field {}",
                record.name, field.name
            )));
        };
        fields.push(CheckedValueTemplateField::new(
            field.name.clone(),
            checked_value_template(
                module,
                semantic_index,
                types,
                &field.ty,
                value,
                binding,
                depth + 1,
            )?,
        ));
    }

    Ok(CheckedValueTemplate::Record {
        ty: types.intern(&TypeRef::Named(record.name.clone()))?,
        fields,
    })
}

fn canonical_record_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    record: &Record,
    value: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    depth: usize,
) -> Result<String> {
    if let ValueExpr::Record(value) = value {
        if value.fields.is_empty() {
            return Err(Error::new(format!(
                "fieldless record values use `{}`; braced record values must declare at least one field",
                value.name
            )));
        }
    }

    if record.fields.is_empty() {
        return match value {
            ValueExpr::Identifier(name) if name == &record.name => Ok(record.name.to_string()),
            _ => Err(Error::new(format!(
                "provided value is not a value of record {}",
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

    let declared_fields = record
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut provided = BTreeMap::new();
    for field in &value.fields {
        if provided.insert(field.name.as_str(), &field.value).is_some() {
            return Err(Error::new(format!(
                "record value {} duplicates field {}",
                record.name, field.name
            )));
        }
        if !declared_fields.contains(field.name.as_str()) {
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
        let field_value = canonical_value(
            module,
            semantic_index,
            &field.ty,
            value,
            bindings,
            depth + 1,
        )?;
        parts.push(format!("{}:{field_value}", field.name));
    }
    let label = format!("{}{{{}}}", record.name, parts.join(","));
    validate_state_value_label(&label).map_err(|err| Error::new(err.to_string()))?;
    Ok(label)
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

fn reject_reserved_state_values(
    process_name: &Identifier,
    state_values: &[CheckedStateValue],
) -> Result<()> {
    if state_values
        .iter()
        .any(|value| value.label() == STEP_STATE_PARAMETER_NAME)
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
        Determinism, Enum, EnumVariant, Function, Identifier, Module, Process, Record, RecordField,
        RecordValue, RecordValueField, TypeRef, ValueExpr,
    };
    use super::super::symbols::SemanticIndex;
    use super::*;

    #[test]
    fn state_value_limit_reports_process_context() {
        let module = test_module();
        let semantic_index =
            SemanticIndex::build(&module).expect("test module should index successfully");
        let process = &module.processes[0];
        let mut types = CheckedTypeInterner::new(&semantic_index);
        let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
            .expect("state space should build");
        state_space.values = (0..MAX_STATE_VALUES_PER_PROCESS)
            .map(|index| {
                CheckedStateValue::new(
                    CheckedTypeRef::test_value("MainState"),
                    format!("State{index}"),
                )
            })
            .collect();

        let err = state_space
            .resolve_state_value(&semantic_index, &ValueExpr::Identifier(ident("MainState")))
            .expect_err("state value limit should fail");

        assert!(err.to_string().contains(&format!(
            "process Main state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}"
        )));
    }

    #[test]
    fn state_value_nesting_limit_rejects_programmatic_ast() {
        let module = recursive_state_module();
        let semantic_index =
            SemanticIndex::build(&module).expect("recursive state module should index");
        let process = &module.processes[0];
        let mut types = CheckedTypeInterner::new(&semantic_index);
        let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
            .expect("state space should build");
        let value = nested_record_value(MAX_VALUE_NESTING + 1);

        let err = state_space
            .resolve_state_value(&semantic_index, &value)
            .expect_err("excessive AST value nesting should fail");

        assert!(
            err.to_string()
                .contains("value nesting exceeds maximum depth")
        );
    }

    #[test]
    fn state_space_rejects_empty_braced_record_value_ast() {
        let module = test_module();
        let semantic_index =
            SemanticIndex::build(&module).expect("test module should index successfully");
        let process = &module.processes[0];
        let mut types = CheckedTypeInterner::new(&semantic_index);
        let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
            .expect("state space should build");
        let value = ValueExpr::Record(RecordValue {
            name: ident("MainState"),
            fields: Vec::new(),
        });

        let err = state_space
            .resolve_state_value(&semantic_index, &value)
            .expect_err("empty braced record value AST should fail");

        assert!(err.to_string().contains(
            "fieldless record values use `MainState`; braced record values must declare at least one field"
        ));
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
                variants: vec![unit_variant("Start")],
            }],
            functions: Vec::new(),
            processes: vec![Process {
                name: ident("Main"),
                mailbox_bound: 1,
                state_type: state_type.clone(),
                msg_type: TypeRef::Named(ident("MainMsg")),
                init: function("init", state_type.clone()),
                functions: Vec::new(),
                steps: vec![function("step", state_type)],
            }],
        }
    }

    fn recursive_state_module() -> Module {
        let state_type = TypeRef::Named(ident("MainState"));
        Module {
            name: ident("recursive_state"),
            records: vec![Record {
                name: ident("MainState"),
                fields: vec![RecordField {
                    name: ident("next"),
                    ty: state_type.clone(),
                }],
            }],
            enums: vec![Enum {
                name: ident("MainMsg"),
                variants: vec![unit_variant("Start")],
            }],
            functions: Vec::new(),
            processes: vec![Process {
                name: ident("Main"),
                mailbox_bound: 1,
                state_type: state_type.clone(),
                msg_type: TypeRef::Named(ident("MainMsg")),
                init: function("init", state_type.clone()),
                functions: Vec::new(),
                steps: vec![function("step", state_type)],
            }],
        }
    }

    fn nested_record_value(depth: usize) -> ValueExpr {
        let mut value = ValueExpr::Identifier(ident("MainState"));
        for _ in 0..depth {
            value = ValueExpr::Record(RecordValue {
                name: ident("MainState"),
                fields: vec![RecordValueField {
                    name: ident("next"),
                    value,
                }],
            });
        }
        value
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

    fn unit_variant(value: &str) -> EnumVariant {
        EnumVariant {
            name: ident(value),
            payload_type: None,
        }
    }
}
