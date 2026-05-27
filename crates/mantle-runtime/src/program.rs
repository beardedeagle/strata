use std::collections::BTreeSet;

pub(in crate::program) use actions::ActionAdmissionContext;
pub(crate) use actions::{LoadedAction, LoadedLoopElement, LoadedSendTarget};
use admission::{
    validate_loaded_artifact_identity, validate_loaded_ident_field, validate_loaded_output_text,
};
pub(crate) use effects::LoadedEffectAuthority;
use transitions::{TransitionLookup, load_transitions, validate_loaded_transition_coverage};
pub use values::RuntimePayload;
pub(crate) use values::{
    LoadedStateValue, LoadedValueTemplate, LoadedValueTemplateField, LoadedValueTemplateMapEntry,
    RuntimeValue,
};

mod actions;
mod admission;
mod effects;
mod processes;
mod templates;
mod transition_admission;
mod transitions;
mod type_validation;
mod values;

use mantle_artifact::{
    ArtifactAction, ArtifactAuthority, ArtifactCapabilityDescriptor, ArtifactEnumVariant,
    ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef, ArtifactScalarType,
    ArtifactSendTarget, ArtifactSpawnKind, ArtifactSpawnSite, ArtifactTransition, ArtifactType,
    ArtifactTypeField, ArtifactTypeKind, ArtifactValueShape, AuthorityId, EffectOutcomeId,
    EnumVariantId, Error, LoopElementId, MAX_ACTIONS_PER_PROCESS, MAX_AUTHORITIES_PER_PROCESS,
    MAX_EFFECT_OUTCOMES_PER_TRANSITION, MAX_ENUM_VARIANTS_PER_TYPE, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_NEXT_STATE_IF_ELSE_DEPTH, MAX_OUTPUT_LITERALS,
    MAX_PROCESS_COUNT, MAX_PROCESS_REFS_PER_PROCESS, MAX_SPAWN_SITES_PER_PROCESS,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS, MAX_TYPE_COUNT,
    MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact, MessageId, NextState,
    OutputId, ProcessId, ProcessRefId, Result, SpawnSiteId, StateId, StepResult, TypeId,
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
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedProcess {
    pub(crate) debug_name: String,
    pub(crate) state_type: TypeId,
    pub(crate) state_values: Vec<LoadedStateValue>,
    pub(crate) message_type: TypeId,
    pub(crate) message_variants: Vec<LoadedMessageVariant>,
    pub(crate) authorities: Vec<LoadedAuthority>,
    pub(crate) spawn_sites: Vec<LoadedSpawnSite>,
    pub(crate) process_refs: Vec<LoadedProcessRef>,
    pub(crate) mailbox_bound: usize,
    pub(crate) init_state: StateId,
    pub(crate) transitions: Vec<LoadedTransition>,
    transition_lookup: TransitionLookup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedAuthority {
    pub(crate) debug_name: String,
    pub(crate) descriptor: LoadedCapabilityDescriptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoadedCapabilityDescriptor {
    Spawn { target: ProcessId },
}

impl LoadedAuthority {
    fn from_artifact(authority: &ArtifactAuthority) -> Self {
        Self {
            debug_name: authority.debug_name.clone(),
            descriptor: LoadedCapabilityDescriptor::from_artifact(authority.descriptor),
        }
    }
}

impl LoadedCapabilityDescriptor {
    fn from_artifact(descriptor: ArtifactCapabilityDescriptor) -> Self {
        match descriptor {
            ArtifactCapabilityDescriptor::Spawn { target } => Self::Spawn { target },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadedSpawnKind {
    DynamicLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedSpawnSite {
    pub(crate) target: ProcessId,
    pub(crate) authority: AuthorityId,
    pub(crate) kind: LoadedSpawnKind,
}

impl LoadedSpawnSite {
    fn from_artifact(spawn_site: &ArtifactSpawnSite) -> Self {
        Self {
            target: spawn_site.target,
            authority: spawn_site.authority,
            kind: match spawn_site.kind {
                ArtifactSpawnKind::DynamicLocal => LoadedSpawnKind::DynamicLocal,
            },
        }
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
