use std::collections::BTreeMap;

use mantle_artifact::{ArtifactPrimitiveType, ArtifactScalarType};

use super::super::ast::{Enum, EnumVariant, Identifier, Module, Process, Record, TypeRef};
use super::super::checked::{
    CheckedComponentId, CheckedMessageVariantId, CheckedPortId, CheckedProcessId, CheckedProtocolId,
};
use super::super::diagnostic::{Error, Result};
use super::super::{BOOL_FALSE, BOOL_TRUE, BOOL_TYPE};
mod boundaries;
mod build;
mod builtins;
mod collection_type;
mod type_decls;
mod type_validation;

use builtins::is_builtin_value_constructor_name;
pub(super) use builtins::{BuiltinValueShape, ValueEnumVariant, ValueEnumVariantInfo};
pub(super) use collection_type::CollectionType;
use type_decls::{TypeDecl, TypeDeclMap};
use type_validation::{
    SourceValueTypeContext, validate_collection_capacity, validate_source_value_type,
};

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
        self.intern_str(value.as_str())
    }

    fn intern_str(&mut self, value: &str) -> Result<Symbol> {
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

#[derive(Debug)]
pub(super) struct SemanticIndex {
    symbols: SymbolTable,
    proc_result_type: Symbol,
    process_ref_type: Symbol,
    list_type: Symbol,
    map_type: Symbol,
    unit_type: Symbol,
    option_type: Symbol,
    result_type: Symbol,
    send_error_type: Symbol,
    spawn_error_type: Symbol,
    types: TypeDeclMap,
    processes: BTreeMap<Symbol, CheckedProcessId>,
    protocols: BTreeMap<Symbol, CheckedProtocolId>,
    ports: BTreeMap<Symbol, CheckedPortId>,
    components: BTreeMap<Symbol, CheckedComponentId>,
    port_contracts: Vec<PortContract>,
    component_contracts: Vec<ComponentContract>,
    enum_variants: Vec<BTreeMap<Symbol, usize>>,
}

#[derive(Debug, Clone)]
pub(super) struct PortContract {
    pub(super) protocol: CheckedProtocolId,
    pub(super) target_process: CheckedProcessId,
    pub(super) message_type: TypeRef,
}

#[derive(Debug, Clone)]
pub(super) struct ComponentContract {
    pub(super) export_port: CheckedPortId,
    pub(super) import_ports: Vec<CheckedPortId>,
}

impl SemanticIndex {
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

    pub(super) fn protocol_id(&self, name: &Identifier) -> Result<CheckedProtocolId> {
        let symbol = self
            .symbols
            .resolve(name.as_str())
            .ok_or_else(|| Error::new(format!("protocol {name} is not declared")))?;
        self.protocols
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("protocol {name} is not declared")))
    }

    pub(super) fn port_id(&self, name: &Identifier) -> Result<CheckedPortId> {
        let symbol = self
            .symbols
            .resolve(name.as_str())
            .ok_or_else(|| Error::new(format!("port {name} is not declared")))?;
        self.ports
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("port {name} is not declared")))
    }

    pub(super) fn component_id(&self, name: &Identifier) -> Result<CheckedComponentId> {
        let symbol = self
            .symbols
            .resolve(name.as_str())
            .ok_or_else(|| Error::new(format!("component {name} is not declared")))?;
        self.components
            .get(&symbol)
            .copied()
            .ok_or_else(|| Error::new(format!("component {name} is not declared")))
    }

    pub(super) fn port_contract(&self, port: CheckedPortId) -> Result<&PortContract> {
        self.port_contracts
            .get(port.index())
            .ok_or_else(|| Error::new(format!("port id {} is not declared", port.as_u32())))
    }

    pub(super) fn component_contract(
        &self,
        component: CheckedComponentId,
    ) -> Result<&ComponentContract> {
        self.component_contracts
            .get(component.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "component id {} is not declared",
                    component.as_u32()
                ))
            })
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
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return false;
        };
        let Some(constructor_symbol) = self.symbols.resolve(constructor.as_str()) else {
            return false;
        };
        args.len() == 1
            && const_args.is_empty()
            && constructor_symbol == self.proc_result_type
            && self.same_type(&args[0], state_type)
    }

    pub(super) fn process_ref_target_type(&self, ty: &TypeRef) -> Result<Option<CheckedProcessId>> {
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return Ok(None);
        };
        let Some(constructor_symbol) = self.symbols.resolve(constructor.as_str()) else {
            return Ok(None);
        };
        if constructor_symbol != self.process_ref_type {
            return Ok(None);
        }
        if args.len() != 1 || !const_args.is_empty() {
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

    pub(super) fn collection_type<'a>(
        &self,
        ty: &'a TypeRef,
    ) -> Result<Option<CollectionType<'a>>> {
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return Ok(None);
        };
        let Some(constructor_symbol) = self.symbols.resolve(constructor.as_str()) else {
            return Ok(None);
        };
        if constructor_symbol == self.list_type {
            if args.len() != 1 || const_args.len() != 1 {
                return Err(Error::new(format!(
                    "list type {ty} must declare exactly one element type and one numeric capacity"
                )));
            }
            validate_collection_capacity(ty, const_args[0])?;
            return Ok(Some(CollectionType::List {
                element: &args[0],
                capacity: const_args[0],
            }));
        }
        if constructor_symbol == self.map_type {
            if args.len() != 2 || const_args.len() != 1 {
                return Err(Error::new(format!(
                    "map type {ty} must declare exactly two type arguments and one numeric capacity"
                )));
            }
            validate_collection_capacity(ty, const_args[0])?;
            return Ok(Some(CollectionType::Map {
                key: &args[0],
                value: &args[1],
                capacity: const_args[0],
            }));
        }
        Ok(None)
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
            .get(symbol)
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))
    }

    pub(super) fn enum_decl<'a>(&self, module: &'a Module, ty: &TypeRef) -> Result<&'a Enum> {
        match self.type_decl(ty)? {
            TypeDecl::Enum(index) => Ok(&module.enums[index]),
            TypeDecl::Record(_) | TypeDecl::Primitive(_) | TypeDecl::Scalar(_) | TypeDecl::Unit => {
                Err(Error::new(format!("type {ty} is not declared as an enum")))
            }
        }
    }

    pub(super) fn record_decl<'a>(&self, module: &'a Module, ty: &TypeRef) -> Result<&'a Record> {
        match self.type_decl(ty)? {
            TypeDecl::Record(index) => Ok(&module.records[index]),
            TypeDecl::Enum(_) | TypeDecl::Primitive(_) | TypeDecl::Scalar(_) | TypeDecl::Unit => {
                Err(Error::new(format!("type {ty} is not declared as a record")))
            }
        }
    }

    pub(super) fn primitive_type(&self, ty: &TypeRef) -> Result<Option<ArtifactPrimitiveType>> {
        if ty.as_named().is_none() {
            return Ok(None);
        }
        match self.type_decl(ty) {
            Ok(TypeDecl::Primitive(primitive)) => Ok(Some(primitive)),
            Ok(TypeDecl::Unit | TypeDecl::Scalar(_) | TypeDecl::Record(_) | TypeDecl::Enum(_)) => {
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn scalar_type(&self, ty: &TypeRef) -> Result<Option<ArtifactScalarType>> {
        if ty.as_named().is_none() {
            return Ok(None);
        }
        match self.type_decl(ty) {
            Ok(TypeDecl::Scalar(scalar)) => Ok(Some(scalar)),
            Ok(
                TypeDecl::Unit | TypeDecl::Primitive(_) | TypeDecl::Record(_) | TypeDecl::Enum(_),
            ) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub(super) fn validate_source_value_type(&self, module: &Module, ty: &TypeRef) -> Result<()> {
        validate_source_value_type(
            SourceValueTypeContext {
                module,
                symbols: &self.symbols,
                types: &self.types,
                process_ref_type: self.process_ref_type,
                list_type: self.list_type,
                map_type: self.map_type,
                builtin_types: self.builtin_type_symbols(),
            },
            ty,
        )
    }

    pub(super) fn bool_type(&self, module: &Module) -> Result<TypeRef> {
        let symbol = self
            .symbols
            .resolve(BOOL_TYPE)
            .ok_or_else(bool_contract_error)?;
        let Some(TypeDecl::Enum(index)) = self.types.get(symbol) else {
            return Err(bool_contract_error());
        };
        let enum_decl = module
            .enums
            .get(index)
            .ok_or_else(|| Error::new(format!("enum index {index} is not declared")))?;
        let [false_variant, true_variant] = enum_decl.variants.as_slice() else {
            return Err(bool_contract_error());
        };
        if false_variant.name.as_str() != BOOL_FALSE
            || false_variant.payload_type.is_some()
            || true_variant.name.as_str() != BOOL_TRUE
            || true_variant.payload_type.is_some()
        {
            return Err(bool_contract_error());
        }
        Ok(TypeRef::Named(enum_decl.name.clone()))
    }

    pub(super) fn enum_variant_index(
        &self,
        module: &Module,
        ty: &TypeRef,
        variant: &Identifier,
    ) -> Result<usize> {
        if let Some(index) = self.builtin_value_enum_variant_index(ty, variant)? {
            return Ok(index);
        }
        let enum_index = match self.type_decl(ty)? {
            TypeDecl::Enum(index) => index,
            TypeDecl::Record(_) | TypeDecl::Primitive(_) | TypeDecl::Scalar(_) | TypeDecl::Unit => {
                return Err(Error::new(format!("type {ty} is not declared as an enum")));
            }
        };
        let enum_decl = module
            .enums
            .get(enum_index)
            .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?;
        let variant_symbol = self.symbols.resolve(variant.as_str()).ok_or_else(|| {
            Error::new(format!(
                "match pattern {variant} is not a variant of enum {}",
                enum_decl.name
            ))
        })?;
        self.enum_variants
            .get(enum_index)
            .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?
            .get(&variant_symbol)
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "match pattern {variant} is not a variant of enum {}",
                    enum_decl.name
                ))
            })
    }

    pub(super) fn fieldless_enum_variant_type(
        &self,
        module: &Module,
        value: &Identifier,
    ) -> Result<TypeRef> {
        self.fieldless_enum_variant_type_with_context(module, value, "match scrutinee")
    }

    pub(super) fn equality_fieldless_enum_variant_type(
        &self,
        module: &Module,
        value: &Identifier,
    ) -> Result<TypeRef> {
        self.fieldless_enum_variant_type_with_context(module, value, "equality operand")
    }

    fn fieldless_enum_variant_type_with_context(
        &self,
        module: &Module,
        value: &Identifier,
        context: &str,
    ) -> Result<TypeRef> {
        let value_symbol = self.symbols.resolve(value.as_str()).ok_or_else(|| {
            Error::new(format!("{context} {value} is not a fieldless enum variant"))
        })?;
        let mut matches = Vec::new();
        for (enum_index, variants) in self.enum_variants.iter().enumerate() {
            let Some(variant_index) = variants.get(&value_symbol) else {
                continue;
            };
            let enum_decl = module
                .enums
                .get(enum_index)
                .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?;
            let variant = enum_decl.variants.get(*variant_index).ok_or_else(|| {
                Error::new(format!(
                    "enum {} variant index {variant_index} is not declared",
                    enum_decl.name
                ))
            })?;
            matches.push((enum_decl.name.clone(), variant.payload_type.is_some()));
        }

        match matches.as_slice() {
            [(name, false)] => Ok(TypeRef::Named(name.clone())),
            [(_, true)] => Err(Error::new(format!(
                "{context} {value} must be a fieldless enum variant"
            ))),
            [] => Err(Error::new(format!(
                "{context} {value} is not a fieldless enum variant"
            ))),
            _ => Err(Error::new(format!(
                "{context} {value} is ambiguous across enum declarations"
            ))),
        }
    }

    pub(super) fn enum_variant_type(&self, module: &Module, value: &Identifier) -> Result<TypeRef> {
        let value_symbol = self
            .symbols
            .resolve(value.as_str())
            .ok_or_else(|| Error::new(format!("pattern {value} is not a declared enum variant")))?;
        let mut matches = Vec::new();
        for (enum_index, variants) in self.enum_variants.iter().enumerate() {
            if variants.contains_key(&value_symbol) {
                let enum_decl = module.enums.get(enum_index).ok_or_else(|| {
                    Error::new(format!("enum index {enum_index} is not declared"))
                })?;
                matches.push(enum_decl.name.clone());
            }
        }

        match matches.as_slice() {
            [name] => Ok(TypeRef::Named(name.clone())),
            [] => Err(Error::new(format!(
                "pattern {value} is not a declared enum variant"
            ))),
            _ => Err(Error::new(format!(
                "pattern {value} is ambiguous across enum declarations"
            ))),
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
            TypeDecl::Record(_) | TypeDecl::Primitive(_) | TypeDecl::Scalar(_) | TypeDecl::Unit => {
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
        if is_builtin_value_constructor_name(name.as_str()) {
            return true;
        }
        let Some(symbol) = self.symbols.resolve(name.as_str()) else {
            return false;
        };
        if symbol == self.list_type || symbol == self.map_type {
            return true;
        }
        self.types.contains_key(symbol)
            || self
                .enum_variants
                .iter()
                .any(|variants| variants.contains_key(&symbol))
    }
}

fn bool_contract_error() -> Error {
    Error::new("if condition must have type Bool")
}
