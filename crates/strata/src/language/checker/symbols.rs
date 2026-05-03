use std::collections::BTreeMap;

use super::super::ast::{Enum, Identifier, Module, Record, TypeRef};
use super::super::checked::{CheckedMessageId, CheckedProcessId};
use super::super::diagnostic::{Error, Result};
use super::super::PROC_RESULT_TYPE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Symbol(u32);

impl Symbol {
    fn from_index(index: usize) -> Result<Self> {
        let value = u32::try_from(index)
            .map_err(|_| Error::new(format!("symbol index {index} is too large")))?;
        Ok(Self(value))
    }
}

#[derive(Debug, Default)]
struct SymbolTable {
    by_text: BTreeMap<String, Symbol>,
}

impl SymbolTable {
    fn intern(&mut self, value: &Identifier) -> Result<Symbol> {
        let value = value.as_str();
        if let Some(symbol) = self.by_text.get(value) {
            return Ok(*symbol);
        }
        let symbol = Symbol::from_index(self.by_text.len())?;
        self.by_text.insert(value.to_string(), symbol);
        Ok(symbol)
    }

    fn resolve(&self, value: &str) -> Option<Symbol> {
        self.by_text.get(value).copied()
    }
}

#[derive(Debug, Clone, Copy)]
enum TypeDecl {
    Record(usize),
    Enum(usize),
}

impl TypeDecl {
    fn kind(self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
        }
    }
}

fn reject_reserved_type_name(name: &str, symbol: Symbol, reserved: Symbol) -> Result<()> {
    if symbol == reserved {
        return Err(Error::new(format!("type name {name} is reserved")));
    }
    Ok(())
}

fn validate_record_fields(
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
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
        type_decl_from_tables(symbols, types, &field.ty).map_err(|_| {
            Error::new(format!(
                "record {} field {} uses undeclared type {}",
                record.name, field.name, field.ty
            ))
        })?;
    }
    Ok(())
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

#[derive(Debug)]
pub(super) struct SemanticIndex {
    symbols: SymbolTable,
    proc_result_type: Symbol,
    types: BTreeMap<Symbol, TypeDecl>,
    processes: BTreeMap<Symbol, CheckedProcessId>,
    enum_variants: Vec<BTreeMap<Symbol, usize>>,
}

impl SemanticIndex {
    pub(super) fn build(module: &Module) -> Result<Self> {
        let mut symbols = SymbolTable::default();
        let mut types = BTreeMap::new();
        let mut records = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut enum_variants = Vec::with_capacity(module.enums.len());
        let mut processes = BTreeMap::new();

        let _module_symbol = symbols.intern(&module.name)?;
        let proc_result_type = symbols.intern(&Identifier::new(PROC_RESULT_TYPE)?)?;

        for (index, record) in module.records.iter().enumerate() {
            let symbol = symbols.intern(&record.name)?;
            reject_reserved_type_name(record.name.as_str(), symbol, proc_result_type)?;
            if records.insert(symbol, index).is_some() {
                return Err(Error::new(format!(
                    "duplicate record declaration {}",
                    record.name
                )));
            }
            if let Some(previous) = types.insert(symbol, TypeDecl::Record(index)) {
                return Err(Error::new(format!(
                    "duplicate type declaration {} used by {} and record",
                    record.name,
                    previous.kind()
                )));
            }
            for field in &record.fields {
                symbols.intern(&field.name)?;
            }
        }

        for (index, item) in module.enums.iter().enumerate() {
            let symbol = symbols.intern(&item.name)?;
            reject_reserved_type_name(item.name.as_str(), symbol, proc_result_type)?;
            if enums.insert(symbol, index).is_some() {
                return Err(Error::new(format!(
                    "duplicate enum declaration {}",
                    item.name
                )));
            }
            if let Some(previous) = types.insert(symbol, TypeDecl::Enum(index)) {
                return Err(Error::new(format!(
                    "duplicate type declaration {} used by {} and enum",
                    item.name,
                    previous.kind()
                )));
            }

            let mut variants = BTreeMap::new();
            for (variant_index, variant) in item.variants.iter().enumerate() {
                let variant_symbol = symbols.intern(variant)?;
                if variants.insert(variant_symbol, variant_index).is_some() {
                    return Err(Error::new(format!(
                        "duplicate variant in enum {} declaration {}",
                        item.name, variant
                    )));
                }
            }
            enum_variants.push(variants);
        }

        for (index, process) in module.processes.iter().enumerate() {
            let symbol = symbols.intern(&process.name)?;
            if processes
                .insert(symbol, CheckedProcessId::from_index(index)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate process declaration {}",
                    process.name
                )));
            }
        }

        for record in &module.records {
            validate_record_fields(&symbols, &types, record)?;
        }

        Ok(Self {
            symbols,
            proc_result_type,
            types,
            processes,
            enum_variants,
        })
    }

    pub(super) fn process_id(&self, name: &Identifier) -> Result<CheckedProcessId> {
        self.process_id_by_name(name.as_str())
    }

    pub(super) fn process_id_by_name(&self, name: &str) -> Result<CheckedProcessId> {
        let symbol = self
            .symbols
            .resolve(name)
            .ok_or_else(|| Error::new(format!("process {name} is not declared")))?;
        self.processes
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("process {name} is not declared")))
    }

    fn same_identifier(&self, left: &Identifier, right: &Identifier) -> bool {
        self.symbols
            .resolve(left.as_str())
            .zip(self.symbols.resolve(right.as_str()))
            .is_some_and(|(left_symbol, right_symbol)| left_symbol == right_symbol)
    }

    pub(super) fn same_type(&self, left: &TypeRef, right: &TypeRef) -> bool {
        match (left, right) {
            (TypeRef::Named(left), TypeRef::Named(right)) => self.same_identifier(left, right),
            (
                TypeRef::Applied {
                    constructor: left_constructor,
                    args: left_args,
                },
                TypeRef::Applied {
                    constructor: right_constructor,
                    args: right_args,
                },
            ) => {
                left_args.len() == right_args.len()
                    && self.same_identifier(left_constructor, right_constructor)
                    && left_args
                        .iter()
                        .zip(right_args)
                        .all(|(left_arg, right_arg)| self.same_type(left_arg, right_arg))
            }
            _ => false,
        }
    }

    pub(super) fn is_proc_result_of(&self, ty: &TypeRef, state_type: &TypeRef) -> bool {
        let TypeRef::Applied { constructor, args } = ty else {
            return false;
        };
        let Some(constructor_symbol) = self.symbols.resolve(constructor.as_str()) else {
            return false;
        };
        args.len() == 1
            && constructor_symbol == self.proc_result_type
            && self.same_type(&args[0], state_type)
    }

    fn type_decl(&self, ty: &TypeRef) -> Result<TypeDecl> {
        let Some(name) = ty.as_named() else {
            return Err(Error::new(format!("type {ty} is not declared")));
        };
        let symbol = self
            .symbols
            .resolve(name)
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
        self.types
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))
    }

    pub(super) fn enum_decl<'a>(&self, module: &'a Module, ty: &TypeRef) -> Result<&'a Enum> {
        match self.type_decl(ty)? {
            TypeDecl::Enum(index) => Ok(&module.enums[index]),
            TypeDecl::Record(_) => Err(Error::new(format!("type {ty} is not declared as an enum"))),
        }
    }

    pub(super) fn record_decl<'a>(&self, module: &'a Module, ty: &TypeRef) -> Result<&'a Record> {
        match self.type_decl(ty)? {
            TypeDecl::Record(index) => Ok(&module.records[index]),
            TypeDecl::Enum(_) => Err(Error::new(format!("type {ty} is not declared as a record"))),
        }
    }

    pub(super) fn message_id_for_process(
        &self,
        module: &Module,
        sender_process: &str,
        process_id: CheckedProcessId,
        message: &Identifier,
    ) -> Result<CheckedMessageId> {
        let process = module.processes.get(process_id.index()).ok_or_else(|| {
            Error::new(format!(
                "process id {} is not declared",
                process_id.as_u32()
            ))
        })?;
        let msg_enum = self.enum_decl(module, &process.msg_type)?;
        let message_symbol = self.symbols.resolve(message.as_str()).ok_or_else(|| {
            Error::new(format!(
                "process {} sends message {} not accepted by {}",
                sender_process, message, process.name
            ))
        })?;
        let enum_index = match self.type_decl(&process.msg_type)? {
            TypeDecl::Enum(index) => index,
            TypeDecl::Record(_) => {
                return Err(Error::new(format!(
                    "type {} is not declared as an enum",
                    msg_enum.name
                )))
            }
        };
        self.enum_variants[enum_index]
            .get(&message_symbol)
            .copied()
            .map(CheckedMessageId::from_index)
            .transpose()?
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} sends message {} not accepted by {}",
                    sender_process, message, process.name
                ))
            })
    }
}
