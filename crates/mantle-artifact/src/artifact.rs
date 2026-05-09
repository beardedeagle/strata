use std::collections::BTreeSet;
use std::fmt;

use crate::validation::{
    validate_count, validate_encoded_artifact_size, validate_ident_field, validate_output_text,
    validate_source_hash, validate_unique_message_variant_list, validate_unique_state_value_list,
    validate_value_label,
};
mod codec;
mod process_validation;
mod value_template;

pub use value_template::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactValueTemplate, ArtifactValueTemplateField,
};

use crate::{
    ARTIFACT_FORMAT, ARTIFACT_MAGIC, ARTIFACT_SCHEMA_VERSION, Error, MAX_ACTIONS_PER_PROCESS,
    MAX_EFFECTS_PER_TRANSITION, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT, MAX_PROCESS_REFS_PER_PROCESS,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS, MAX_TYPE_COUNT,
    MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MessageId, OutputId, ProcessId,
    ProcessRefId, Result, StateId, TypeId,
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
}

impl NextState {
    pub(crate) fn kind_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Value(_) => "value",
            Self::Template(_) => "template",
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
}

impl ArtifactType {
    pub fn value(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
        }
    }

    pub fn process_ref(label: impl Into<String>, target: ProcessId) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::ProcessRef { target },
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

    pub fn validate_value_type(&self, field: &str, ty: TypeId) -> Result<()> {
        match self.type_entry(ty)?.kind {
            ArtifactTypeKind::Value => Ok(()),
            ArtifactTypeKind::ProcessRef { .. } => Err(Error::new(format!(
                "artifact field {field} type id {} must be a value type",
                ty.as_u32()
            ))),
        }
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
            self.type_label(ty).map(str::to_owned)
        })
    }

    fn validate_type_table(&self) -> Result<()> {
        for (type_index, ty) in self.types.iter().enumerate() {
            validate_ident_field(&format!("type.{type_index}.label"), &ty.label)?;
            if let ArtifactTypeKind::ProcessRef { target } = ty.kind {
                if target.index() >= self.processes.len() {
                    return Err(Error::new(format!(
                        "type id {type_index} targets undefined process id {}",
                        target.as_u32()
                    )));
                }
            }
        }
        Ok(())
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
    pub value: String,
    pub label: String,
    pub payload: Option<ArtifactPayload>,
}

impl ArtifactStateValue {
    pub fn new(ty: TypeId, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            ty,
            label: value.clone(),
            value,
            payload: None,
        }
    }

    pub fn with_label(ty: TypeId, value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            ty,
            value: value.into(),
            label: label.into(),
            payload: None,
        }
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
    pub step_result: StepResult,
    pub next_state: NextState,
    pub effects: Vec<ArtifactEffect>,
    pub actions: Vec<ArtifactAction>,
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
}

impl ArtifactAction {
    fn effect(&self) -> ArtifactEffect {
        match self {
            Self::Emit { .. } => ArtifactEffect::Emit,
            Self::Spawn { .. } => ArtifactEffect::Spawn,
            Self::Send { .. } => ArtifactEffect::Send,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSendTarget {
    ProcessRef(ProcessRefId),
    ReceivedPayload {
        ty: TypeId,
        target_process: ProcessId,
    },
}
