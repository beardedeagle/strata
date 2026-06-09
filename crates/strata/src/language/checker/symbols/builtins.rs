use super::super::super::ast::{Enum, Identifier, Module, TypeRef};
use super::super::super::diagnostic::{Error, Result};
use super::super::super::{
    BOOL_FALSE, BOOL_TRUE, OPTION_TYPE, RESULT_TYPE, SEND_ERROR_TYPE, SPAWN_ERROR_TYPE,
};
use super::type_validation::BuiltinTypeSymbols;
use super::{SemanticIndex, Symbol, TypeDecl};

pub(super) fn is_builtin_value_constructor_name(name: &str) -> bool {
    matches!(
        name,
        "None"
            | "Some"
            | "Ok"
            | "Err"
            | "Full"
            | "Stopped"
            | "Crashed"
            | "MailboxClosed"
            | "Denied"
            | "Exhausted"
            | "BackendUnavailable"
            | BOOL_FALSE
            | BOOL_TRUE
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language::checker) struct ValueEnum {
    pub(in crate::language::checker) name: String,
    pub(in crate::language::checker) variants: Vec<ValueEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language::checker) struct ValueEnumVariant {
    pub(in crate::language::checker) name: Identifier,
    pub(in crate::language::checker) payload_type: Option<TypeRef>,
}

pub(in crate::language::checker) struct ValueEnumVariantInfo {
    pub(in crate::language::checker) index: usize,
    pub(in crate::language::checker) payload_type: Option<TypeRef>,
}

impl SemanticIndex {
    pub(super) fn builtin_type_symbols(&self) -> BuiltinTypeSymbols {
        BuiltinTypeSymbols {
            option: self.option_type,
            result: self.result_type,
            send_error: self.send_error_type,
            spawn_error: self.spawn_error_type,
        }
    }

    pub(in crate::language::checker) fn is_unit_type(&self, ty: &TypeRef) -> Result<bool> {
        self.is_named_builtin(ty, self.unit_type)
    }

    pub(in crate::language::checker) fn value_enum(
        &self,
        module: &Module,
        ty: &TypeRef,
    ) -> Result<ValueEnum> {
        if let Some(value_enum) = self.builtin_value_enum(ty)? {
            return Ok(value_enum);
        }
        let enum_decl = self.enum_decl(module, ty)?;
        Ok(module_enum(enum_decl))
    }

    pub(in crate::language::checker) fn value_enum_name<'a>(
        &self,
        module: &'a Module,
        ty: &'a TypeRef,
    ) -> Result<&'a str> {
        if let Some(name) = self.builtin_value_enum_name(ty)? {
            return Ok(name);
        }
        Ok(self.enum_decl(module, ty)?.name.as_str())
    }

    pub(in crate::language::checker) fn value_enum_variant(
        &self,
        module: &Module,
        ty: &TypeRef,
        variant: &Identifier,
    ) -> Result<ValueEnumVariantInfo> {
        if let Some(info) = self.value_enum_variant_option(module, ty, variant)? {
            return Ok(info);
        }
        Err(Error::new(format!(
            "value {variant} is not a variant of enum {}",
            self.value_enum_name(module, ty)?
        )))
    }

    pub(in crate::language::checker) fn value_enum_variant_option(
        &self,
        module: &Module,
        ty: &TypeRef,
        variant: &Identifier,
    ) -> Result<Option<ValueEnumVariantInfo>> {
        if let Some(info) = self.builtin_value_enum_variant_option(ty, variant)? {
            return Ok(Some(info));
        }
        if ty.as_named().is_none() {
            return Ok(None);
        }
        let TypeDecl::Enum(enum_index) = self.type_decl(ty)? else {
            return Ok(None);
        };
        let enum_decl = module
            .enums
            .get(enum_index)
            .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?;
        let Some(variant_symbol) = self.symbols.resolve(variant.as_str()) else {
            return Ok(None);
        };
        let Some(variant_index) = self
            .enum_variants
            .get(enum_index)
            .ok_or_else(|| Error::new(format!("enum index {enum_index} is not declared")))?
            .get(&variant_symbol)
            .copied()
        else {
            return Ok(None);
        };
        let variant_decl = enum_decl.variants.get(variant_index).ok_or_else(|| {
            Error::new(format!(
                "enum {} variant index {variant_index} is not declared",
                enum_decl.name
            ))
        })?;
        Ok(Some(ValueEnumVariantInfo {
            index: variant_index,
            payload_type: variant_decl.payload_type.clone(),
        }))
    }

    pub(super) fn builtin_value_enum_variant_index(
        &self,
        ty: &TypeRef,
        variant: &Identifier,
    ) -> Result<Option<usize>> {
        Ok(self
            .builtin_value_enum_variant_option(ty, variant)?
            .map(|info| info.index))
    }

    pub(in crate::language::checker) fn builtin_value_shape(
        &self,
        ty: &TypeRef,
    ) -> Result<Option<BuiltinValueShape>> {
        if self.is_unit_type(ty)? {
            return Ok(Some(BuiltinValueShape::Unit));
        }
        if let Some(value_enum) = self.builtin_value_enum(ty)? {
            return Ok(Some(BuiltinValueShape::Enum(value_enum)));
        }
        Ok(None)
    }

    fn is_named_builtin(&self, ty: &TypeRef, expected: Symbol) -> Result<bool> {
        let TypeRef::Named(name) = ty else {
            return Ok(false);
        };
        let symbol = self
            .symbols
            .resolve(name.as_str())
            .ok_or_else(|| Error::new(format!("type {name} is not declared")))?;
        Ok(symbol == expected)
    }

    fn builtin_value_enum(&self, ty: &TypeRef) -> Result<Option<ValueEnum>> {
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return Ok(None);
        };
        let symbol = self
            .symbols
            .resolve(constructor.as_str())
            .ok_or_else(|| Error::new(format!("type {ty} is not declared")))?;
        if !const_args.is_empty() {
            return Ok(None);
        }
        if symbol == self.option_type && args.len() == 1 {
            return Ok(Some(value_enum(
                OPTION_TYPE,
                [("None", None), ("Some", Some(args[0].clone()))],
            )?));
        }
        if symbol == self.result_type && args.len() == 2 {
            return Ok(Some(value_enum(
                RESULT_TYPE,
                [
                    ("Ok", Some(args[0].clone())),
                    ("Err", Some(args[1].clone())),
                ],
            )?));
        }
        if symbol == self.send_error_type && args.len() == 1 {
            return Ok(Some(value_enum(
                SEND_ERROR_TYPE,
                [
                    ("Full", Some(args[0].clone())),
                    ("Stopped", Some(args[0].clone())),
                    ("Crashed", Some(args[0].clone())),
                    ("MailboxClosed", Some(args[0].clone())),
                ],
            )?));
        }
        if symbol == self.spawn_error_type && args.len() == 1 {
            return Ok(Some(value_enum(
                SPAWN_ERROR_TYPE,
                [
                    ("Denied", Some(args[0].clone())),
                    ("Exhausted", Some(args[0].clone())),
                    ("BackendUnavailable", Some(args[0].clone())),
                ],
            )?));
        }
        Ok(None)
    }

    fn builtin_value_enum_name(&self, ty: &TypeRef) -> Result<Option<&'static str>> {
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return Ok(None);
        };
        let symbol = self
            .symbols
            .resolve(constructor.as_str())
            .ok_or_else(|| Error::new(format!("type {ty} is not declared")))?;
        if !const_args.is_empty() {
            return Ok(None);
        }
        if symbol == self.option_type && args.len() == 1 {
            return Ok(Some(OPTION_TYPE));
        }
        if symbol == self.result_type && args.len() == 2 {
            return Ok(Some(RESULT_TYPE));
        }
        if symbol == self.send_error_type && args.len() == 1 {
            return Ok(Some(SEND_ERROR_TYPE));
        }
        if symbol == self.spawn_error_type && args.len() == 1 {
            return Ok(Some(SPAWN_ERROR_TYPE));
        }
        Ok(None)
    }

    fn builtin_value_enum_variant_option(
        &self,
        ty: &TypeRef,
        variant: &Identifier,
    ) -> Result<Option<ValueEnumVariantInfo>> {
        let TypeRef::Applied {
            constructor,
            args,
            const_args,
        } = ty
        else {
            return Ok(None);
        };
        let symbol = self
            .symbols
            .resolve(constructor.as_str())
            .ok_or_else(|| Error::new(format!("type {ty} is not declared")))?;
        if !const_args.is_empty() {
            return Ok(None);
        }
        if symbol == self.option_type && args.len() == 1 {
            return Ok(builtin_variant(
                [("None", None), ("Some", Some(&args[0]))],
                variant,
            ));
        }
        if symbol == self.result_type && args.len() == 2 {
            return Ok(builtin_variant(
                [("Ok", Some(&args[0])), ("Err", Some(&args[1]))],
                variant,
            ));
        }
        if symbol == self.send_error_type && args.len() == 1 {
            return Ok(builtin_variant(
                [
                    ("Full", Some(&args[0])),
                    ("Stopped", Some(&args[0])),
                    ("Crashed", Some(&args[0])),
                    ("MailboxClosed", Some(&args[0])),
                ],
                variant,
            ));
        }
        if symbol == self.spawn_error_type && args.len() == 1 {
            return Ok(builtin_variant(
                [
                    ("Denied", Some(&args[0])),
                    ("Exhausted", Some(&args[0])),
                    ("BackendUnavailable", Some(&args[0])),
                ],
                variant,
            ));
        }
        Ok(None)
    }
}

pub(in crate::language::checker) enum BuiltinValueShape {
    Unit,
    Enum(ValueEnum),
}

fn module_enum(enum_decl: &Enum) -> ValueEnum {
    ValueEnum {
        name: enum_decl.name.to_string(),
        variants: enum_decl
            .variants
            .iter()
            .map(|variant| ValueEnumVariant {
                name: variant.name.clone(),
                payload_type: variant.payload_type.clone(),
            })
            .collect(),
    }
}

fn value_enum<const N: usize>(
    name: &str,
    variants: [(&str, Option<TypeRef>); N],
) -> Result<ValueEnum> {
    Ok(ValueEnum {
        name: name.to_string(),
        variants: variants
            .into_iter()
            .map(|(name, payload_type)| {
                Ok(ValueEnumVariant {
                    name: Identifier::new(name)?,
                    payload_type,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

fn builtin_variant<const N: usize>(
    variants: [(&'static str, Option<&TypeRef>); N],
    variant: &Identifier,
) -> Option<ValueEnumVariantInfo> {
    variants
        .into_iter()
        .enumerate()
        .find_map(|(index, (name, payload_type))| {
            (name == variant.as_str()).then(|| ValueEnumVariantInfo {
                index,
                payload_type: payload_type.cloned(),
            })
        })
}
