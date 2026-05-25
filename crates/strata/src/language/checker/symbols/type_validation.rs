use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::MAX_VALUE_TEMPLATE_FIELDS;

use super::super::super::ast::{Enum, EnumVariant, Identifier, Module, Record, TypeRef};
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
    module: &Module,
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
            module,
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
                "record {} field {} type {} is not a source value type: {err}",
                record.name, field.name, field.ty
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MessagePayloadTypeContext<'a> {
    pub(super) module: &'a Module,
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
                context.module,
                context.symbols,
                context.types,
                context.process_ref_type,
                context.list_type,
                context.map_type,
                payload_type,
            )
            .map_err(|err| {
                Error::new(format!(
                    "enum {} variant {} payload type {} is invalid: {err}",
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
                context.module,
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
    module: &Module,
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
    process_ref_type: Symbol,
    list_type: Symbol,
    map_type: Symbol,
    ty: &TypeRef,
) -> Result<()> {
    let mut validator = SourceValueTypeValidator {
        module,
        symbols,
        types,
        process_ref_type,
        list_type,
        map_type,
        visiting: BTreeSet::new(),
    };
    validator.validate(ty)
}

struct SourceValueTypeValidator<'a> {
    module: &'a Module,
    symbols: &'a SymbolTable,
    types: &'a BTreeMap<Symbol, TypeDecl>,
    process_ref_type: Symbol,
    list_type: Symbol,
    map_type: Symbol,
    visiting: BTreeSet<Symbol>,
}

impl SourceValueTypeValidator<'_> {
    fn validate(&mut self, ty: &TypeRef) -> Result<()> {
        match ty {
            TypeRef::Named(name) => self.validate_named(ty, name),
            TypeRef::Applied {
                constructor,
                args,
                const_args,
            } => self.validate_applied(ty, constructor, args, const_args),
        }
    }

    fn validate_named(&mut self, ty: &TypeRef, name: &Identifier) -> Result<()> {
        if is_process_ref_name(self.symbols, self.process_ref_type, name) {
            return Err(Error::new(format!(
                "type {ty} is not a source value type; process references must be direct message payloads"
            )));
        }
        let symbol = self
            .symbols
            .resolve(name.as_str())
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
        let decl = self
            .types
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
        if !self.visiting.insert(symbol) {
            return Ok(());
        }
        let result = match decl {
            TypeDecl::Record(index) => self.validate_record(ty, index),
            TypeDecl::Enum(index) => self.validate_enum(ty, index),
        };
        self.visiting.remove(&symbol);
        result
    }

    fn validate_applied(
        &mut self,
        ty: &TypeRef,
        constructor: &Identifier,
        args: &[TypeRef],
        const_args: &[usize],
    ) -> Result<()> {
        let constructor_symbol = self
            .symbols
            .resolve(constructor.as_str())
            .ok_or_else(|| Error::new(format!("type {ty} is not declared")))?;
        if constructor_symbol == self.process_ref_type {
            return Err(Error::new(format!(
                "type {ty} is not a source value type; process references must be direct message payloads"
            )));
        }
        if constructor_symbol == self.list_type && args.len() == 1 && const_args.len() == 1 {
            validate_collection_capacity(ty, const_args[0])?;
            return self.validate(&args[0]);
        }
        if constructor_symbol == self.list_type {
            return Err(Error::new(format!(
                "list type {ty} must declare exactly one element type and one numeric capacity"
            )));
        }
        if constructor_symbol == self.map_type && args.len() == 2 && const_args.len() == 1 {
            validate_collection_capacity(ty, const_args[0])?;
            self.validate(&args[0])?;
            return self.validate(&args[1]);
        }
        if constructor_symbol == self.map_type {
            return Err(Error::new(format!(
                "map type {ty} must declare exactly two type arguments and one numeric capacity"
            )));
        }
        Err(Error::new(format!("type {ty} is not declared")))
    }

    fn validate_record(&mut self, ty: &TypeRef, index: usize) -> Result<()> {
        let record = self.module.records.get(index).ok_or_else(|| {
            Error::new(format!(
                "record index {index} is not declared for type {ty}"
            ))
        })?;
        for field in &record.fields {
            if type_contains_process_ref(self.symbols, self.process_ref_type, &field.ty) {
                return Err(Error::new(format!(
                    "record {} field {} type {} contains a process reference; process references must be direct message payloads",
                    record.name, field.name, field.ty
                )));
            }
            self.validate(&field.ty)?;
        }
        Ok(())
    }

    fn validate_enum(&mut self, ty: &TypeRef, index: usize) -> Result<()> {
        let enum_decl = self.module.enums.get(index).ok_or_else(|| {
            Error::new(format!("enum index {index} is not declared for type {ty}"))
        })?;
        for variant in &enum_decl.variants {
            let Some(payload_type) = &variant.payload_type else {
                continue;
            };
            self.validate(payload_type)?;
        }
        Ok(())
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

pub(super) fn validate_collection_capacity(ty: &TypeRef, capacity: usize) -> Result<()> {
    if capacity > MAX_VALUE_TEMPLATE_FIELDS {
        return Err(Error::new(format!(
            "collection type {ty} capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
        )));
    }
    Ok(())
}
