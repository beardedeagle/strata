use std::collections::BTreeMap;

use super::super::ast::{Enum, EnumVariant, Identifier, Module, Process, Record, TypeRef};
use super::super::checked::{CheckedMessageVariantId, CheckedProcessId};
use super::super::diagnostic::{Error, Result};
use super::super::{PROC_RESULT_TYPE, PROCESS_REF_TYPE};
use super::CHECKED_TYPE_LABEL_PREFIX;

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

#[derive(Debug, Clone, Copy)]
enum MessageResolutionContext<'a> {
    Send { sender_process: &'a str },
    StepPattern,
}

impl MessageResolutionContext<'_> {
    fn not_accepted_error(self, process: &Process, message: &Identifier) -> Error {
        match self {
            Self::Send { sender_process } => Error::new(format!(
                "process {} sends message {} not accepted by {}",
                sender_process, message, process.name
            )),
            Self::StepPattern => Error::new(format!(
                "process {} step pattern message {} is not accepted",
                process.name, message
            )),
        }
    }
}

fn reject_reserved_type_name(name: &str, symbol: Symbol, reserved: Symbol) -> Result<()> {
    if symbol == reserved {
        return Err(Error::new(format!("type name {name} is reserved")));
    }
    Ok(())
}

fn reject_internal_type_label_prefix(name: &str) -> Result<()> {
    if name.starts_with(CHECKED_TYPE_LABEL_PREFIX) {
        return Err(Error::new(format!(
            "type name {name} uses reserved prefix {CHECKED_TYPE_LABEL_PREFIX}"
        )));
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

fn validate_message_payload_type(
    symbols: &SymbolTable,
    types: &BTreeMap<Symbol, TypeDecl>,
    processes: &BTreeMap<Symbol, CheckedProcessId>,
    process_ref_type: Symbol,
    enum_decl: &Enum,
    variant: &EnumVariant,
    payload_type: &TypeRef,
) -> Result<()> {
    match payload_type {
        TypeRef::Named(_) => {
            type_decl_from_tables(symbols, types, payload_type).map_err(|_| {
                Error::new(format!(
                    "enum {} variant {} uses undeclared payload type {}",
                    enum_decl.name, variant.name, payload_type
                ))
            })?;
        }
        TypeRef::Applied { constructor, args } => {
            let Some(constructor_symbol) = symbols.resolve(constructor.as_str()) else {
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} must be a named record, enum, or process reference type",
                    enum_decl.name, variant.name, payload_type
                )));
            };
            if constructor_symbol == process_ref_type
                && args.len() == 1
                && matches!(&args[0], TypeRef::Named(_))
            {
                let TypeRef::Named(target) = &args[0] else {
                    unreachable!("matches! above proves named target");
                };
                let target_symbol = symbols.resolve(target.as_str()).ok_or_else(|| {
                    Error::new(format!(
                        "enum {} variant {} payload type {} targets undeclared process {}",
                        enum_decl.name, variant.name, payload_type, target
                    ))
                })?;
                if processes.contains_key(&target_symbol) {
                    return Ok(());
                }
                return Err(Error::new(format!(
                    "enum {} variant {} payload type {} targets undeclared process {}",
                    enum_decl.name, variant.name, payload_type, target
                )));
            }
            return Err(Error::new(format!(
                "enum {} variant {} payload type {} must be a named record, enum, or process reference type",
                enum_decl.name, variant.name, payload_type
            )));
        }
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
    process_ref_type: Symbol,
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
        let process_ref_type = symbols.intern(&Identifier::new(PROCESS_REF_TYPE)?)?;

        for (index, record) in module.records.iter().enumerate() {
            let symbol = symbols.intern(&record.name)?;
            reject_reserved_type_name(record.name.as_str(), symbol, proc_result_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, process_ref_type)?;
            reject_internal_type_label_prefix(record.name.as_str())?;
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
            reject_reserved_type_name(item.name.as_str(), symbol, process_ref_type)?;
            reject_internal_type_label_prefix(item.name.as_str())?;
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
                let variant_symbol = symbols.intern(&variant.name)?;
                if variants.insert(variant_symbol, variant_index).is_some() {
                    return Err(Error::new(format!(
                        "duplicate variant in enum {} declaration {}",
                        item.name, variant.name
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

        for item in &module.enums {
            for variant in &item.variants {
                if let Some(payload_type) = &variant.payload_type {
                    validate_message_payload_type(
                        &symbols,
                        &types,
                        &processes,
                        process_ref_type,
                        item,
                        variant,
                        payload_type,
                    )?;
                }
            }
        }

        for record in &module.records {
            validate_record_fields(&symbols, &types, record)?;
        }

        Ok(Self {
            symbols,
            proc_result_type,
            process_ref_type,
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

    pub(super) fn process_ref_target_type(&self, ty: &TypeRef) -> Result<Option<CheckedProcessId>> {
        let TypeRef::Applied { constructor, args } = ty else {
            return Ok(None);
        };
        let Some(constructor_symbol) = self.symbols.resolve(constructor.as_str()) else {
            return Ok(None);
        };
        if constructor_symbol != self.process_ref_type {
            return Ok(None);
        }
        if args.len() != 1 {
            return Err(Error::new(format!(
                "process reference type {ty} must declare exactly one target process"
            )));
        }
        let TypeRef::Named(target) = &args[0] else {
            return Err(Error::new(format!(
                "process reference type {ty} must target a declared process"
            )));
        };
        self.process_id(target).map(Some)
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
    ) -> Result<CheckedMessageVariantId> {
        self.message_id_for_process_with_context(
            module,
            process_id,
            message,
            MessageResolutionContext::Send { sender_process },
        )
    }

    pub(super) fn message_id_for_step_pattern(
        &self,
        module: &Module,
        process_id: CheckedProcessId,
        message: &Identifier,
    ) -> Result<CheckedMessageVariantId> {
        self.message_id_for_process_with_context(
            module,
            process_id,
            message,
            MessageResolutionContext::StepPattern,
        )
    }

    fn message_id_for_process_with_context(
        &self,
        module: &Module,
        process_id: CheckedProcessId,
        message: &Identifier,
        context: MessageResolutionContext<'_>,
    ) -> Result<CheckedMessageVariantId> {
        let process = module.processes.get(process_id.index()).ok_or_else(|| {
            Error::new(format!(
                "process id {} is not declared",
                process_id.as_u32()
            ))
        })?;
        let message_symbol = self
            .symbols
            .resolve(message.as_str())
            .ok_or_else(|| context.not_accepted_error(process, message))?;
        let enum_index = match self.type_decl(&process.msg_type)? {
            TypeDecl::Enum(index) => index,
            TypeDecl::Record(_) => {
                return Err(Error::new(format!(
                    "type {} is not declared as an enum",
                    process.msg_type
                )));
            }
        };
        self.enum_variants
            .get(enum_index)
            .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?
            .get(&message_symbol)
            .copied()
            .map(CheckedMessageVariantId::from_index)
            .transpose()?
            .ok_or_else(|| context.not_accepted_error(process, message))
    }

    pub(super) fn message_variant<'a>(
        &self,
        module: &'a Module,
        process_id: CheckedProcessId,
        message: CheckedMessageVariantId,
    ) -> Result<&'a EnumVariant> {
        let process = module.processes.get(process_id.index()).ok_or_else(|| {
            Error::new(format!(
                "process id {} is not declared",
                process_id.as_u32()
            ))
        })?;
        let enum_decl = self.enum_decl(module, &process.msg_type)?;
        enum_decl.variants.get(message.index()).ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} is not accepted",
                process.name,
                message.as_u32()
            ))
        })
    }

    pub(super) fn identifier_conflicts_with_declared_value(&self, name: &Identifier) -> bool {
        let Some(symbol) = self.symbols.resolve(name.as_str()) else {
            return false;
        };
        self.types.contains_key(&symbol)
            || self
                .enum_variants
                .iter()
                .any(|variants| variants.contains_key(&symbol))
    }
}
