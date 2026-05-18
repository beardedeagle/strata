use std::collections::BTreeSet;
use std::fmt;

use crate::validation::{
    validate_count, validate_encoded_artifact_size, validate_ident_field, validate_output_text,
    validate_source_hash, validate_state_value_identity_label,
    validate_unique_message_variant_list, validate_unique_state_value_list,
};
mod codec;
mod process_validation;
mod value_template;

pub use value_template::{
    ArtifactMapEntry, ArtifactPayload, ArtifactProcessRefPayload, ArtifactRecordField,
    ArtifactValue, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, MapProjectionMode,
};

use crate::{
    ARTIFACT_FORMAT, ARTIFACT_MAGIC, ARTIFACT_SCHEMA_VERSION, EnumVariantId, Error, LoopElementId,
    MAX_ACTIONS_PER_PROCESS, MAX_EFFECTS_PER_TRANSITION, MAX_ENUM_VARIANTS_PER_TYPE,
    MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS,
    MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MessageId, OutputId,
    ProcessId, ProcessRefId, Result, StateId, TypeId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Stop,
    Panic,
}

impl StepResult {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
            Self::Panic => "Panic",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "Continue" => Ok(Self::Continue),
            "Stop" => Ok(Self::Stop),
            "Panic" => Ok(Self::Panic),
            _ => Err(Error::new(format!("invalid step_result value {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactEffect {
    Emit,
    Spawn,
    Send,
}

impl ArtifactEffect {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Emit => "emit",
            Self::Spawn => "spawn",
            Self::Send => "send",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "emit" => Ok(Self::Emit),
            "spawn" => Ok(Self::Spawn),
            "send" => Ok(Self::Send),
            _ => Err(Error::new(format!("invalid effect value {value:?}"))),
        }
    }
}

impl fmt::Display for ArtifactEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextState {
    Current,
    Value(StateId),
    Template(ArtifactValueTemplate),
    IfElse {
        condition: ArtifactValueTemplate,
        then_state: Box<NextState>,
        else_state: Box<NextState>,
    },
}

impl NextState {
    pub(crate) fn kind_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Value(_) => "value",
            Self::Template(_) => "template",
            Self::IfElse { .. } => "if_else",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactTypeKind {
    Value,
    ProcessRef { target: ProcessId },
}

impl ArtifactTypeKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::ProcessRef { .. } => "process_ref",
        }
    }

    pub(crate) fn parse(value: &str, target: Option<ProcessId>) -> Result<Self> {
        match (value, target) {
            ("value", None) => Ok(Self::Value),
            ("process_ref", Some(target)) => Ok(Self::ProcessRef { target }),
            ("process_ref", None) => Err(Error::new(
                "process_ref artifact type requires target_process",
            )),
            ("value", Some(_)) => Err(Error::new(
                "value artifact type must not declare target_process",
            )),
            _ => Err(Error::new(format!("invalid artifact type kind {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactType {
    pub label: String,
    pub kind: ArtifactTypeKind,
    pub shape: Option<ArtifactValueShape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueShape {
    Atom,
    Record {
        fields: Vec<ArtifactTypeField>,
    },
    Enum {
        variants: Vec<ArtifactEnumVariant>,
    },
    List {
        element: TypeId,
        capacity: usize,
    },
    Map {
        key: TypeId,
        value: TypeId,
        capacity: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTypeField {
    pub name: String,
    pub ty: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactEnumVariant {
    pub label: String,
    pub payload_type: Option<TypeId>,
}

impl ArtifactType {
    pub fn value(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::Atom),
        }
    }

    pub fn enum_value(label: impl Into<String>, enum_variants: Vec<String>) -> Self {
        Self::enum_value_with_payloads(
            label,
            enum_variants
                .into_iter()
                .map(|label| ArtifactEnumVariant {
                    label,
                    payload_type: None,
                })
                .collect(),
        )
    }

    pub fn enum_value_with_payloads(
        label: impl Into<String>,
        variants: Vec<ArtifactEnumVariant>,
    ) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::Enum { variants }),
        }
    }

    pub fn record(label: impl Into<String>, fields: Vec<ArtifactTypeField>) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::Record { fields }),
        }
    }

    pub fn list(label: impl Into<String>, element: TypeId, capacity: usize) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::List { element, capacity }),
        }
    }

    pub fn map(label: impl Into<String>, key: TypeId, value: TypeId, capacity: usize) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::Map {
                key,
                value,
                capacity,
            }),
        }
    }

    pub fn process_ref(label: impl Into<String>, target: ProcessId) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::ProcessRef { target },
            shape: None,
        }
    }

    pub fn value_shape(&self) -> Result<&ArtifactValueShape> {
        match (&self.kind, &self.shape) {
            (ArtifactTypeKind::Value, Some(shape)) => Ok(shape),
            (ArtifactTypeKind::Value, None) => Err(Error::new(format!(
                "value type {} must declare a value shape",
                self.label
            ))),
            (ArtifactTypeKind::ProcessRef { .. }, Some(_)) => Err(Error::new(format!(
                "process reference type {} must not declare a value shape",
                self.label
            ))),
            (ArtifactTypeKind::ProcessRef { .. }, None) => Err(Error::new(format!(
                "process reference type {} does not have a value shape",
                self.label
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleArtifact {
    pub format: String,
    pub schema_version: String,
    pub source_language: String,
    pub module: String,
    pub entry_process: ProcessId,
    pub entry_message: MessageId,
    pub types: Vec<ArtifactType>,
    pub outputs: Vec<String>,
    pub processes: Vec<ArtifactProcess>,
    pub source_hash_fnv1a64: String,
}

impl MantleArtifact {
    pub fn validate(&self) -> Result<()> {
        validate_artifact_identity(&self.format, &self.schema_version)?;
        validate_ident_field("source_language", &self.source_language)?;
        validate_ident_field("module", &self.module)?;
        validate_source_hash(&self.source_hash_fnv1a64)?;
        validate_count("type_count", self.types.len(), 1, MAX_TYPE_COUNT)?;
        validate_count("process_count", self.processes.len(), 1, MAX_PROCESS_COUNT)?;
        validate_count("output_count", self.outputs.len(), 0, MAX_OUTPUT_LITERALS)?;
        self.validate_type_table()?;
        for output in &self.outputs {
            validate_output_text(output)?;
        }

        let mut process_debug_names = BTreeSet::new();
        for process in &self.processes {
            process.validate_identity(self)?;
            if !process_debug_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate process debug_name {}",
                    process.debug_name
                )));
            }
        }

        let Some(entry_process) = self.processes.get(self.entry_process.index()) else {
            return Err(Error::new(format!(
                "entry process id {} is not defined",
                self.entry_process.as_u32()
            )));
        };
        if self.entry_message.index() >= entry_process.message_variants.len() {
            return Err(Error::new(format!(
                "entry message id {} is not accepted by process id {}",
                self.entry_message.as_u32(),
                self.entry_process.as_u32()
            )));
        }
        if entry_process.message_variants[self.entry_message.index()]
            .payload_type
            .is_some()
        {
            return Err(Error::new(format!(
                "entry message id {} must not require a payload",
                self.entry_message.as_u32()
            )));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            process.validate_references(self, ProcessId::from_index(process_index)?)?;
        }
        validate_encoded_artifact_size(self)?;

        Ok(())
    }

    pub fn type_entry(&self, ty: TypeId) -> Result<&ArtifactType> {
        self.types
            .get(ty.index())
            .ok_or_else(|| Error::new(format!("artifact type id {} is not defined", ty.as_u32())))
    }

    pub fn type_label(&self, ty: TypeId) -> Result<&str> {
        Ok(self.type_entry(ty)?.label.as_str())
    }

    pub fn enum_variant_label(&self, ty: TypeId, variant: EnumVariantId) -> Result<&str> {
        let type_entry = self.type_entry(ty)?;
        enum_variant_entry(ty, type_entry, variant).map(|variant| variant.label.as_str())
    }

    pub fn enum_variant_payload_type(
        &self,
        ty: TypeId,
        variant: EnumVariantId,
    ) -> Result<Option<TypeId>> {
        let type_entry = self.type_entry(ty)?;
        enum_variant_entry(ty, type_entry, variant).map(|variant| variant.payload_type)
    }

    pub fn validate_value_type(&self, field: &str, ty: TypeId) -> Result<()> {
        validate_value_type_entry(field, ty, self.type_entry(ty)?)
    }

    pub fn validate_value_matches_type(
        &self,
        field: &str,
        ty: TypeId,
        value: &ArtifactValue,
    ) -> Result<()> {
        let type_entry = self.type_entry(ty)?;
        validate_value_type_entry(field, ty, type_entry)?;
        self.validate_value_matches_type_at_depth(field, ty, value, 0)
    }

    pub fn process_ref_target_for_type_id(&self, field: &str, ty: TypeId) -> Result<ProcessId> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::ProcessRef { target } => {
                self.processes.get(target.index()).ok_or_else(|| {
                    Error::new(format!(
                        "artifact field {field} type id {} targets undefined process id {}",
                        ty.as_u32(),
                        target.as_u32()
                    ))
                })?;
                Ok(target)
            }
            ArtifactTypeKind::Value => Err(Error::new(format!(
                "artifact field {field} type id {} must be a process reference type",
                ty.as_u32()
            ))),
        }
    }

    pub fn validate_process_ref_type_id_target(
        &self,
        field: &str,
        ty: TypeId,
        target_process: ProcessId,
    ) -> Result<()> {
        let target = self.process_ref_target_for_type_id(field, ty)?;
        if target != target_process {
            return Err(Error::new(format!(
                "artifact field {field} type id {} targets process id {}, expected {}",
                ty.as_u32(),
                target.as_u32(),
                target_process.as_u32()
            )));
        }
        Ok(())
    }

    pub fn evaluate_state_value(
        &self,
        template: &ArtifactValueTemplate,
        received_payload: Option<&ArtifactPayload>,
    ) -> Result<ArtifactStateValue> {
        self.evaluate_state_value_with_current_state(template, received_payload, None)
    }

    fn evaluate_state_value_with_current_state(
        &self,
        template: &ArtifactValueTemplate,
        received_payload: Option<&ArtifactPayload>,
        current_state_payload: Option<&ArtifactPayload>,
    ) -> Result<ArtifactStateValue> {
        template.evaluate_state_value(received_payload, current_state_payload, &|ty| {
            self.type_entry(ty).cloned()
        })
    }

    fn validate_type_table(&self) -> Result<()> {
        for (type_index, ty) in self.types.iter().enumerate() {
            validate_ident_field(&format!("type.{type_index}.label"), &ty.label)?;
            match ty.kind {
                ArtifactTypeKind::Value => {
                    let Some(shape) = &ty.shape else {
                        return Err(Error::new(format!(
                            "type.{type_index} value type must declare a value shape"
                        )));
                    };
                    self.validate_value_shape(type_index, shape)?;
                }
                ArtifactTypeKind::ProcessRef { target } => {
                    if ty.shape.is_some() {
                        return Err(Error::new(format!(
                            "type.{type_index} process reference type must not declare a value shape"
                        )));
                    }
                    if target.index() >= self.processes.len() {
                        return Err(Error::new(format!(
                            "type id {type_index} targets undefined process id {}",
                            target.as_u32()
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_value_shape(&self, type_index: usize, shape: &ArtifactValueShape) -> Result<()> {
        match shape {
            ArtifactValueShape::Atom => Ok(()),
            ArtifactValueShape::Record { fields } => {
                validate_count(
                    &format!("type.{type_index}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                let mut seen = BTreeSet::new();
                for (field_index, field) in fields.iter().enumerate() {
                    validate_ident_field(
                        &format!("type.{type_index}.field.{field_index}.name"),
                        &field.name,
                    )?;
                    if !seen.insert(field.name.as_str()) {
                        return Err(Error::new(format!(
                            "type.{type_index} duplicates field {}",
                            field.name
                        )));
                    }
                    validate_value_type_entry(
                        &format!("type.{type_index}.field.{field_index}.type_id"),
                        field.ty,
                        self.type_entry(field.ty)?,
                    )?;
                }
                Ok(())
            }
            ArtifactValueShape::Enum { variants } => {
                validate_count(
                    &format!("type.{type_index}.enum_variant_count"),
                    variants.len(),
                    1,
                    MAX_ENUM_VARIANTS_PER_TYPE,
                )?;
                let mut seen = BTreeSet::new();
                for (variant_index, variant) in variants.iter().enumerate() {
                    validate_ident_field(
                        &format!("type.{type_index}.enum_variant.{variant_index}"),
                        &variant.label,
                    )?;
                    if !seen.insert(variant.label.as_str()) {
                        return Err(Error::new(format!(
                            "type.{type_index} duplicates enum variant {}",
                            variant.label
                        )));
                    }
                    if let Some(payload_type) = variant.payload_type {
                        self.type_entry(payload_type)?;
                    }
                }
                Ok(())
            }
            ArtifactValueShape::List { element, capacity } => {
                validate_count(
                    &format!("type.{type_index}.capacity"),
                    *capacity,
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                validate_value_type_entry(
                    &format!("type.{type_index}.element_type_id"),
                    *element,
                    self.type_entry(*element)?,
                )
            }
            ArtifactValueShape::Map {
                key,
                value,
                capacity,
            } => {
                validate_count(
                    &format!("type.{type_index}.capacity"),
                    *capacity,
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                validate_value_type_entry(
                    &format!("type.{type_index}.key_type_id"),
                    *key,
                    self.type_entry(*key)?,
                )?;
                validate_value_type_entry(
                    &format!("type.{type_index}.value_type_id"),
                    *value,
                    self.type_entry(*value)?,
                )
            }
        }
    }

    fn validate_value_matches_type_at_depth(
        &self,
        field: &str,
        ty: TypeId,
        value: &ArtifactValue,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum typed value depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        let type_entry = self.type_entry(ty)?;
        validate_value_type_entry(field, ty, type_entry)?;
        value.validate_without_process_ref(field)?;
        match type_entry.value_shape()? {
            ArtifactValueShape::Atom => validate_atom_value(field, ty, type_entry, value),
            ArtifactValueShape::Enum { variants } => {
                self.validate_enum_value(field, ty, variants, value, depth)
            }
            ArtifactValueShape::Record { fields } => {
                self.validate_record_value(field, ty, type_entry, fields, value, depth)
            }
            ArtifactValueShape::List { element, capacity } => {
                self.validate_list_value(field, *element, *capacity, value, depth)
            }
            ArtifactValueShape::Map {
                key,
                value: item,
                capacity,
            } => self.validate_map_value(field, *key, *item, *capacity, value, depth),
        }
    }

    fn validate_enum_value(
        &self,
        field: &str,
        ty: TypeId,
        variants: &[ArtifactEnumVariant],
        value: &ArtifactValue,
        depth: usize,
    ) -> Result<()> {
        match value {
            ArtifactValue::Atom(label) => {
                let Some(variant) = variants.iter().find(|variant| variant.label == *label) else {
                    return Err(value_not_member_error(
                        field,
                        ty,
                        self.type_entry(ty)?,
                        value,
                    ));
                };
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "{field} enum variant {label} requires a payload"
                    )));
                }
                Ok(())
            }
            ArtifactValue::EnumVariant { variant, payload } => {
                let Some(entry) = variants.iter().find(|entry| entry.label == *variant) else {
                    return Err(value_not_member_error(
                        field,
                        ty,
                        self.type_entry(ty)?,
                        value,
                    ));
                };
                let Some(payload_type) = entry.payload_type else {
                    return Err(Error::new(format!(
                        "{field} enum variant {variant} must not carry a payload"
                    )));
                };
                self.validate_value_matches_type_at_depth(
                    &format!("{field}.payload"),
                    payload_type,
                    payload,
                    depth + 1,
                )
            }
            ArtifactValue::Record { .. }
            | ArtifactValue::List(_)
            | ArtifactValue::Map(_)
            | ArtifactValue::ProcessRef { .. } => Err(value_not_member_error(
                field,
                ty,
                self.type_entry(ty)?,
                value,
            )),
        }
    }

    fn validate_record_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        expected_fields: &[ArtifactTypeField],
        value: &ArtifactValue,
        depth: usize,
    ) -> Result<()> {
        let ArtifactValue::Record {
            constructor,
            fields,
        } = value
        else {
            return Err(Error::new(format!(
                "{field} value {} does not match record type {} (type id {})",
                value.label(),
                type_entry.label,
                ty.as_u32()
            )));
        };
        if constructor != &type_entry.label {
            return Err(Error::new(format!(
                "{field} record constructor {constructor} does not match type {} (type id {})",
                type_entry.label,
                ty.as_u32()
            )));
        }
        if fields.len() != expected_fields.len() {
            return Err(Error::new(format!(
                "{field} record field_count is {}, expected {} for type {} (type id {})",
                fields.len(),
                expected_fields.len(),
                type_entry.label,
                ty.as_u32()
            )));
        }
        let mut seen = BTreeSet::new();
        for actual in fields {
            if !seen.insert(actual.name.as_str()) {
                return Err(Error::new(format!(
                    "{field} record duplicates field {}",
                    actual.name
                )));
            }
            let Some(expected) = expected_fields
                .iter()
                .find(|expected| expected.name == actual.name)
            else {
                return Err(Error::new(format!(
                    "{field} record field {} is not declared by type {} (type id {})",
                    actual.name,
                    type_entry.label,
                    ty.as_u32()
                )));
            };
            self.validate_value_matches_type_at_depth(
                &format!("{field}.field.{}", actual.name),
                expected.ty,
                &actual.value,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn validate_list_value(
        &self,
        field: &str,
        element: TypeId,
        capacity: usize,
        value: &ArtifactValue,
        depth: usize,
    ) -> Result<()> {
        let ArtifactValue::List(items) = value else {
            return Err(Error::new(format!(
                "{field} value {} does not match list type",
                value.label()
            )));
        };
        if items.len() > capacity {
            return Err(Error::new(format!(
                "{field} list item_count is {}, capacity is {}",
                items.len(),
                capacity
            )));
        }
        for (index, item) in items.iter().enumerate() {
            self.validate_value_matches_type_at_depth(
                &format!("{field}.item.{index}"),
                element,
                item,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn validate_map_value(
        &self,
        field: &str,
        key: TypeId,
        item: TypeId,
        capacity: usize,
        value: &ArtifactValue,
        depth: usize,
    ) -> Result<()> {
        let ArtifactValue::Map(entries) = value else {
            return Err(Error::new(format!(
                "{field} value {} does not match map type",
                value.label()
            )));
        };
        if entries.len() > capacity {
            return Err(Error::new(format!(
                "{field} map entry_count is {}, capacity is {}",
                entries.len(),
                capacity
            )));
        }
        let mut seen = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            self.validate_value_matches_type_at_depth(
                &format!("{field}.entry.{index}.key"),
                key,
                &entry.key,
                depth + 1,
            )?;
            if !seen.insert(&entry.key) {
                return Err(Error::new(format!(
                    "{field} duplicates map key {}",
                    entry.key.label()
                )));
            }
            self.validate_value_matches_type_at_depth(
                &format!("{field}.entry.{index}.value"),
                item,
                &entry.value,
                depth + 1,
            )?;
        }
        Ok(())
    }
}

fn validate_value_type_entry(field: &str, ty: TypeId, type_entry: &ArtifactType) -> Result<()> {
    match type_entry.kind {
        ArtifactTypeKind::Value => Ok(()),
        ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
            "artifact field {field} type id {} must be a value type",
            ty.as_u32()
        ))),
    }
}

fn enum_variant_entry(
    ty: TypeId,
    type_entry: &ArtifactType,
    variant: EnumVariantId,
) -> Result<&ArtifactEnumVariant> {
    let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
        return Err(Error::new(format!(
            "artifact type id {} is not an enum type",
            ty.as_u32()
        )));
    };
    variants.get(variant.index()).ok_or_else(|| {
        Error::new(format!(
            "artifact type id {} has no enum variant id {}",
            ty.as_u32(),
            variant.as_u32()
        ))
    })
}

fn validate_atom_value(
    field: &str,
    ty: TypeId,
    type_entry: &ArtifactType,
    value: &ArtifactValue,
) -> Result<()> {
    if matches!(value, ArtifactValue::Atom(_)) {
        return Ok(());
    }
    Err(Error::new(format!(
        "{field} value {} does not match atom type {} (type id {})",
        value.label(),
        type_entry.label,
        ty.as_u32()
    )))
}

fn value_not_member_error(
    field: &str,
    ty: TypeId,
    type_entry: &ArtifactType,
    value: &ArtifactValue,
) -> Error {
    Error::new(format!(
        "{field} value {} is not a member of enum type {} (type id {})",
        value.label(),
        type_entry.label,
        ty.as_u32()
    ))
}

pub fn validate_value_enum_membership(
    field: &str,
    ty: TypeId,
    type_entry: &ArtifactType,
    value: &ArtifactValue,
) -> Result<()> {
    let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
        return Ok(());
    };
    match value {
        ArtifactValue::Atom(label)
            if variants
                .iter()
                .any(|variant| variant.label == *label && variant.payload_type.is_none()) =>
        {
            Ok(())
        }
        ArtifactValue::EnumVariant { variant, .. }
            if variants
                .iter()
                .any(|declared| declared.label == *variant && declared.payload_type.is_some()) =>
        {
            Ok(())
        }
        _ => Err(value_not_member_error(field, ty, type_entry, value)),
    }
}

fn validate_artifact_identity(format: &str, schema_version: &str) -> Result<()> {
    if format != ARTIFACT_FORMAT {
        return Err(Error::new(format!(
            "unsupported artifact format {format}; expected {ARTIFACT_FORMAT}"
        )));
    }
    if schema_version != ARTIFACT_SCHEMA_VERSION {
        return Err(Error::new(format!(
            "unsupported artifact schema version {schema_version}; expected {ARTIFACT_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn validate_unique_process_ref_list(process_refs: &[ArtifactProcessRef]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for process_ref in process_refs {
        validate_ident_field("process reference", &process_ref.debug_name)?;
        if !seen.insert(process_ref.debug_name.as_str()) {
            return Err(Error::new(format!(
                "duplicate process reference {}",
                process_ref.debug_name
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMessageVariant {
    pub label: String,
    pub payload_type: Option<TypeId>,
}

impl ArtifactMessageVariant {
    pub fn unit(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            payload_type: None,
        }
    }

    pub fn payload(label: impl Into<String>, payload_type: TypeId) -> Self {
        Self {
            label: label.into(),
            payload_type: Some(payload_type),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStateValue {
    pub ty: TypeId,
    pub value: ArtifactValue,
    pub label: String,
    pub payload: Option<ArtifactPayload>,
}

impl ArtifactStateValue {
    pub fn new(ty: TypeId, value: ArtifactValue) -> Result<Self> {
        Self::from_value(ty, value)
    }

    pub fn with_label(ty: TypeId, value: ArtifactValue, label: impl AsRef<str>) -> Result<Self> {
        let label = label.as_ref();
        value.validate_without_process_ref("state value")?;
        validate_state_value_identity_label(&value, label)?;
        Ok(Self {
            ty,
            value,
            label: label.to_string(),
            payload: None,
        })
    }

    pub fn from_value(ty: TypeId, value: ArtifactValue) -> Result<Self> {
        value.validate_without_process_ref("state value")?;
        let label = value.label();
        Ok(Self {
            ty,
            value,
            label,
            payload: None,
        })
    }

    fn has_same_identity(&self, other: &Self) -> bool {
        self.ty == other.ty && self.value == other.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcess {
    pub debug_name: String,
    pub state_type: TypeId,
    pub state_values: Vec<ArtifactStateValue>,
    pub message_type: TypeId,
    pub message_variants: Vec<ArtifactMessageVariant>,
    pub process_refs: Vec<ArtifactProcessRef>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub transitions: Vec<ArtifactTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProcessRef {
    pub debug_name: String,
    pub target: ProcessId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTransition {
    pub current_state: Option<StateId>,
    pub message: MessageId,
    pub payload_guard: Option<ArtifactPayload>,
    pub step_result: StepResult,
    pub next_state: NextState,
    pub effects: Vec<ArtifactEffect>,
    pub actions: Vec<ArtifactAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLoopElement {
    pub id: LoopElementId,
    pub ty: TypeId,
}

impl ArtifactTransition {
    fn transition_context(&self) -> String {
        match self.current_state {
            Some(current_state) => format!(
                "message id {} current_state id {}",
                self.message.as_u32(),
                current_state.as_u32()
            ),
            None => format!("message id {}", self.message.as_u32()),
        }
    }

    fn validate_effects(&self, process_debug_name: &str) -> Result<BTreeSet<ArtifactEffect>> {
        validate_count(
            "effect_count",
            self.effects.len(),
            0,
            MAX_EFFECTS_PER_TRANSITION,
        )?;
        let mut effects = BTreeSet::new();
        for &effect in &self.effects {
            if !effects.insert(effect) {
                return Err(Error::new(format!(
                    "process {process_debug_name} transition {} declares duplicate effect {effect}",
                    self.message.as_u32()
                )));
            }
        }
        Ok(effects)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactBranch {
    Then,
    Else,
}

impl ArtifactBranch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Then => "then",
            Self::Else => "else",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactAction {
    Emit {
        output: OutputId,
    },
    Spawn {
        target: ProcessId,
        process_ref: ProcessRefId,
    },
    Send {
        target: ArtifactSendTarget,
        message: MessageId,
        payload: Option<ArtifactValueTemplate>,
    },
    IfElse {
        condition: ArtifactValueTemplate,
        then_actions: Vec<ArtifactAction>,
        else_actions: Vec<ArtifactAction>,
    },
    ForEach {
        element: ArtifactLoopElement,
        collection: ArtifactValueTemplate,
        max_items: usize,
        body: Vec<ArtifactAction>,
    },
}

impl ArtifactAction {
    fn collect_effects(&self, effects: &mut BTreeSet<ArtifactEffect>) {
        match self {
            Self::Emit { .. } => {
                effects.insert(ArtifactEffect::Emit);
            }
            Self::Spawn { .. } => {
                effects.insert(ArtifactEffect::Spawn);
            }
            Self::Send { .. } => {
                effects.insert(ArtifactEffect::Send);
            }
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                for action in then_actions {
                    action.collect_effects(effects);
                }
                for action in else_actions {
                    action.collect_effects(effects);
                }
            }
            Self::ForEach { body, .. } => {
                for action in body {
                    action.collect_effects(effects);
                }
            }
        }
    }

    fn action_count_at_depth(&self, depth: usize) -> Result<usize> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "artifact action nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        match self {
            Self::Emit { .. } | Self::Spawn { .. } | Self::Send { .. } => Ok(1),
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                let then_count = action_count_at_depth(then_actions, depth + 1)?;
                let else_count = action_count_at_depth(else_actions, depth + 1)?;
                then_count
                    .checked_add(else_count)
                    .and_then(|count| count.checked_add(1))
                    .ok_or_else(|| Error::new("artifact action_count overflowed"))
            }
            Self::ForEach { body, .. } => action_count_at_depth(body, depth + 1)?
                .checked_add(1)
                .ok_or_else(|| Error::new("artifact action_count overflowed")),
        }
    }
}

fn action_count(actions: &[ArtifactAction]) -> Result<usize> {
    action_count_at_depth(actions, 0)
}

fn action_count_at_depth(actions: &[ArtifactAction], depth: usize) -> Result<usize> {
    actions.iter().try_fold(0usize, |count, action| {
        count
            .checked_add(action.action_count_at_depth(depth)?)
            .ok_or_else(|| Error::new("artifact action_count overflowed"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSendTarget {
    ProcessRef(ProcessRefId),
    ReceivedPayload {
        ty: TypeId,
        target_process: ProcessId,
    },
}
