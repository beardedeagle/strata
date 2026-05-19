use std::collections::BTreeMap;

use mantle_artifact::MAX_VALUE_TEMPLATE_FIELDS;

use super::super::super::ast::{Enum, EnumVariant, Identifier, Record, TypeRef};
use super::super::super::checked::CheckedProcessId;
use super::super::super::diagnostic::{Error, Result};
use super::super::CHECKED_TYPE_LABEL_PREFIX;
use super::{Symbol, SymbolTable, TypeDecl};

pub(super) fn reject_reserved_type_name(
    name: &str,
    symbol: Symbol,
    reserved: Symbol,
) -> Result<()> {
    if symbol == reserved {
        return Err(Error::new(format!("type name {name} is reserved")));
    }
    Ok(())
}

pub(super) fn reject_internal_type_label_prefix(name: &str) -> Result<()> {
    if name.starts_with(CHECKED_TYPE_LABEL_PREFIX) {
        return Err(Error::new(format!(
            "type name {name} uses reserved prefix {CHECKED_TYPE_LABEL_PREFIX}"
        )));
    }
    Ok(())
}

pub(super) fn validate_record_fields(
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
    process_ref_type: Symbol,
    list_type: Symbol,
    map_type: Symbol,
    record: &Record,
) -> Result<()> {
    let mut field_names = BTreeMap::new();
    for field in &record.fields {
        let field_symbol = symbols
            .resolve(field.name.as_str())
            .ok_or_else(|| Error::new(format!("field {} is not interned", field.name)))?;
        if field_names.insert(field_symbol, ()).is_some() {
            return Err(Error::new(format!(
                "record {} declares duplicate field {}",
                record.name, field.name
            )));
        }
        if let Err(err) = validate_source_value_type(
            symbols,
            types,
            process_ref_type,
            list_type,
            map_type,
            &field.ty,
        ) {
            if collection_type_signature_error(symbols, list_type, map_type, &field.ty) {
                return Err(err);
            }
            if type_contains_process_ref(symbols, process_ref_type, &field.ty) {
                return Err(Error::new(format!(
                    "record {} field {} type {} contains a process reference; process references must be direct message payloads",
                    record.name, field.name, field.ty
                )));
            }
            return Err(Error::new(format!(
                "record {} field {} uses undeclared type {}",
                record.name, field.name, field.ty
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MessagePayloadTypeContext<'a> {
    pub(super) symbols: &'a SymbolTable,
    pub(super) types: &'a BTreeMap<Symbol, TypeDecl>,
    pub(super) processes: &'a BTreeMap<Symbol, CheckedProcessId>,
    pub(super) process_ref_type: Symbol,
    pub(super) list_type: Symbol,
    pub(super) map_type: Symbol,
}

pub(super) fn validate_message_payload_type(
    context: MessagePayloadTypeContext<'_>,
    enum_decl: &Enum,
    variant: &EnumVariant,
    payload_type: &TypeRef,
) -> Result<()> {
    match payload_type {
        TypeRef::Named(name) => {
            if is_process_ref_name(context.symbols, context.process_ref_type, name) {
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} must declare exactly one target process",
                    enum_decl.name, variant.name, payload_type
                )));
            }
            validate_source_value_type(
                context.symbols,
                context.types,
                context.process_ref_type,
                context.list_type,
                context.map_type,
                payload_type,
            )
            .map_err(|_| {
                Error::new(format!(
                    "enum {} variant {} uses undeclared payload type {}",
                    enum_decl.name, variant.name, payload_type
                ))
            })?;
        }
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let Some(constructor_symbol) = context.symbols.resolve(constructor.as_str()) else {
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} must be a named record, enum, list, map, or process reference type",
                    enum_decl.name, variant.name, payload_type
                )));
            };
            if constructor_symbol == context.process_ref_type
                && args.len() == 1
                && const_args.is_empty()
            {
                let TypeRef::Named(target) = &args[0] else {
                    return Err(Error::new(format!(
                        "enum {} variant {} payload type {} must target a named process",
                        enum_decl.name, variant.name, payload_type
                    )));
                };
                let target_symbol = context.symbols.resolve(target.as_str()).ok_or_else(|| {
                    Error::new(format!(
                        "enum {} variant {} payload type {} targets undeclared process {}",
                        enum_decl.name, variant.name, payload_type, target
                    ))
                })?;
                if context.processes.contains_key(&target_symbol) {
                    return Ok(());
                }
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} targets undeclared process {}",
                    enum_decl.name, variant.name, payload_type, target
                )));
            }
            if constructor_symbol == context.process_ref_type {
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} must declare exactly one target process",
                    enum_decl.name, variant.name, payload_type
                )));
            }
            if let Err(err) = validate_source_value_type(
                context.symbols,
                context.types,
                context.process_ref_type,
                context.list_type,
                context.map_type,
                payload_type,
            ) {
                if collection_type_signature_error(
                    context.symbols,
                    context.list_type,
                    context.map_type,
                    payload_type,
                ) {
                    return Err(err);
                }
                if type_contains_process_ref(
                    context.symbols,
                    context.process_ref_type,
                    payload_type,
                ) {
                    return Err(Error::new(format!(
                        "enum {} variant {} payload type {} contains a process reference; process references must be direct message payloads",
                        enum_decl.name, variant.name, payload_type
                    )));
                }
            } else {
                return Ok(());
            }
            return Err(Error::new(format!(
                "enum {} variant {} payload type {} must be a named record, enum, list, map, or process reference type",
                enum_decl.name, variant.name, payload_type
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_source_value_type(
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
    process_ref_type: Symbol,
    list_type: Symbol,
    map_type: Symbol,
    ty: &TypeRef,
) -> Result<()> {
    match ty {
        TypeRef::Named(name) => {
            if is_process_ref_name(symbols, process_ref_type, name) {
                return Err(Error::new(format!(
                    "type {ty} is not a source value type; process references must be direct message payloads"
                )));
            }
            type_decl_from_tables(symbols, types, ty)?;
            Ok(())
        }
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let constructor_symbol = symbols
                .resolve(constructor.as_str())
                .ok_or_else(|| Error::new(format!("type {ty} is not declared")))?;
            if constructor_symbol == process_ref_type {
                return Err(Error::new(format!(
                    "type {ty} is not a source value type; process references must be direct message payloads"
                )));
            }
            if constructor_symbol == list_type && args.len() == 1 && const_args.len() == 1 {
                validate_collection_capacity(ty, const_args[0])?;
                return validate_source_value_type(
                    symbols,
                    types,
                    process_ref_type,
                    list_type,
                    map_type,
                    &args[0],
                );
            }
            if constructor_symbol == list_type {
                return Err(Error::new(format!(
                    "list type {ty} must declare exactly one element type and one numeric capacity"
                )));
            }
            if constructor_symbol == map_type && args.len() == 2 && const_args.len() == 1 {
                validate_collection_capacity(ty, const_args[0])?;
                validate_source_value_type(
                    symbols,
                    types,
                    process_ref_type,
                    list_type,
                    map_type,
                    &args[0],
                )?;
                return validate_source_value_type(
                    symbols,
                    types,
                    process_ref_type,
                    list_type,
                    map_type,
                    &args[1],
                );
            }
            if constructor_symbol == map_type {
                return Err(Error::new(format!(
                    "map type {ty} must declare exactly two type arguments and one numeric capacity"
                )));
            }
            Err(Error::new(format!("type {ty} is not declared")))
        }
    }
}

fn type_contains_process_ref(
    symbols: &SymbolTable,
    process_ref_type: Symbol,
    ty: &TypeRef,
) -> bool {
    match ty {
        TypeRef::Named(name) => is_process_ref_name(symbols, process_ref_type, name),
        TypeRef::Applied {
            constructor, args, ..
        } => {
            symbols
                .resolve(constructor.as_str())
                .is_some_and(|symbol| symbol == process_ref_type)
                || args
                    .iter()
                    .any(|arg| type_contains_process_ref(symbols, process_ref_type, arg))
        }
    }
}

fn is_process_ref_name(symbols: &SymbolTable, process_ref_type: Symbol, name: &Identifier) -> bool {
    symbols
        .resolve(name.as_str())
        .is_some_and(|symbol| symbol == process_ref_type)
}

fn collection_type_signature_error(
    symbols: &SymbolTable,
    list_type: Symbol,
    map_type: Symbol,
    ty: &TypeRef,
) -> bool {
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = ty
    else {
        return false;
    };
    let Some(constructor_symbol) = symbols.resolve(constructor.as_str()) else {
        return false;
    };
    if constructor_symbol == list_type {
        return args.len() != 1
            || const_args.len() != 1
            || const_args[0] > MAX_VALUE_TEMPLATE_FIELDS;
    }
    if constructor_symbol == map_type {
        return args.len() != 2
            || const_args.len() != 1
            || const_args[0] > MAX_VALUE_TEMPLATE_FIELDS;
    }
    false
}

fn type_decl_from_tables(
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
    ty: &TypeRef,
) -> Result<TypeDecl> {
    let Some(name) = ty.as_named() else {
        return Err(Error::new(format!("type {ty} is not declared")));
    };
    let symbol = symbols
        .resolve(name)
        .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
    types
        .get(&symbol)
        .copied()
        .ok_or_else(|| Error::new(format!("type {name} is not declared")))
}

pub(super) fn validate_collection_capacity(ty: &TypeRef, capacity: usize) -> Result<()> {
    if capacity > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "collection type {ty} capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    Ok(())
}
