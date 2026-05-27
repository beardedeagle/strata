use std::collections::BTreeSet;
use std::fmt;

use crate::validation::{
    validate_count, validate_encoded_artifact_size, validate_ident_field, validate_output_text,
    validate_source_hash, validate_state_value_identity_label,
    validate_unique_message_variant_list, validate_unique_state_value_list,
};
mod codec;
mod process_validation;
mod validation;
mod value_template;

pub use validation::validate_value_enum_membership;
pub(in crate::artifact) use validation::{
    validate_artifact_identity, validate_unique_process_ref_list,
};

pub use value_template::{
    ArtifactMapEntry, ArtifactPayload, ArtifactProcessRefPayload, ArtifactRecordField,
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactScalarType,
    ArtifactScalarValue, ArtifactValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, MapProjectionMode,
};

use crate::{
    ARTIFACT_FORMAT, ARTIFACT_MAGIC, ARTIFACT_SCHEMA_VERSION, AuthorityId, EffectOutcomeId,
    EnumVariantId, Error, LoopElementId, MAX_ACTIONS_PER_PROCESS, MAX_EFFECTS_PER_TRANSITION,
    MAX_ENUM_VARIANTS_PER_TYPE, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_OUTPUT_LITERALS, MAX_PROCESS_COUNT, MAX_PROCESS_REFS_PER_PROCESS,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TRANSITIONS_PER_PROCESS, MAX_TYPE_COUNT,
    MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, MessageId, OutputId, ProcessId,
    ProcessRefId, Result, SpawnSiteId, StateId, TypeId,
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
    Scalar {
        scalar: ArtifactScalarType,
    },
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

    pub fn scalar(label: impl Into<String>, scalar: ArtifactScalarType) -> Self {
        Self {
            label: label.into(),
            kind: ArtifactTypeKind::Value,
            shape: Some(ArtifactValueShape::Scalar { scalar }),
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
    pub authorities: Vec<ArtifactAuthority>,
    pub spawn_sites: Vec<ArtifactSpawnSite>,
    pub process_refs: Vec<ArtifactProcessRef>,
    pub mailbox_bound: usize,
    pub init_state: StateId,
    pub transitions: Vec<ArtifactTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAuthority {
    pub debug_name: String,
    pub descriptor: ArtifactCapabilityDescriptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactCapabilityDescriptor {
    Spawn { target: ProcessId },
}

impl ArtifactCapabilityDescriptor {
    pub(crate) const fn kind_str(self) -> &'static str {
        match self {
            Self::Spawn { .. } => "spawn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactSpawnKind {
    DynamicLocal,
}

impl ArtifactSpawnKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DynamicLocal => "dynamic_local",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "dynamic_local" => Ok(Self::DynamicLocal),
            _ => Err(Error::new(format!("invalid spawn_kind value {value:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSpawnSite {
    pub target: ProcessId,
    pub authority: AuthorityId,
    pub kind: ArtifactSpawnKind,
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
        spawn_site: SpawnSiteId,
    },
    SpawnOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
        target: ProcessId,
        spawn_site: SpawnSiteId,
    },
    Send {
        target: ArtifactSendTarget,
        message: MessageId,
        payload: Option<ArtifactValueTemplate>,
    },
    SendOutcome {
        outcome: EffectOutcomeId,
        outcome_ty: TypeId,
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
            Self::Spawn { .. } | Self::SpawnOutcome { .. } => {
                effects.insert(ArtifactEffect::Spawn);
            }
            Self::Send { .. } | Self::SendOutcome { .. } => {
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
            Self::Emit { .. }
            | Self::Spawn { .. }
            | Self::SpawnOutcome { .. }
            | Self::Send { .. }
            | Self::SendOutcome { .. } => Ok(1),
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
