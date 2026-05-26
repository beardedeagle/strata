use mantle_artifact::{MAX_IDENTIFIER_BYTES, MAX_TYPE_COUNT};

use super::super::ast::{Module, TypeRef};
use super::super::checked::{
    CheckedEnumVariant, CheckedProcessId, CheckedTypeField, CheckedTypeId, CheckedTypeKind,
    CheckedTypeRef, CheckedValueShape,
};
use super::super::diagnostic::{Error, Result};
use super::CHECKED_TYPE_LABEL_PREFIX;
use super::symbols::{CollectionType, SemanticIndex};

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
            CheckedTypeRef::new(id, label.clone(), placeholder_kind),
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

        if let Ok(record_decl) = self.semantic_index.record_decl(self.module, ty) {
            if record_decl.fields.is_empty() {
                return Ok(CheckedValueShape::Atom);
            }
            let fields = record_decl
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.ty.clone()))
                .collect::<Vec<_>>();
            let fields = fields
                .into_iter()
                .map(|(name, field_ty)| {
                    Ok(CheckedTypeField {
                        name,
                        ty: self.intern(&field_ty)?.id(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(CheckedValueShape::Record { fields });
        }

        if let Ok(enum_decl) = self.semantic_index.enum_decl(self.module, ty) {
            let variants = enum_decl
                .variants
                .iter()
                .map(|variant| (variant.name.clone(), variant.payload_type.clone()))
                .collect::<Vec<_>>();
            let variants = variants
                .into_iter()
                .map(|(name, payload_type)| {
                    let payload_type = payload_type
                        .as_ref()
                        .map(|payload_type| self.intern(payload_type).map(|ty| ty.id()))
                        .transpose()?;
                    Ok(CheckedEnumVariant { name, payload_type })
                })
                .collect::<Result<Vec<_>>>()?;
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
            let mut label = format!(
                "{CHECKED_TYPE_LABEL_PREFIX}{}",
                checked_type_label_component(&TypeRef::Named(constructor.clone()))?
            );
            label.push('_');
            label.push_str(&args.len().to_string());
            label.push('_');
            label.push_str(&const_args.len().to_string());
            for arg in args {
                label.push('_');
                label.push_str(&checked_type_label_component(arg)?);
            }
            for value in const_args {
                label.push('_');
                label.push_str(&value.to_string());
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

fn checked_type_label_component(ty: &TypeRef) -> Result<String> {
    match ty {
        TypeRef::Named(name) => Ok(format!("{}_{}", name.as_str().len(), name)),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let mut label = format!(
                "{}_{}_{}_{}",
                constructor.as_str().len(),
                constructor,
                args.len(),
                const_args.len()
            );
            for arg in args {
                label.push('_');
                label.push_str(&checked_type_label_component(arg)?);
            }
            for value in const_args {
                label.push('_');
                label.push_str(&value.to_string());
            }
            Ok(label)
        }
    }
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
