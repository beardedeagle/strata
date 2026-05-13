use mantle_artifact::{ArtifactValue, MAX_STATE_VALUES_PER_PROCESS};

use super::super::super::MAX_VALUE_NESTING;
use super::super::super::ast::{
    Determinism, Enum, EnumVariant, Function, Identifier, Module, Process, Record, RecordField,
    RecordValue, RecordValueField, TypeRef, ValueExpr,
};
use super::super::super::checked::{CheckedStateValue, CheckedTypeRef};
use super::super::CheckedTypeInterner;
use super::super::symbols::SemanticIndex;
use super::*;

#[test]
fn state_value_limit_reports_process_context() {
    let module = test_module();
    let semantic_index =
        SemanticIndex::build(&module).expect("test module should index successfully");
    let process = &module.processes[0];
    let mut types = CheckedTypeInterner::new(&module, &semantic_index);
    let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
        .expect("state space should build");
    state_space.values = (0..MAX_STATE_VALUES_PER_PROCESS)
        .map(|index| {
            CheckedStateValue::new(
                CheckedTypeRef::test_value("MainState"),
                ArtifactValue::Atom(format!("State{index}")),
            )
        })
        .collect();

    let err = state_space
        .resolve_state_value(
            &semantic_index,
            &mut types,
            &ValueExpr::Identifier(ident("MainState")),
        )
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
    let mut types = CheckedTypeInterner::new(&module, &semantic_index);
    let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
        .expect("state space should build");
    let value = nested_record_value(MAX_VALUE_NESTING + 1);

    let err = state_space
        .resolve_state_value(&semantic_index, &mut types, &value)
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
    let mut types = CheckedTypeInterner::new(&module, &semantic_index);
    let mut state_space = StateSpace::new(&module, &semantic_index, process, &mut types)
        .expect("state space should build");
    let value = ValueExpr::Record(RecordValue {
        name: ident("MainState"),
        fields: Vec::new(),
    });

    let err = state_space
        .resolve_state_value(&semantic_index, &mut types, &value)
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
