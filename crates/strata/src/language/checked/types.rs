use std::fmt;

use crate::language::ast::Identifier;
use crate::language::diagnostic::{Error, Result};

use super::{CheckedEnumVariantId, CheckedProcessId, CheckedTypeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedTypeKind {
    Value { shape: CheckedValueShape },
    ProcessRef { target: CheckedProcessId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedValueShape {
    Atom,
    Record {
        fields: Vec<CheckedTypeField>,
    },
    Enum {
        variants: Vec<CheckedEnumVariant>,
    },
    List {
        element: CheckedTypeId,
        capacity: usize,
    },
    Map {
        key: CheckedTypeId,
        value: CheckedTypeId,
        capacity: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedTypeField {
    pub(in crate::language) name: Identifier,
    pub(in crate::language) ty: CheckedTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedEnumVariant {
    pub(in crate::language) name: Identifier,
    pub(in crate::language) payload_type: Option<CheckedTypeId>,
}

#[derive(Debug, Clone)]
pub(in crate::language) struct CheckedTypeRef {
    id: CheckedTypeId,
    label: String,
    kind: CheckedTypeKind,
}

impl CheckedTypeRef {
    pub(in crate::language) fn new(
        id: CheckedTypeId,
        label: String,
        kind: CheckedTypeKind,
    ) -> Self {
        Self { id, label, kind }
    }

    pub(in crate::language) fn id(&self) -> CheckedTypeId {
        self.id
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn kind(&self) -> &CheckedTypeKind {
        &self.kind
    }

    pub(in crate::language) fn enum_variant_label(
        &self,
        variant: CheckedEnumVariantId,
    ) -> Result<&Identifier> {
        match &self.kind {
            CheckedTypeKind::Value {
                shape: CheckedValueShape::Enum { variants },
            } => variants
                .get(variant.index())
                .map(|variant| &variant.name)
                .ok_or_else(|| {
                    Error::new(format!(
                        "checked type {} has no enum variant id {}",
                        self.label,
                        variant.as_u32()
                    ))
                }),
            CheckedTypeKind::Value { .. } => Err(Error::new(format!(
                "checked type {} is not an enum value type",
                self.label
            ))),
            CheckedTypeKind::ProcessRef { .. } => Err(Error::new(format!(
                "checked type {} is not an enum value type",
                self.label
            ))),
        }
    }

    pub(in crate::language) fn enum_variant_payload_type(
        &self,
        variant: CheckedEnumVariantId,
    ) -> Result<Option<CheckedTypeId>> {
        match &self.kind {
            CheckedTypeKind::Value {
                shape: CheckedValueShape::Enum { variants },
            } => variants
                .get(variant.index())
                .map(|variant| variant.payload_type)
                .ok_or_else(|| {
                    Error::new(format!(
                        "checked type {} has no enum variant id {}",
                        self.label,
                        variant.as_u32()
                    ))
                }),
            CheckedTypeKind::Value { .. } | CheckedTypeKind::ProcessRef { .. } => Err(Error::new(
                format!("checked type {} is not an enum value type", self.label),
            )),
        }
    }

    #[cfg(test)]
    pub(in crate::language) fn test_value(label: &str) -> Self {
        Self::new(
            test_type_id(label, None),
            label.to_string(),
            CheckedTypeKind::Value {
                shape: CheckedValueShape::Atom,
            },
        )
    }

    #[cfg(test)]
    pub(in crate::language) fn test_enum_value(label: &str, enum_variants: &[&str]) -> Self {
        let enum_variants = enum_variants
            .iter()
            .map(|variant| Identifier::new(*variant).expect("test enum variant should be valid"))
            .map(|name| CheckedEnumVariant {
                name,
                payload_type: None,
            })
            .collect();
        Self::new(
            test_type_id(label, None),
            label.to_string(),
            CheckedTypeKind::Value {
                shape: CheckedValueShape::Enum {
                    variants: enum_variants,
                },
            },
        )
    }

    #[cfg(test)]
    pub(in crate::language) fn test_process_ref(label: &str, target: CheckedProcessId) -> Self {
        Self::new(
            test_type_id(label, Some(target)),
            label.to_string(),
            CheckedTypeKind::ProcessRef { target },
        )
    }
}

#[cfg(test)]
fn test_type_id(label: &str, target: Option<CheckedProcessId>) -> CheckedTypeId {
    let mut hash = 0x811c_9dc5u32;
    for byte in label.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    if let Some(target) = target {
        for byte in target.as_u32().to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    CheckedTypeId::from_raw_test(hash)
}

impl PartialEq for CheckedTypeRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for CheckedTypeRef {}

impl fmt::Display for CheckedTypeRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}
