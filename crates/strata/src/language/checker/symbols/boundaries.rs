use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::language::CAP_TYPE;
use crate::language::ast::Protocol;

pub(super) fn reject_boundary_name_conflict(
    name: &Identifier,
    symbol: Symbol,
    types: &TypeDeclMap,
    processes: &BTreeMap<Symbol, CheckedProcessId>,
) -> Result<()> {
    if types.contains_key(symbol) {
        return Err(Error::new(format!(
            "boundary declaration {name} conflicts with an existing type"
        )));
    }
    if processes.contains_key(&symbol) {
        return Err(Error::new(format!(
            "boundary declaration {name} conflicts with an existing process"
        )));
    }
    Ok(())
}

pub(super) fn reject_duplicate_boundary_name(
    boundary_names: &mut BTreeSet<Symbol>,
    symbol: Symbol,
    name: &Identifier,
) -> Result<()> {
    if boundary_names.insert(symbol) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "duplicate boundary declaration name {name}"
        )))
    }
}

pub(super) fn validate_protocol_message_type(
    symbols: &SymbolTable,
    types: &TypeDeclMap,
    protocol: &Protocol,
) -> Result<()> {
    let TypeRef::Named(name) = &protocol.message_type else {
        return Err(Error::new(format!(
            "protocol {} message type must be a named enum",
            protocol.name
        )));
    };
    let symbol = symbols
        .resolve(name.as_str())
        .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
    match types.get(symbol) {
        Some(TypeDecl::Enum(_)) => Ok(()),
        Some(_) => Err(Error::new(format!(
            "protocol {} message type {} must be an enum",
            protocol.name, protocol.message_type
        ))),
        None => Err(Error::new(format!("type {name} is not declared"))),
    }
}

pub(super) fn validate_boundary_authority(
    ty: &TypeRef,
    descriptor: &str,
    expected: &Identifier,
    kind: &str,
) -> Result<()> {
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = ty
    else {
        return Err(boundary_authority_error(kind, expected, descriptor));
    };
    if constructor.as_str() != CAP_TYPE || !const_args.is_empty() || args.len() != 1 {
        return Err(boundary_authority_error(kind, expected, descriptor));
    }
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = &args[0]
    else {
        return Err(boundary_authority_error(kind, expected, descriptor));
    };
    if constructor.as_str() != descriptor || !const_args.is_empty() || args.len() != 1 {
        return Err(boundary_authority_error(kind, expected, descriptor));
    }
    let TypeRef::Named(target) = &args[0] else {
        return Err(boundary_authority_error(kind, expected, descriptor));
    };
    if target != expected {
        return Err(Error::new(format!(
            "{kind} {expected} authority must be Cap<{descriptor}<{expected}>>"
        )));
    }
    Ok(())
}

pub(super) fn protocol_id_from_map(
    symbols: &SymbolTable,
    protocols: &BTreeMap<Symbol, CheckedProtocolId>,
    name: &Identifier,
) -> Result<CheckedProtocolId> {
    let symbol = symbols
        .resolve(name.as_str())
        .ok_or_else(|| Error::new(format!("protocol {name} is not declared")))?;
    protocols
        .get(&symbol)
        .copied()
        .ok_or_else(|| Error::new(format!("protocol {name} is not declared")))
}

pub(super) fn port_id_from_map(
    symbols: &SymbolTable,
    ports: &BTreeMap<Symbol, CheckedPortId>,
    name: &Identifier,
) -> Result<CheckedPortId> {
    let symbol = symbols
        .resolve(name.as_str())
        .ok_or_else(|| Error::new(format!("port {name} is not declared")))?;
    ports
        .get(&symbol)
        .copied()
        .ok_or_else(|| Error::new(format!("port {name} is not declared")))
}

pub(super) fn process_id_from_map(
    symbols: &SymbolTable,
    processes: &BTreeMap<Symbol, CheckedProcessId>,
    name: &Identifier,
) -> Result<CheckedProcessId> {
    let symbol = symbols
        .resolve(name.as_str())
        .ok_or_else(|| Error::new(format!("process {name} is not declared")))?;
    processes
        .get(&symbol)
        .copied()
        .ok_or_else(|| Error::new(format!("process {name} is not declared")))
}

pub(super) fn same_type_with_symbols(
    symbols: &SymbolTable,
    left: &TypeRef,
    right: &TypeRef,
) -> bool {
    match (left, right) {
        (TypeRef::Named(left), TypeRef::Named(right)) => symbols
            .resolve(left.as_str())
            .zip(symbols.resolve(right.as_str()))
            .is_some_and(|(left, right)| left == right),
        (
            TypeRef::Applied {
                constructor: left_constructor,
                args: left_args,
                const_args: left_const_args,
            },
            TypeRef::Applied {
                constructor: right_constructor,
                args: right_args,
                const_args: right_const_args,
            },
        ) => {
            left_args.len() == right_args.len()
                && left_const_args == right_const_args
                && symbols
                    .resolve(left_constructor.as_str())
                    .zip(symbols.resolve(right_constructor.as_str()))
                    .is_some_and(|(left, right)| left == right)
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| same_type_with_symbols(symbols, left, right))
        }
        _ => false,
    }
}

fn boundary_authority_error(kind: &str, expected: &Identifier, descriptor: &str) -> Error {
    Error::new(format!(
        "{kind} {expected} authority must be Cap<{descriptor}<{expected}>>"
    ))
}
