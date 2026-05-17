use std::collections::BTreeSet;

pub(crate) use actions::{LoadedAction, LoadedLoopElement, LoadedSendTarget};
use admission::{
    validate_loaded_artifact_identity, validate_loaded_ident_field, validate_loaded_output_text,
};
pub(crate) use effects::LoadedEffectAuthority;
use templates::{
    LoadedTemplateAdmission, evaluate_loaded_state_value,
    loaded_template_depends_on_received_payload, validate_loaded_bool_condition,
};
use transitions::{TransitionLookup, load_transitions, validate_loaded_transition_coverage};
pub use values::RuntimePayload;
pub(crate) use values::{
    LoadedStateValue, LoadedValueTemplate, LoadedValueTemplateField, LoadedValueTemplateMapEntry,
    RuntimeValue,
};

mod actions;
mod admission;
mod effects;
mod templates;
mod transitions;
mod values;

use mantle_artifact::{
    ArtifactAction, ArtifactEnumVariant, ArtifactMessageVariant, ArtifactProcess,
    ArtifactProcessRef, ArtifactSendTarget, ArtifactTransition, ArtifactType, ArtifactTypeField,
    ArtifactTypeKind, ArtifactValueShape, EnumVariantId, Error, LoopElementId,
    MAX_ACTIONS_PER_PROCESS, MAX_ENUM_VARIANTS_PER_TYPE, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT,
    MAX_PROCESS_REFS_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS,
    MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact, MessageId,
    NextState, OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult, TypeId,
    validate_message_label, validate_state_value_identity_label,
};

#[derive(Debug, Clone)]
pub(crate) struct LoadedProgram {
    pub(crate) format: String,
    pub(crate) schema_version: String,
    pub(crate) source_language: String,
    pub(crate) module: String,
    pub(crate) entry_process: ProcessId,
    pub(crate) entry_message: MessageId,
    pub(crate) types: Vec<ArtifactType>,
    pub(crate) outputs: Vec<String>,
    pub(crate) processes: Vec<LoadedProcess>,
}

#[derive(Clone, Copy)]
struct LoadedTransitionValueTypes<'a> {
    received_payload: Option<TypeId>,
    current_state_payload: Option<&'a RuntimePayload>,
}

impl LoadedTransitionValueTypes<'_> {
    fn current_state_payload_type(self) -> Option<TypeId> {
        self.current_state_payload.map(|payload| payload.ty)
    }
}

impl LoadedProgram {
    pub(crate) fn from_artifact(artifact: &MantleArtifact) -> Result<Self> {
        artifact.validate()?;
        let processes = artifact
            .processes
            .iter()
            .map(LoadedProcess::from_artifact)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            format: artifact.format.clone(),
            schema_version: artifact.schema_version.clone(),
            source_language: artifact.source_language.clone(),
            module: artifact.module.clone(),
            entry_process: artifact.entry_process,
            entry_message: artifact.entry_message,
            types: artifact.types.clone(),
            outputs: artifact.outputs.clone(),
            processes,
        })
    }

    pub(crate) fn process(&self, id: ProcessId) -> Result<&LoadedProcess> {
        self.processes
            .get(id.index())
            .ok_or_else(|| Error::new(format!("process id {} is not loaded", id.as_u32())))
    }

    pub(crate) fn process_label(&self, id: ProcessId) -> Result<&str> {
        Ok(self.process(id)?.debug_name.as_str())
    }

    pub(crate) fn state_label(&self, process_id: ProcessId, state_id: StateId) -> Result<&str> {
        self.process(process_id)?
            .state_values
            .get(state_id.index())
            .map(|state| state.label.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "state id {} is not loaded for process id {}",
                    state_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn message_label(
        &self,
        process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<&str> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(|message| message.label.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    message_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn message_payload_type(
        &self,
        process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<Option<TypeId>> {
        self.process(process_id)?
            .message_variants
            .get(message_id.index())
            .map(|message| message.payload_type)
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    message_id.as_u32(),
                    process_id.as_u32()
                ))
            })
    }

    pub(crate) fn output(&self, output_id: OutputId) -> Result<&str> {
        self.outputs
            .get(output_id.index())
            .map(String::as_str)
            .ok_or_else(|| Error::new(format!("output id {} is not loaded", output_id.as_u32())))
    }

    pub(crate) fn type_entry(&self, ty: TypeId) -> Result<&ArtifactType> {
        self.types
            .get(ty.index())
            .ok_or_else(|| Error::new(format!("loaded type id {} is not loaded", ty.as_u32())))
    }

    pub(crate) fn type_label(&self, ty: TypeId) -> Result<&str> {
        Ok(self.type_entry(ty)?.label.as_str())
    }

    pub(crate) fn enum_variant_label(&self, ty: TypeId, variant: EnumVariantId) -> Result<&str> {
        let type_entry = self.type_entry(ty)?;
        let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
            return Err(Error::new(format!(
                "loaded type id {} is not an enum type",
                ty.as_u32()
            )));
        };
        variants
            .get(variant.index())
            .map(|variant| variant.label.as_str())
            .ok_or_else(|| {
                Error::new(format!(
                    "loaded type id {} has no enum variant id {}",
                    ty.as_u32(),
                    variant.as_u32()
                ))
            })
    }

    pub(crate) fn enum_variant_payload_type(
        &self,
        ty: TypeId,
        variant: EnumVariantId,
    ) -> Result<Option<TypeId>> {
        let type_entry = self.type_entry(ty)?;
        let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
            return Err(Error::new(format!(
                "loaded type id {} is not an enum type",
                ty.as_u32()
            )));
        };
        variants
            .get(variant.index())
            .map(|variant| variant.payload_type)
            .ok_or_else(|| {
                Error::new(format!(
                    "loaded type id {} has no enum variant id {}",
                    ty.as_u32(),
                    variant.as_u32()
                ))
            })
    }

    pub(crate) fn validate_value_type(&self, field: &str, ty: TypeId) -> Result<()> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::Value => Ok(()),
            ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
                "{field} type id {} must be a value type",
                ty.as_u32()
            ))),
        }
    }

    pub(crate) fn validate_value_matches_type(
        &self,
        field: &str,
        ty: TypeId,
        value: &RuntimeValue,
    ) -> Result<()> {
        self.validate_value_matches_type_at_depth(field, ty, value, 0)
    }

    pub(crate) fn validate_runtime_payload_matches_type(
        &self,
        field: &str,
        expected_type: TypeId,
        payload: &RuntimePayload,
    ) -> Result<()> {
        if payload.ty != expected_type {
            return Err(Error::new(format!(
                "{field} has type id {}, expected {}",
                payload.ty.as_u32(),
                expected_type.as_u32()
            )));
        }
        match self.type_entry(expected_type)?.kind {
            ArtifactTypeKind::Value => {
                if payload.process_ref.is_some() {
                    return Err(Error::new(format!(
                        "{field} must not carry process reference metadata"
                    )));
                }
                self.validate_value_matches_type(field, expected_type, &payload.value)
            }
            ArtifactTypeKind::ProcessRef { target } => {
                let Some(process_ref) = payload.process_ref else {
                    return Err(Error::new(format!(
                        "{field} requires process reference metadata"
                    )));
                };
                if process_ref.target_process != target {
                    return Err(Error::new(format!(
                        "{field} process reference metadata targets process id {}, expected {} for type id {}",
                        process_ref.target_process.as_u32(),
                        target.as_u32(),
                        expected_type.as_u32()
                    )));
                }
                RuntimePayload::validate_process_ref_value(field, payload)
            }
        }
    }

    pub(crate) fn runtime_payload_value(
        &self,
        field: &str,
        ty: TypeId,
        value: RuntimeValue,
    ) -> Result<RuntimePayload> {
        let payload = RuntimePayload::value(ty, value)?;
        self.validate_runtime_payload_matches_type(field, ty, &payload)?;
        Ok(payload)
    }

    pub(crate) fn process_ref_target_for_type_id(
        &self,
        field: &str,
        ty: TypeId,
    ) -> Result<ProcessId> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::ProcessRef { target } => {
                self.process(target)?;
                Ok(target)
            }
            ArtifactTypeKind::Value => Err(Error::new(format!(
                "{field} type id {} must be a process reference type",
                ty.as_u32()
            ))),
        }
    }

    pub(crate) fn validate_process_ref_type_id_target(
        &self,
        field: &str,
        ty: TypeId,
        target_process: ProcessId,
    ) -> Result<()> {
        let target = self.process_ref_target_for_type_id(field, ty)?;
        if target != target_process {
            return Err(Error::new(format!(
                "{field} type id {} targets process id {}, expected {}",
                ty.as_u32(),
                target.as_u32(),
                target_process.as_u32()
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_admission(&self) -> Result<()> {
        validate_loaded_artifact_identity(&self.format, &self.schema_version)?;
        validate_loaded_ident_field("source_language", &self.source_language)?;
        validate_loaded_ident_field("module", &self.module)?;

        if self.types.is_empty() || self.types.len() > MAX_TYPE_COUNT {
            return Err(Error::new(format!(
                "loaded type_count must be between 1 and {MAX_TYPE_COUNT}"
            )));
        }
        if self.processes.is_empty() || self.processes.len() > MAX_PROCESS_COUNT {
            return Err(Error::new(format!(
                "loaded process_count must be between 1 and {MAX_PROCESS_COUNT}"
            )));
        }
        if self.outputs.len() > MAX_OUTPUT_LITERALS {
            return Err(Error::new(format!(
                "loaded output_count must be no greater than {MAX_OUTPUT_LITERALS}"
            )));
        }
        for output in &self.outputs {
            validate_loaded_output_text(output)?;
        }
        for (type_index, ty) in self.types.iter().enumerate() {
            validate_loaded_ident_field(&format!("type.{type_index}.label"), &ty.label)?;
            self.validate_type_shape(type_index, ty)?;
        }

        let entry_process = self.process(self.entry_process)?;
        if self.entry_message.index() >= entry_process.message_variants.len() {
            return Err(Error::new(format!(
                "entry message id {} is not loaded for process id {}",
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

        let mut process_names = BTreeSet::new();
        for process in &self.processes {
            validate_loaded_ident_field("process debug_name", &process.debug_name)?;
            if !process_names.insert(process.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded process debug_name {:?}",
                    process.debug_name
                )));
            }
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            process.validate_admission(self, ProcessId::from_index(process_index)?)?;
        }
        Ok(())
    }

    fn validate_type_shape(&self, type_index: usize, ty: &ArtifactType) -> Result<()> {
        match ty.kind {
            ArtifactTypeKind::Value => {
                let Some(shape) = &ty.shape else {
                    return Err(Error::new(format!(
                        "loaded type.{type_index} value type must declare a value shape"
                    )));
                };
                self.validate_value_shape(type_index, shape)
            }
            ArtifactTypeKind::ProcessRef { target } => {
                if ty.shape.is_some() {
                    return Err(Error::new(format!(
                        "loaded type.{type_index} process reference type must not declare a value shape"
                    )));
                }
                self.process(target)?;
                Ok(())
            }
        }
    }

    fn validate_value_shape(&self, type_index: usize, shape: &ArtifactValueShape) -> Result<()> {
        match shape {
            ArtifactValueShape::Atom => Ok(()),
            ArtifactValueShape::Record { fields } => {
                if fields.is_empty() || fields.len() > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.field_count must be between 1 and {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                let mut seen = BTreeSet::new();
                for (field_index, field) in fields.iter().enumerate() {
                    validate_loaded_ident_field(
                        &format!("loaded type.{type_index}.field.{field_index}.name"),
                        &field.name,
                    )?;
                    if !seen.insert(field.name.as_str()) {
                        return Err(Error::new(format!(
                            "loaded type.{type_index} duplicates field {}",
                            field.name
                        )));
                    }
                    self.validate_value_type(
                        &format!("loaded type.{type_index}.field.{field_index}.type_id"),
                        field.ty,
                    )?;
                }
                Ok(())
            }
            ArtifactValueShape::Enum { variants } => {
                if variants.is_empty() || variants.len() > MAX_ENUM_VARIANTS_PER_TYPE {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.enum_variant_count must be between 1 and {MAX_ENUM_VARIANTS_PER_TYPE}"
                    )));
                }
                let mut seen = BTreeSet::new();
                for (variant_index, variant) in variants.iter().enumerate() {
                    validate_loaded_ident_field(
                        &format!("loaded type.{type_index}.enum_variant.{variant_index}"),
                        &variant.label,
                    )?;
                    if !seen.insert(variant.label.as_str()) {
                        return Err(Error::new(format!(
                            "loaded type.{type_index} duplicates enum variant {}",
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
                if *capacity > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                self.validate_value_type(
                    &format!("loaded type.{type_index}.element_type_id"),
                    *element,
                )
            }
            ArtifactValueShape::Map {
                key,
                value,
                capacity,
            } => {
                if *capacity > MAX_VALUE_TEMPLATE_FIELDS {
                    return Err(Error::new(format!(
                        "loaded type.{type_index}.capacity must be no greater than {MAX_VALUE_TEMPLATE_FIELDS}"
                    )));
                }
                self.validate_value_type(&format!("loaded type.{type_index}.key_type_id"), *key)?;
                self.validate_value_type(&format!("loaded type.{type_index}.value_type_id"), *value)
            }
        }
    }

    fn validate_value_matches_type_at_depth(
        &self,
        field: &str,
        ty: TypeId,
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum typed value depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        let type_entry = self.type_entry(ty)?;
        self.validate_value_type(field, ty)?;
        values::validate_non_process_ref_value(field, value)?;
        match type_entry.value_shape()? {
            ArtifactValueShape::Atom => self.validate_atom_value(field, ty, type_entry, value),
            ArtifactValueShape::Enum { variants } => {
                self.validate_enum_value(field, ty, type_entry, variants, value, depth)
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

    fn validate_atom_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        value: &RuntimeValue,
    ) -> Result<()> {
        if matches!(value, RuntimeValue::Atom(_)) {
            return Ok(());
        }
        Err(Error::new(format!(
            "{field} value {} does not match atom type {} (type id {})",
            value.label(),
            type_entry.label,
            ty.as_u32()
        )))
    }

    fn validate_enum_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        variants: &[ArtifactEnumVariant],
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        match value {
            RuntimeValue::Atom(label) => {
                let Some(variant) = variants.iter().find(|variant| variant.label == *label) else {
                    return Err(runtime_value_not_member_error(field, ty, type_entry, value));
                };
                if variant.payload_type.is_some() {
                    return Err(Error::new(format!(
                        "{field} enum variant {label} requires a payload"
                    )));
                }
                Ok(())
            }
            RuntimeValue::EnumVariant { variant, payload } => {
                let Some(entry) = variants.iter().find(|entry| entry.label == *variant) else {
                    return Err(runtime_value_not_member_error(field, ty, type_entry, value));
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
            RuntimeValue::Record { .. }
            | RuntimeValue::List(_)
            | RuntimeValue::Map(_)
            | RuntimeValue::ProcessRef { .. } => {
                Err(runtime_value_not_member_error(field, ty, type_entry, value))
            }
        }
    }

    fn validate_record_value(
        &self,
        field: &str,
        ty: TypeId,
        type_entry: &ArtifactType,
        expected_fields: &[ArtifactTypeField],
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::Record {
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
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::List(items) = value else {
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
        value: &RuntimeValue,
        depth: usize,
    ) -> Result<()> {
        let RuntimeValue::Map(entries) = value else {
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

fn runtime_value_not_member_error(
    field: &str,
    ty: TypeId,
    type_entry: &ArtifactType,
    value: &RuntimeValue,
) -> Error {
    Error::new(format!(
        "{field} value {} is not a member of enum type {} (type id {})",
        value.label(),
        type_entry.label,
        ty.as_u32()
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcess {
    pub(crate) debug_name: String,
    pub(crate) state_type: TypeId,
    pub(crate) state_values: Vec<LoadedStateValue>,
    pub(crate) message_variants: Vec<LoadedMessageVariant>,
    pub(crate) process_refs: Vec<LoadedProcessRef>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
    transition_lookup: TransitionLookup,
}

impl LoadedProcess {
    fn from_artifact(process: &ArtifactProcess) -> Result<Self> {
        let transitions = load_transitions(process)?;
        let transition_lookup = TransitionLookup::from_transitions(&transitions);

        Ok(Self {
            debug_name: process.debug_name.clone(),
            state_type: process.state_type,
            state_values: process
                .state_values
                .iter()
                .map(LoadedStateValue::from_artifact)
                .collect::<Result<Vec<_>>>()?,
            message_variants: process
                .message_variants
                .iter()
                .map(LoadedMessageVariant::from_artifact)
                .collect(),
            process_refs: process
                .process_refs
                .iter()
                .map(LoadedProcessRef::from_artifact)
                .collect(),
            mailbox_bound: process.mailbox_bound,
            init_state: process.init_state,
            transitions,
            transition_lookup,
        })
    }

    pub(crate) fn transition_for_dispatch(
        &self,
        message: MessageId,
        current_state: StateId,
        payload: Option<&RuntimePayload>,
    ) -> Result<&LoadedTransition> {
        let lookup_state = self
            .transition_lookup
            .is_state_specific_message(message)
            .then_some(current_state);
        let payload_specific = self
            .transition_lookup
            .is_payload_specific_base(message, lookup_state);
        let transition_index = self
            .transition_lookup
            .for_dispatch(message, current_state, payload)
            .ok_or_else(|| {
                self.transition_lookup_error(message, lookup_state, payload_specific, payload)
            })?;
        self.transition_by_lookup_index(transition_index)
    }

    fn transition_by_lookup_index(&self, index: usize) -> Result<&LoadedTransition> {
        self.transitions.get(index).ok_or_else(|| {
            Error::new(format!(
                "process {} transition index {} is not loaded",
                self.debug_name, index
            ))
        })
    }

    fn transition_lookup_error(
        &self,
        message: MessageId,
        current_state: Option<StateId>,
        payload_specific: bool,
        payload: Option<&RuntimePayload>,
    ) -> Error {
        let state = current_state
            .map(|state| format!(" current_state id {}", state.as_u32()))
            .unwrap_or_default();
        if payload_specific {
            return match payload {
                Some(payload) => Error::new(format!(
                    "process {} has no transition for message id {}{} payload {}",
                    self.debug_name,
                    message.as_u32(),
                    state,
                    payload.label()
                )),
                None => Error::new(format!(
                    "process {} has payload-specific transition(s) for message id {}{}, but the queued message has no payload",
                    self.debug_name,
                    message.as_u32(),
                    state
                )),
            };
        }
        Error::new(format!(
            "process {} has no transition for message id {}{}",
            self.debug_name,
            message.as_u32(),
            state
        ))
    }

    fn validate_admission(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        self.validate_state_table(program)?;
        self.validate_message_table(program)?;
        self.validate_process_refs(program, process_id)?;
        if self.mailbox_bound == 0 || self.mailbox_bound > MAX_MAILBOX_BOUND {
            return Err(Error::new(format!(
                "process {} loaded mailbox_bound must be between 1 and {MAX_MAILBOX_BOUND}",
                self.debug_name
            )));
        }
        if self.init_state.index() >= self.state_values.len() {
            return Err(Error::new(format!(
                "process {} init_state id {} is not a loaded state value",
                self.debug_name,
                self.init_state.as_u32()
            )));
        }
        if self.transitions.is_empty() || self.transitions.len() > MAX_TRANSITIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded transition_count must be between 1 and {MAX_TRANSITIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let action_count = self
            .transitions
            .iter()
            .try_fold(0usize, |count, transition| {
                count
                    .checked_add(actions::action_count(&transition.actions)?)
                    .ok_or_else(|| Error::new("loaded action_count overflowed"))
            })?;
        if action_count > MAX_ACTIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}",
                self.debug_name
            )));
        }

        validate_loaded_transition_coverage(self)?;
        for transition in &self.transitions {
            let message = transition.message;
            transition.validate_admission(program, self, message)?;
            transition.effect_authority.validate_actions(
                &self.debug_name,
                message,
                &transition.actions,
            )?;
        }
        Ok(())
    }

    fn validate_state_table(&self, program: &LoadedProgram) -> Result<()> {
        validate_loaded_ident_field("process debug_name", &self.debug_name)?;
        program.validate_value_type("state_type", self.state_type)?;
        if self.state_values.is_empty() || self.state_values.len() > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded state_value_count must be between 1 and {MAX_STATE_VALUES_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut states = BTreeSet::new();
        for state in &self.state_values {
            program
                .validate_value_type("state value type", state.ty)
                .map_err(|err| {
                    Error::new(format!(
                        "process {} state value type: {err}",
                        self.debug_name
                    ))
                })?;
            program
                .validate_value_matches_type("state value", state.ty, &state.value)
                .map_err(|err| {
                    Error::new(format!("process {} state value: {err}", self.debug_name))
                })?;
            validate_state_value_identity_label(&state.value, &state.label)
                .map_err(|err| Error::new(format!("process {} {err}", self.debug_name)))?;
            if state.ty != self.state_type {
                return Err(Error::new(format!(
                    "process {} loaded state value {} has type id {}, expected {}",
                    self.debug_name,
                    state.label,
                    state.ty.as_u32(),
                    self.state_type.as_u32()
                )));
            }
            if let Some(payload) = &state.payload {
                program
                    .validate_value_type("state value payload type", payload.ty)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload type: {err}",
                            self.debug_name
                        ))
                    })?;
                if payload.process_ref.is_some() || payload.value.contains_process_ref() {
                    return Err(Error::new(format!(
                        "process {} state value {} carries a process reference payload",
                        self.debug_name, state.label
                    )));
                }
                program
                    .validate_value_matches_type("state value payload", payload.ty, &payload.value)
                    .map_err(|err| {
                        Error::new(format!(
                            "process {} state value payload: {err}",
                            self.debug_name
                        ))
                    })?;
            }
            if !states.insert((state.ty, state.value.clone())) {
                return Err(Error::new(format!(
                    "process {} loads duplicate state value {} with type id {}",
                    self.debug_name,
                    state.value.label(),
                    state.ty.as_u32()
                )));
            }
        }
        Ok(())
    }

    fn validate_message_table(&self, program: &LoadedProgram) -> Result<()> {
        if self.message_variants.is_empty()
            || self.message_variants.len() > MAX_MESSAGE_VARIANTS_PER_PROCESS
        {
            return Err(Error::new(format!(
                "process {} loaded message_count must be between 1 and {MAX_MESSAGE_VARIANTS_PER_PROCESS}",
                self.debug_name
            )));
        }

        let mut messages = BTreeSet::new();
        for message in &self.message_variants {
            validate_message_label(&message.label).map_err(|err| {
                Error::new(format!("process {} message label: {err}", self.debug_name))
            })?;
            if let Some(payload_type) = message.payload_type {
                program.type_entry(payload_type).map_err(|err| {
                    Error::new(format!(
                        "process {} message payload_type: {err}",
                        self.debug_name
                    ))
                })?;
            }
            if !messages.insert(message.label.as_str()) {
                return Err(Error::new(format!(
                    "process {} loads duplicate message label {}",
                    self.debug_name, message.label
                )));
            }
        }
        Ok(())
    }

    fn validate_process_refs(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
        if self.process_refs.len() > MAX_PROCESS_REFS_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} loaded process_ref_count must be no greater than {MAX_PROCESS_REFS_PER_PROCESS}",
                self.debug_name
            )));
        }

        for (process_ref_index, process_ref) in self.process_refs.iter().enumerate() {
            program.process(process_ref.target)?;
            if process_ref.target == program.entry_process {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets entry process id {}",
                    self.debug_name,
                    process_ref_index,
                    process_ref.target.as_u32()
                )));
            }
            if process_ref.target == process_id {
                return Err(Error::new(format!(
                    "process {} process reference id {} targets itself",
                    self.debug_name, process_ref_index
                )));
            }
        }
        Ok(())
    }

    fn process_ref_target(&self, process_ref: ProcessRefId) -> Result<ProcessId> {
        self.process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references unloaded process reference id {}",
                    self.debug_name,
                    process_ref.as_u32()
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedMessageVariant {
    pub(crate) label: String,
    pub(crate) payload_type: Option<TypeId>,
}

impl LoadedMessageVariant {
    fn from_artifact(message: &ArtifactMessageVariant) -> Self {
        Self {
            label: message.label.clone(),
            payload_type: message.payload_type,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcessRef {
    pub(crate) target: ProcessId,
}

impl LoadedProcessRef {
    fn from_artifact(process_ref: &ArtifactProcessRef) -> Self {
        Self {
            target: process_ref.target,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedTransition {
    pub(crate) current_state: Option<StateId>,
    pub(crate) message: MessageId,
    pub(crate) payload_guard: Option<RuntimePayload>,
    pub(crate) step_result: StepResult,
    pub(crate) next_state: LoadedNextState,
    pub(crate) effect_authority: LoadedEffectAuthority,
    pub(crate) actions: Vec<LoadedAction>,
}

impl LoadedTransition {
    fn from_artifact(transition: &ArtifactTransition) -> Result<Self> {
        Ok(Self {
            current_state: transition.current_state,
            message: transition.message,
            payload_guard: transition
                .payload_guard
                .as_ref()
                .map(RuntimePayload::from_artifact)
                .transpose()?,
            step_result: transition.step_result,
            next_state: LoadedNextState::from_artifact(&transition.next_state)?,
            effect_authority: LoadedEffectAuthority::from_artifact(&transition.effects),
            actions: transition
                .actions
                .iter()
                .map(LoadedAction::from_artifact)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn validate_admission(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        self.validate_next_state(program, process, message)?;
        self.validate_payload_guard(program, process, message)?;

        let current_state_payload = transition_current_state_payload(process, self)?;
        let mut spawned_refs = vec![false; process.process_refs.len()];
        for action in &self.actions {
            action.validate_admission(
                program,
                process,
                message,
                current_state_payload,
                &mut spawned_refs,
            )?;
        }
        Ok(())
    }

    fn validate_payload_guard(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let Some(payload_guard) = &self.payload_guard else {
            return Ok(());
        };
        if payload_guard.process_ref.is_some() || payload_guard.value.contains_process_ref() {
            return Err(Error::new(format!(
                "process {} message id {} payload guard cannot be a process reference payload",
                process.debug_name,
                message.as_u32()
            )));
        }
        let message_variant = process
            .message_variants
            .get(message.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} is not loaded",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        let payload_type = message_variant
            .payload_type
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} message id {} has a payload guard but the message does not accept a payload",
                    process.debug_name,
                    message.as_u32()
                ))
            })?;
        if payload_guard.ty != payload_type {
            return Err(Error::new(format!(
                "process {} message id {} payload guard has type id {}, expected {}",
                process.debug_name,
                message.as_u32(),
                payload_guard.ty.as_u32(),
                payload_type.as_u32()
            )));
        }
        program.validate_value_matches_type(
            &format!(
                "process {} message id {} payload guard",
                process.debug_name,
                message.as_u32()
            ),
            payload_guard.ty,
            &payload_guard.value,
        )?;
        Ok(())
    }

    fn validate_next_state(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        message: MessageId,
    ) -> Result<()> {
        let context = self.transition_context(message);
        let value_types = LoadedTransitionValueTypes {
            received_payload: process
                .message_variants
                .get(message.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} message id {} is not loaded",
                        process.debug_name,
                        message.as_u32()
                    ))
                })?
                .payload_type,
            current_state_payload: transition_current_state_payload(process, self)?,
        };
        self.validate_next_state_node(program, process, &context, &self.next_state, value_types, 0)
    }

    fn validate_next_state_node(
        &self,
        program: &LoadedProgram,
        process: &LoadedProcess,
        context: &str,
        next_state: &LoadedNextState,
        value_types: LoadedTransitionValueTypes,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "process {} {} next_state nesting exceeds maximum depth {}",
                process.debug_name, context, MAX_VALUE_TEMPLATE_DEPTH
            )));
        }
        match next_state {
            LoadedNextState::Current => Ok(()),
            LoadedNextState::Value(state) => {
                if state.index() >= process.state_values.len() {
                    return Err(Error::new(format!(
                        "process {} {} next_state id {} is not a loaded state value",
                        process.debug_name,
                        context,
                        state.as_u32()
                    )));
                }
                Ok(())
            }
            LoadedNextState::Template(template) => {
                LoadedTemplateAdmission {
                    expected_type: Some(process.state_type),
                    received_payload_type: value_types.received_payload,
                    current_state_payload_type: value_types.current_state_payload_type(),
                    allow_direct_process_ref: false,
                    loop_elements: &[],
                    program,
                    process,
                    spawned_refs: &[],
                }
                .validate(
                    &format!(
                        "process {} {} next_state_template",
                        process.debug_name, context
                    ),
                    template,
                )?;
                if loaded_template_depends_on_received_payload(template) {
                    return Ok(());
                }
                let value = evaluate_loaded_state_value(
                    program,
                    template,
                    None,
                    value_types.current_state_payload,
                )?;
                if process.state_values.iter().any(|state_value| {
                    state_value.ty == value.ty && state_value.value == value.value
                }) {
                    return Ok(());
                }
                Err(Error::new(format!(
                    "process {} {} next_state_template produced value {} not admitted by loaded state table",
                    process.debug_name, context, value.label
                )))
            }
            LoadedNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                validate_loaded_bool_condition(
                    program,
                    process,
                    &format!(
                        "process {} {} next_state_condition",
                        process.debug_name, context
                    ),
                    condition,
                    value_types.received_payload,
                    value_types.current_state_payload,
                )?;
                self.validate_next_state_node(
                    program,
                    process,
                    &format!("{context} then"),
                    then_state,
                    value_types,
                    depth + 1,
                )?;
                self.validate_next_state_node(
                    program,
                    process,
                    &format!("{context} else"),
                    else_state,
                    value_types,
                    depth + 1,
                )
            }
        }
    }

    fn transition_context(&self, message: MessageId) -> String {
        match self.current_state {
            Some(current_state) => format!(
                "message id {} current_state id {}",
                message.as_u32(),
                current_state.as_u32()
            ),
            None => format!("message id {}", message.as_u32()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadedNextState {
    Current,
    Value(StateId),
    Template(LoadedValueTemplate),
    IfElse {
        condition: LoadedValueTemplate,
        then_state: Box<LoadedNextState>,
        else_state: Box<LoadedNextState>,
    },
}

impl LoadedNextState {
    pub(crate) fn from_artifact(next_state: &NextState) -> Result<Self> {
        match next_state {
            NextState::Current => Ok(Self::Current),
            NextState::Value(state) => Ok(Self::Value(*state)),
            NextState::Template(template) => Ok(Self::Template(
                LoadedValueTemplate::from_artifact(template)?,
            )),
            NextState::IfElse {
                condition,
                then_state,
                else_state,
            } => Ok(Self::IfElse {
                condition: LoadedValueTemplate::from_artifact(condition)?,
                then_state: Box::new(LoadedNextState::from_artifact(then_state)?),
                else_state: Box::new(LoadedNextState::from_artifact(else_state)?),
            }),
        }
    }
}

fn transition_current_state_payload<'a>(
    process: &'a LoadedProcess,
    transition: &LoadedTransition,
) -> Result<Option<&'a RuntimePayload>> {
    let Some(current_state) = transition.current_state else {
        return Ok(None);
    };
    let state_value = process
        .state_values
        .get(current_state.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} message id {} current_state id {} is not a loaded state value",
                process.debug_name,
                transition.message.as_u32(),
                current_state.as_u32()
            ))
        })?;
    Ok(state_value.payload.as_ref())
}
