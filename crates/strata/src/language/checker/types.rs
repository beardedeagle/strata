use std::fmt::Write as _;

use mantle_artifact::{MAX_IDENTIFIER_BYTES, MAX_TYPE_COUNT};

use super::super::ast::{Module, TypeRef};
use super::super::checked::{
    CheckedEnumVariant, CheckedProcessId, CheckedTypeField, CheckedTypeId, CheckedTypeKind,
    CheckedTypeRef, CheckedValueShape,
};
use super::super::diagnostic::{Error, Result};
use super::CHECKED_TYPE_LABEL_PREFIX;
use super::symbols::{BuiltinValueShape, CollectionType, SemanticIndex};

const CHECKED_PROCESS_REF_TYPE_LABEL_PREFIX: &str = "__strata_checked_process_ref_";

pub(super) struct CheckedTypeInterner<'a> {
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    entries: Vec<(TypeRef, CheckedTypeRef)>,
}

impl<'a> CheckedTypeInterner<'a> {
    pub(super) fn new(module: &'a Module, semantic_index: &'a SemanticIndex) -> Self {
        Self {
            module,
            semantic_index,
            entries: Vec::new(),
        }
    }

    pub(super) fn intern(&mut self, ty: &TypeRef) -> Result<CheckedTypeRef> {
        if let Some((_, checked)) = self
            .entries
            .iter()
            .find(|(existing, _)| self.semantic_index.same_type(existing, ty))
        {
            return Ok(checked.clone());
        }

        if self.entries.len() >= MAX_TYPE_COUNT {
            return Err(Error::new(format!(
                "checked type_count exceeds Mantle artifact limit of {MAX_TYPE_COUNT} types"
            )));
        }
        let id = CheckedTypeId::from_index(self.entries.len())?;
        let process_ref_target = self.semantic_index.process_ref_target_type(ty)?;
        let label = checked_type_label(ty, process_ref_target)?;
        let placeholder_kind = match process_ref_target {
            Some(target) => CheckedTypeKind::ProcessRef { target },
            None => CheckedTypeKind::Value {
                shape: CheckedValueShape::Atom,
            },
        };
        self.entries.push((
            ty.clone(),
            CheckedTypeRef::new(id, String::new(), placeholder_kind),
        ));

        let kind = match process_ref_target {
            Some(target) => CheckedTypeKind::ProcessRef { target },
            None => CheckedTypeKind::Value {
                shape: self.value_shape(ty)?,
            },
        };
        let checked = CheckedTypeRef::new(id, label, kind);
        self.entries[id.index()].1 = checked.clone();
        Ok(checked)
    }

    fn value_shape(&mut self, ty: &TypeRef) -> Result<CheckedValueShape> {
        if let Some(scalar) = self.semantic_index.scalar_type(ty)? {
            return Ok(CheckedValueShape::Scalar(scalar));
        }
        if let Some(collection) = self.semantic_index.collection_type(ty)? {
            return match collection {
                CollectionType::List { element, capacity } => {
                    let element = self.intern(element)?.id();
                    Ok(CheckedValueShape::List { element, capacity })
                }
                CollectionType::Map {
                    key,
                    value,
                    capacity,
                } => {
                    let key = self.intern(key)?.id();
                    let value = self.intern(value)?.id();
                    Ok(CheckedValueShape::Map {
                        key,
                        value,
                        capacity,
                    })
                }
            };
        }

        if let Some(shape) = self.semantic_index.builtin_value_shape(ty)? {
            return match shape {
                BuiltinValueShape::Unit => Ok(CheckedValueShape::Atom),
                BuiltinValueShape::Enum(value_enum) => {
                    let variants = value_enum
                        .variants
                        .into_iter()
                        .map(|variant| {
                            let payload_type = variant
                                .payload_type
                                .as_ref()
                                .map(|payload_type| self.intern(payload_type).map(|ty| ty.id()))
                                .transpose()?;
                            Ok(CheckedEnumVariant {
                                name: variant.name,
                                payload_type,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(CheckedValueShape::Enum { variants })
                }
            };
        }

        if let Ok(record_decl) = self.semantic_index.record_decl(self.module, ty) {
            if record_decl.fields.is_empty() {
                return Ok(CheckedValueShape::Atom);
            }
            let mut fields = Vec::with_capacity(record_decl.fields.len());
            for field in &record_decl.fields {
                fields.push(CheckedTypeField {
                    name: field.name.clone(),
                    ty: self.intern(&field.ty)?.id(),
                });
            }
            return Ok(CheckedValueShape::Record { fields });
        }

        if let Ok(enum_decl) = self.semantic_index.enum_decl(self.module, ty) {
            let mut variants = Vec::with_capacity(enum_decl.variants.len());
            for variant in &enum_decl.variants {
                let payload_type = variant
                    .payload_type
                    .as_ref()
                    .map(|payload_type| self.intern(payload_type).map(|ty| ty.id()))
                    .transpose()?;
                variants.push(CheckedEnumVariant {
                    name: variant.name.clone(),
                    payload_type,
                });
            }
            return Ok(CheckedValueShape::Enum { variants });
        }

        Err(Error::new(format!(
            "type {ty} is not declared as a source value type"
        )))
    }

    pub(super) fn source_type(&self, checked_ty: &CheckedTypeRef) -> Result<&TypeRef> {
        self.entries
            .get(checked_ty.id().index())
            .filter(|(_, checked)| checked == checked_ty)
            .map(|(ty, _)| ty)
            .ok_or_else(|| {
                Error::new(format!(
                    "checked type id {} is not interned",
                    checked_ty.id().as_u32()
                ))
            })
    }

    pub(super) fn into_types(self) -> Vec<CheckedTypeRef> {
        self.entries
            .into_iter()
            .map(|(_, checked)| checked)
            .collect()
    }
}

fn checked_type_label(
    ty: &TypeRef,
    process_ref_target: Option<CheckedProcessId>,
) -> Result<String> {
    if let Some(target) = process_ref_target {
        return checked_process_ref_type_label(target);
    }
    match ty {
        TypeRef::Named(name) => Ok(name.to_string()),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let mut label = String::with_capacity(checked_type_label_capacity_hint(ty));
            label.push_str(CHECKED_TYPE_LABEL_PREFIX);
            push_checked_type_label_named_component(&mut label, constructor.as_str())?;
            label.push('_');
            write!(&mut label, "{}", args.len())
                .map_err(|_| Error::new("failed to build checked type label"))?;
            label.push('_');
            write!(&mut label, "{}", const_args.len())
                .map_err(|_| Error::new("failed to build checked type label"))?;
            for arg in args {
                label.push('_');
                push_checked_type_label_component(&mut label, arg)?;
            }
            for value in const_args {
                label.push('_');
                write!(&mut label, "{value}")
                    .map_err(|_| Error::new("failed to build checked type label"))?;
            }
            if label.len() > MAX_IDENTIFIER_BYTES {
                return Err(Error::new(format!(
                    "checked type label for {ty} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
                )));
            }
            Ok(label)
        }
    }
}

fn checked_type_label_capacity_hint(ty: &TypeRef) -> usize {
    match ty {
        TypeRef::Named(name) => name.as_str().len(),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            CHECKED_TYPE_LABEL_PREFIX.len()
                + checked_type_label_named_component_capacity(constructor.as_str())
                + 1
                + decimal_len(args.len())
                + 1
                + decimal_len(const_args.len())
                + args
                    .iter()
                    .map(|arg| 1 + checked_type_label_component_capacity_hint(arg))
                    .sum::<usize>()
                + const_args
                    .iter()
                    .map(|value| 1 + decimal_len(*value))
                    .sum::<usize>()
        }
    }
}

fn checked_type_label_component_capacity_hint(ty: &TypeRef) -> usize {
    match ty {
        TypeRef::Named(name) => checked_type_label_named_component_capacity(name.as_str()),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            checked_type_label_named_component_capacity(constructor.as_str())
                + 1
                + decimal_len(args.len())
                + 1
                + decimal_len(const_args.len())
                + args
                    .iter()
                    .map(|arg| 1 + checked_type_label_component_capacity_hint(arg))
                    .sum::<usize>()
                + const_args
                    .iter()
                    .map(|value| 1 + decimal_len(*value))
                    .sum::<usize>()
        }
    }
}

fn checked_type_label_named_component_capacity(name: &str) -> usize {
    decimal_len(name.len()) + 1 + name.len()
}

fn decimal_len(value: usize) -> usize {
    value
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

fn push_checked_type_label_component(label: &mut String, ty: &TypeRef) -> Result<()> {
    match ty {
        TypeRef::Named(name) => push_checked_type_label_named_component(label, name.as_str()),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            push_checked_type_label_named_component(label, constructor.as_str())?;
            write!(label, "_{}_{}", args.len(), const_args.len())
                .map_err(|_| Error::new("failed to build checked type label"))?;
            for arg in args {
                label.push('_');
                push_checked_type_label_component(label, arg)?;
            }
            for value in const_args {
                label.push('_');
                write!(label, "{value}")
                    .map_err(|_| Error::new("failed to build checked type label"))?;
            }
            Ok(())
        }
    }
}

fn push_checked_type_label_named_component(label: &mut String, name: &str) -> Result<()> {
    write!(label, "{}_{name}", name.len())
        .map_err(|_| Error::new("failed to build checked type label"))
}

fn checked_process_ref_type_label(target: CheckedProcessId) -> Result<String> {
    let label = format!("{CHECKED_PROCESS_REF_TYPE_LABEL_PREFIX}{}", target.as_u32());
    if label.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "checked process reference type label exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    Ok(label)
}
