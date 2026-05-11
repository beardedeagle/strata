use std::fmt;

use mantle_artifact::{ArtifactValue, MapProjectionMode, validate_message_label};

use super::ast::{Effect, Identifier, Module};
use super::diagnostic::{Error, Result};

macro_rules! define_checked_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(in crate::language) struct $name(u32);

        impl $name {
            pub(in crate::language) fn from_index(index: usize) -> Result<Self> {
                let value = u32::try_from(index).map_err(|_| {
                    Error::new(format!("{} index {index} is too large", stringify!($name)))
                })?;
                Ok(Self(value))
            }

            pub(in crate::language) const fn as_u32(self) -> u32 {
                self.0
            }
        }
    };
}

define_checked_id!(CheckedProcessId);
define_checked_id!(CheckedProcessRefId);
define_checked_id!(CheckedMessageVariantId);
define_checked_id!(CheckedStateId);
define_checked_id!(CheckedMessageId);
define_checked_id!(CheckedOutputId);
define_checked_id!(CheckedTypeId);

impl CheckedProcessId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedMessageId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedMessageVariantId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedProcessRefId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedStateId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

impl CheckedTypeId {
    pub(in crate::language) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedTypeKind {
    Value,
    ProcessRef { target: CheckedProcessId },
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

    pub(in crate::language) fn kind(&self) -> CheckedTypeKind {
        self.kind
    }

    #[cfg(test)]
    pub(in crate::language) fn test_value(label: &str) -> Self {
        Self::new(
            test_type_id(label, None),
            label.to_string(),
            CheckedTypeKind::Value,
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
    CheckedTypeId(hash)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedStepResult {
    Continue,
    Stop,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedNextState {
    Current,
    Value(CheckedStateId),
    Template(CheckedValueTemplate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedPayloadValue {
    ty: CheckedTypeRef,
    value: Option<ArtifactValue>,
    label: String,
    process_ref: Option<CheckedProcessRefPayload>,
}

impl CheckedPayloadValue {
    pub(in crate::language) fn new(ty: CheckedTypeRef, value: ArtifactValue) -> Self {
        let label = value.label();
        Self {
            ty,
            value: Some(value),
            label,
            process_ref: None,
        }
    }

    pub(in crate::language) fn process_ref(
        ty: CheckedTypeRef,
        label: String,
        target: CheckedProcessId,
        pid: u64,
    ) -> Self {
        Self {
            ty,
            value: None,
            label,
            process_ref: Some(CheckedProcessRefPayload { target, pid }),
        }
    }

    pub(in crate::language) fn ty(&self) -> &CheckedTypeRef {
        &self.ty
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn value(&self) -> Option<&ArtifactValue> {
        self.value.as_ref()
    }

    pub(in crate::language) fn process_ref_payload(&self) -> Option<CheckedProcessRefPayload> {
        self.process_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedStateValue {
    ty: CheckedTypeRef,
    value: ArtifactValue,
    label: String,
    payload: Option<CheckedPayloadValue>,
}

impl CheckedStateValue {
    pub(in crate::language) fn new(ty: CheckedTypeRef, value: ArtifactValue) -> Self {
        let label = value.label();
        Self {
            ty,
            value,
            label,
            payload: None,
        }
    }

    pub(in crate::language) fn enum_variant(
        ty: CheckedTypeRef,
        value: ArtifactValue,
        payload: Option<CheckedPayloadValue>,
    ) -> Self {
        let label = value.label();
        Self {
            ty,
            value,
            label,
            payload,
        }
    }

    pub(in crate::language) fn ty(&self) -> &CheckedTypeRef {
        &self.ty
    }

    pub(in crate::language) fn value(&self) -> &ArtifactValue {
        &self.value
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn payload(&self) -> Option<&CheckedPayloadValue> {
        self.payload.as_ref()
    }

    pub(in crate::language) fn has_same_identity_as_payload(
        &self,
        payload: &CheckedPayloadValue,
    ) -> bool {
        self.ty == *payload.ty()
            && payload
                .value()
                .is_some_and(|payload_value| &self.value == payload_value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcessRefPayload {
    target: CheckedProcessId,
    pid: u64,
}

impl CheckedProcessRefPayload {
    pub(in crate::language) fn target(self) -> CheckedProcessId {
        self.target
    }

    pub(in crate::language) fn pid(self) -> u64 {
        self.pid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedValueTemplate {
    Literal(CheckedPayloadValue),
    ReceivedPayload {
        ty: CheckedTypeRef,
    },
    CurrentStatePayload {
        ty: CheckedTypeRef,
    },
    RecordField {
        ty: CheckedTypeRef,
        record: Box<CheckedValueTemplate>,
        field: Identifier,
    },
    ListElement {
        ty: CheckedTypeRef,
        list: Box<CheckedValueTemplate>,
        index: usize,
        len: usize,
    },
    MapValue {
        ty: CheckedTypeRef,
        map: Box<CheckedValueTemplate>,
        key: ArtifactValue,
        keys: Vec<ArtifactValue>,
        projection: MapProjectionMode,
    },
    ProcessRef {
        ty: CheckedTypeRef,
        target: CheckedProcessId,
        process_ref: CheckedProcessRefId,
    },
    EnumVariant {
        ty: CheckedTypeRef,
        variant: Identifier,
        payload: Box<CheckedValueTemplate>,
    },
    Record {
        ty: CheckedTypeRef,
        fields: Vec<CheckedValueTemplateField>,
    },
    List {
        ty: CheckedTypeRef,
        items: Vec<CheckedValueTemplate>,
    },
    Map {
        ty: CheckedTypeRef,
        entries: Vec<CheckedValueTemplateMapEntry>,
    },
}

impl CheckedValueTemplate {
    pub(in crate::language) fn result_type(&self) -> &CheckedTypeRef {
        match self {
            Self::Literal(value) => value.ty(),
            Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::RecordField { ty, .. }
            | Self::ListElement { ty, .. }
            | Self::MapValue { ty, .. }
            | Self::ProcessRef { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. }
            | Self::List { ty, .. }
            | Self::Map { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedValueTemplateField {
    name: Identifier,
    value: CheckedValueTemplate,
}

impl CheckedValueTemplateField {
    pub(in crate::language) fn new(name: Identifier, value: CheckedValueTemplate) -> Self {
        Self { name, value }
    }

    pub(in crate::language) fn name(&self) -> &Identifier {
        &self.name
    }

    pub(in crate::language) fn value(&self) -> &CheckedValueTemplate {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedValueTemplateMapEntry {
    key: CheckedValueTemplate,
    value: CheckedValueTemplate,
}

impl CheckedValueTemplateMapEntry {
    pub(in crate::language) fn new(key: CheckedValueTemplate, value: CheckedValueTemplate) -> Self {
        Self { key, value }
    }

    pub(in crate::language) fn key(&self) -> &CheckedValueTemplate {
        &self.key
    }

    pub(in crate::language) fn value(&self) -> &CheckedValueTemplate {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedMessageCase {
    label: String,
    variant: CheckedMessageVariantId,
    payload_type: Option<CheckedTypeRef>,
}

impl CheckedMessageCase {
    pub(in crate::language) fn new(
        label: String,
        variant: CheckedMessageVariantId,
        payload_type: Option<CheckedTypeRef>,
    ) -> Result<Self> {
        validate_message_label(&label).map_err(|err| Error::new(err.to_string()))?;
        Ok(Self {
            label,
            variant,
            payload_type,
        })
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn variant(&self) -> CheckedMessageVariantId {
        self.variant
    }

    pub(in crate::language) fn payload_type(&self) -> Option<&CheckedTypeRef> {
        self.payload_type.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcessRef {
    debug_name: Identifier,
    target: CheckedProcessId,
}

impl CheckedProcessRef {
    pub(in crate::language) fn new(debug_name: Identifier, target: CheckedProcessId) -> Self {
        Self { debug_name, target }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn target(&self) -> CheckedProcessId {
        self.target
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedAction {
    Emit {
        output: CheckedOutputId,
    },
    Spawn {
        target: CheckedProcessId,
        process_ref: CheckedProcessRefId,
    },
    Send {
        target: CheckedSendTarget,
        message: CheckedMessageId,
        payload: Option<CheckedValueTemplate>,
    },
}

impl CheckedAction {
    pub(in crate::language) fn effect(&self) -> Effect {
        match self {
            Self::Emit { .. } => Effect::Emit,
            Self::Spawn { .. } => Effect::Spawn,
            Self::Send { .. } => Effect::Send,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedSendTarget {
    ProcessRef(CheckedProcessRefId),
    ReceivedPayload {
        ty: CheckedTypeRef,
        target: CheckedProcessId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedTransition {
    current_state: Option<CheckedStateId>,
    message: CheckedMessageId,
    step_result: CheckedStepResult,
    next_state: CheckedNextState,
    effects: Vec<Effect>,
    actions: Vec<CheckedAction>,
}

impl CheckedTransition {
    pub(in crate::language) fn new(parts: CheckedTransitionParts) -> Self {
        Self {
            current_state: parts.current_state,
            message: parts.message,
            step_result: parts.step_result,
            next_state: parts.next_state,
            effects: parts.effects,
            actions: parts.actions,
        }
    }

    pub(in crate::language) fn current_state(&self) -> Option<CheckedStateId> {
        self.current_state
    }

    pub(in crate::language) fn message(&self) -> CheckedMessageId {
        self.message
    }

    pub(in crate::language) fn step_result(&self) -> CheckedStepResult {
        self.step_result
    }

    pub(in crate::language) fn next_state(&self) -> CheckedNextState {
        self.next_state.clone()
    }

    pub(in crate::language) fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub(in crate::language) fn actions(&self) -> &[CheckedAction] {
        &self.actions
    }
}

pub(in crate::language) struct CheckedTransitionParts {
    pub current_state: Option<CheckedStateId>,
    pub message: CheckedMessageId,
    pub step_result: CheckedStepResult,
    pub next_state: CheckedNextState,
    pub effects: Vec<Effect>,
    pub actions: Vec<CheckedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcess {
    debug_name: Identifier,
    state_type: CheckedTypeRef,
    state_values: Vec<CheckedStateValue>,
    message_type: CheckedTypeRef,
    message_cases: Vec<CheckedMessageCase>,
    process_refs: Vec<CheckedProcessRef>,
    mailbox_bound: usize,
    init_state: CheckedStateId,
    transitions: Vec<CheckedTransition>,
}

impl CheckedProcess {
    pub(in crate::language) fn new(parts: CheckedProcessParts) -> Self {
        Self {
            debug_name: parts.debug_name,
            state_type: parts.state_type,
            state_values: parts.state_values,
            message_type: parts.message_type,
            message_cases: parts.message_cases,
            process_refs: parts.process_refs,
            mailbox_bound: parts.mailbox_bound,
            init_state: parts.init_state,
            transitions: parts.transitions,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn state_type(&self) -> &CheckedTypeRef {
        &self.state_type
    }

    pub(in crate::language) fn state_values(&self) -> &[CheckedStateValue] {
        &self.state_values
    }

    pub(in crate::language) fn message_type(&self) -> &CheckedTypeRef {
        &self.message_type
    }

    pub(in crate::language) fn message_cases(&self) -> &[CheckedMessageCase] {
        &self.message_cases
    }

    pub(in crate::language) fn process_refs(&self) -> &[CheckedProcessRef] {
        &self.process_refs
    }

    pub(in crate::language) fn mailbox_bound(&self) -> usize {
        self.mailbox_bound
    }

    pub(in crate::language) fn init_state(&self) -> CheckedStateId {
        self.init_state
    }

    pub(in crate::language) fn transitions(&self) -> &[CheckedTransition] {
        &self.transitions
    }
}

pub(in crate::language) struct CheckedProcessParts {
    pub debug_name: Identifier,
    pub state_type: CheckedTypeRef,
    pub state_values: Vec<CheckedStateValue>,
    pub message_type: CheckedTypeRef,
    pub message_cases: Vec<CheckedMessageCase>,
    pub process_refs: Vec<CheckedProcessRef>,
    pub mailbox_bound: usize,
    pub init_state: CheckedStateId,
    pub transitions: Vec<CheckedTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    module: Module,
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
    types: Vec<CheckedTypeRef>,
    outputs: Vec<String>,
    processes: Vec<CheckedProcess>,
}

impl CheckedProgram {
    pub(in crate::language) fn new(parts: CheckedProgramParts) -> Self {
        Self {
            module: parts.module,
            entry_process: parts.entry_process,
            entry_message: parts.entry_message,
            types: parts.types,
            outputs: parts.outputs,
            processes: parts.processes,
        }
    }

    pub fn module_name(&self) -> &str {
        self.module.name.as_str()
    }

    pub fn entry_process_label(&self) -> Result<&str> {
        self.processes
            .get(self.entry_process.index())
            .map(|process| process.debug_name.as_str())
            .ok_or_else(|| Error::new("checked entry process is not defined"))
    }

    pub(in crate::language) fn module(&self) -> &Module {
        &self.module
    }

    pub(in crate::language) fn entry_process(&self) -> CheckedProcessId {
        self.entry_process
    }

    pub(in crate::language) fn entry_message(&self) -> CheckedMessageId {
        self.entry_message
    }

    pub(in crate::language) fn types(&self) -> &[CheckedTypeRef] {
        &self.types
    }

    pub(in crate::language) fn outputs(&self) -> &[String] {
        &self.outputs
    }

    pub(in crate::language) fn processes(&self) -> &[CheckedProcess] {
        &self.processes
    }
}

pub(in crate::language) struct CheckedProgramParts {
    pub module: Module,
    pub entry_process: CheckedProcessId,
    pub entry_message: CheckedMessageId,
    pub types: Vec<CheckedTypeRef>,
    pub outputs: Vec<String>,
    pub processes: Vec<CheckedProcess>,
}
