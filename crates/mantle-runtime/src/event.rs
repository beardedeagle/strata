use std::fmt;
use std::num::NonZeroU64;

use mantle_artifact::{
    ArtifactBranch, AuthorityId, ComponentInstanceId, EffectOutcomeId, Error, LoopElementId,
    MAX_ACTIONS_PER_PROCESS, MAX_VALUE_TEMPLATE_DEPTH, MessageId, OutputId, PortId, ProcessId,
    ProtocolId, Result, SpawnSiteId, StateId, StepResult, SupervisorChildId, SupervisorId, TypeId,
};

use crate::program::RuntimePayload;

mod contract;
mod jsonl;
mod validate;

pub use contract::{
    RUNTIME_TRACE_SCHEMA_ID, RUNTIME_TRACE_SCHEMA_VERSION, RuntimeTraceEventContract,
    RuntimeTraceEventKind,
};
pub use validate::{
    RuntimeTraceSummary, RuntimeTraceValidationLimits, validate_runtime_trace_jsonl,
    validate_runtime_trace_jsonl_with_limits,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeProcessId(NonZeroU64);

impl RuntimeProcessId {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub fn from_u64(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or_else(|| Error::new("runtime process id must be greater than zero"))
    }

    pub const fn as_u64(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn checked_next(self) -> Result<Self> {
        self.as_u64()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or_else(|| Error::new("runtime process id overflowed"))
    }
}

impl fmt::Display for RuntimeProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

const RUNTIME_BRANCH_PATH_CAPACITY: usize = MAX_VALUE_TEMPLATE_DEPTH + 1;
const BRANCH_PATH_INDEX_MASK: u16 = 0x0fff;
const BRANCH_PATH_THEN_ACTION: u16 = 0x1000;
const BRANCH_PATH_ELSE_ACTION: u16 = 0x2000;
const BRANCH_PATH_LOOP_BODY_ACTION: u16 = 0x3000;
const BRANCH_PATH_THEN_STATE: u16 = 0x4000;
const BRANCH_PATH_ELSE_STATE: u16 = 0x4001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeBranchPath {
    segments: [u16; RUNTIME_BRANCH_PATH_CAPACITY],
    len: u8,
}

impl RuntimeBranchPath {
    pub const fn root() -> Self {
        Self {
            segments: [0; RUNTIME_BRANCH_PATH_CAPACITY],
            len: 0,
        }
    }

    pub fn segments(&self) -> &[u16] {
        &self.segments[..usize::from(self.len)]
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub(crate) fn child(self, segment: RuntimeBranchPathSegment) -> Result<Self> {
        let len = usize::from(self.len);
        if len >= RUNTIME_BRANCH_PATH_CAPACITY {
            return Err(Error::new(format!(
                "runtime branch path exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }

        let mut segments = self.segments;
        segments[len] = segment.0;
        Ok(Self {
            segments,
            len: self
                .len
                .checked_add(1)
                .ok_or_else(|| Error::new("runtime branch path length overflowed"))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeBranchPathSegment(u16);

impl RuntimeBranchPathSegment {
    pub(crate) fn action(index: usize) -> Result<Self> {
        Self::indexed(index, 0)
    }

    pub(crate) fn branch_action(branch: ArtifactBranch, index: usize) -> Result<Self> {
        let prefix = match branch {
            ArtifactBranch::Then => BRANCH_PATH_THEN_ACTION,
            ArtifactBranch::Else => BRANCH_PATH_ELSE_ACTION,
        };
        Self::indexed(index, prefix)
    }

    pub(crate) fn loop_body_action(index: usize) -> Result<Self> {
        Self::indexed(index, BRANCH_PATH_LOOP_BODY_ACTION)
    }

    pub(crate) const fn next_state_branch(branch: ArtifactBranch) -> Self {
        match branch {
            ArtifactBranch::Then => Self(BRANCH_PATH_THEN_STATE),
            ArtifactBranch::Else => Self(BRANCH_PATH_ELSE_STATE),
        }
    }

    pub(crate) const fn is_valid_encoded(segment: u16) -> bool {
        match segment {
            BRANCH_PATH_THEN_STATE | BRANCH_PATH_ELSE_STATE => true,
            _ => {
                let prefix = segment & !BRANCH_PATH_INDEX_MASK;
                let index = (segment & BRANCH_PATH_INDEX_MASK) as usize;
                matches!(
                    prefix,
                    0 | BRANCH_PATH_THEN_ACTION
                        | BRANCH_PATH_ELSE_ACTION
                        | BRANCH_PATH_LOOP_BODY_ACTION
                ) && index < MAX_ACTIONS_PER_PROCESS
            }
        }
    }

    fn indexed(index: usize, prefix: u16) -> Result<Self> {
        if index >= MAX_ACTIONS_PER_PROCESS {
            return Err(Error::new(format!(
                "runtime branch path action index {index} exceeds maximum index {}",
                MAX_ACTIONS_PER_PROCESS - 1
            )));
        }
        let segment = u16::try_from(index)
            .map_err(|_| Error::new("runtime branch path action index overflowed"))?;
        Ok(Self(prefix | (segment & BRANCH_PATH_INDEX_MASK)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLoopContext {
    pub element_id: LoopElementId,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    ArtifactLoaded {
        format: String,
        schema_version: String,
        source_language: String,
        module: String,
        entry_process_id: ProcessId,
        entry_process: String,
        entry_message_id: MessageId,
        process_count: usize,
    },
    ProcessSpawned {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        state_id: StateId,
        state: String,
        mailbox_bound: usize,
        spawned_by_pid: Option<RuntimeProcessId>,
    },
    MessageAccepted {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        payload: Option<RuntimePayload>,
        queue_depth: usize,
        sender_pid: Option<RuntimeProcessId>,
    },
    MessageDequeued {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        payload: Option<RuntimePayload>,
        queue_depth: usize,
    },
    ProgramOutput {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        stream: RuntimeOutputStream,
        output_id: OutputId,
        text: String,
    },
    SpawnAuthorityChecked {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        target_process_id: ProcessId,
        spawn_site_id: SpawnSiteId,
        authority_id: AuthorityId,
        authority_policy_decision_id: Option<u32>,
        spawn_kind: RuntimeSpawnKind,
        authority_result: RuntimeAuthorityResult,
    },
    BoundarySendChecked {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        port_id: PortId,
        port: String,
        protocol_id: ProtocolId,
        protocol: String,
        authority_id: AuthorityId,
        authority_policy_decision_id: Option<u32>,
        target_process_id: ProcessId,
        target_process: String,
        message_id: MessageId,
        message: String,
        boundary_result: RuntimeAuthorityResult,
    },
    EffectOutcomeBound {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        outcome_id: EffectOutcomeId,
        action: RuntimeEffectOutcomeAction,
        target_process_id: ProcessId,
        spawn_site_id: Option<SpawnSiteId>,
        message_id: Option<MessageId>,
        port_id: Option<PortId>,
        outcome_result: RuntimeEffectOutcomeResult,
    },
    BranchSelected {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        branch: ArtifactBranch,
        scope: RuntimeBranchScope,
        branch_path: RuntimeBranchPath,
        loop_context: Option<RuntimeLoopContext>,
        condition_type_id: mantle_artifact::TypeId,
        condition: String,
    },
    LoopStarted {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        element_id: LoopElementId,
        collection_type_id: TypeId,
        max_items: usize,
        item_count: usize,
    },
    LoopIteration {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        element_id: LoopElementId,
        index: usize,
        element_type_id: TypeId,
        element: String,
    },
    LoopCompleted {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        element_id: LoopElementId,
        iteration_count: usize,
    },
    StateUpdated {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        from_state_id: StateId,
        from: String,
        to_state_id: StateId,
        to: String,
    },
    ProcessStepped {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
        payload: Option<RuntimePayload>,
        result: RuntimeStepResult,
        state_id: StateId,
        state: String,
    },
    ProcessStopped {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        reason: RuntimeStopReason,
    },
    ProcessFailed {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        state_id: StateId,
        state: String,
        reason: RuntimeFailureReason,
    },
    SupervisorChildStarted {
        supervisor_pid: RuntimeProcessId,
        supervisor_process_id: ProcessId,
        supervisor_process: String,
        supervisor_id: SupervisorId,
        child_id: SupervisorChildId,
        child: String,
        child_pid: RuntimeProcessId,
        child_process_id: ProcessId,
        child_process: String,
        spawn_site_id: SpawnSiteId,
        spawn_kind: RuntimeSpawnKind,
    },
    SupervisorRestartDecision {
        supervisor_pid: RuntimeProcessId,
        supervisor_process_id: ProcessId,
        supervisor_process: String,
        supervisor_id: SupervisorId,
        child_id: SupervisorChildId,
        child: String,
        child_pid: RuntimeProcessId,
        child_process_id: ProcessId,
        child_process: String,
        reason: RuntimeSupervisorExitReason,
        decision: RuntimeSupervisorRestartDecision,
        restart_time_ms: Option<u64>,
        restart_window_count: usize,
        restart_window_limit: u32,
        restart_window_ms: u64,
        new_child_pid: Option<RuntimeProcessId>,
    },
}

impl RuntimeEvent {
    pub const fn trace_kind(&self) -> RuntimeTraceEventKind {
        match self {
            Self::ArtifactLoaded { .. } => RuntimeTraceEventKind::ArtifactLoaded,
            Self::ProcessSpawned { .. } => RuntimeTraceEventKind::ProcessSpawned,
            Self::MessageAccepted { .. } => RuntimeTraceEventKind::MessageAccepted,
            Self::MessageDequeued { .. } => RuntimeTraceEventKind::MessageDequeued,
            Self::ProgramOutput { .. } => RuntimeTraceEventKind::ProgramOutput,
            Self::SpawnAuthorityChecked { .. } => RuntimeTraceEventKind::SpawnAuthorityChecked,
            Self::BoundarySendChecked { .. } => RuntimeTraceEventKind::BoundarySendChecked,
            Self::EffectOutcomeBound { .. } => RuntimeTraceEventKind::EffectOutcomeBound,
            Self::BranchSelected { .. } => RuntimeTraceEventKind::BranchSelected,
            Self::LoopStarted { .. } => RuntimeTraceEventKind::LoopStarted,
            Self::LoopIteration { .. } => RuntimeTraceEventKind::LoopIteration,
            Self::LoopCompleted { .. } => RuntimeTraceEventKind::LoopCompleted,
            Self::StateUpdated { .. } => RuntimeTraceEventKind::StateUpdated,
            Self::ProcessStepped { .. } => RuntimeTraceEventKind::ProcessStepped,
            Self::ProcessStopped { .. } => RuntimeTraceEventKind::ProcessStopped,
            Self::ProcessFailed { .. } => RuntimeTraceEventKind::ProcessFailed,
            Self::SupervisorChildStarted { .. } => RuntimeTraceEventKind::SupervisorChildStarted,
            Self::SupervisorRestartDecision { .. } => {
                RuntimeTraceEventKind::SupervisorRestartDecision
            }
        }
    }

    pub(crate) const fn primary_process_id(&self) -> Option<ProcessId> {
        match self {
            Self::ArtifactLoaded { .. } => None,
            Self::ProcessSpawned { process_id, .. }
            | Self::MessageAccepted { process_id, .. }
            | Self::MessageDequeued { process_id, .. }
            | Self::ProgramOutput { process_id, .. }
            | Self::SpawnAuthorityChecked { process_id, .. }
            | Self::BoundarySendChecked { process_id, .. }
            | Self::EffectOutcomeBound { process_id, .. }
            | Self::BranchSelected { process_id, .. }
            | Self::LoopStarted { process_id, .. }
            | Self::LoopIteration { process_id, .. }
            | Self::LoopCompleted { process_id, .. }
            | Self::StateUpdated { process_id, .. }
            | Self::ProcessStepped { process_id, .. }
            | Self::ProcessStopped { process_id, .. }
            | Self::ProcessFailed { process_id, .. } => Some(*process_id),
            Self::SupervisorChildStarted { .. } | Self::SupervisorRestartDecision { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeEventCompositionContext {
    pub(crate) deployment_id: u32,
    pub(crate) composition_id: u32,
    pub(crate) component_instance_id: Option<ComponentInstanceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffectOutcomeAction {
    Spawn,
    Send,
}

impl RuntimeEffectOutcomeAction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::Send => "send",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffectOutcomeResult {
    Ok,
    Denied,
    Exhausted,
    BackendUnavailable,
    Full,
    Stopped,
    Crashed,
    MailboxClosed,
}

impl RuntimeEffectOutcomeResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Denied => "denied",
            Self::Exhausted => "exhausted",
            Self::BackendUnavailable => "backend_unavailable",
            Self::Full => "full",
            Self::Stopped => "stopped",
            Self::Crashed => "crashed",
            Self::MailboxClosed => "mailbox_closed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBranchScope {
    NextState,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSpawnKind {
    DynamicLocal,
    LexicalSupervisorChild,
}

impl RuntimeSpawnKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DynamicLocal => "dynamic_local",
            Self::LexicalSupervisorChild => "lexical_supervisor_child",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSupervisorExitReason {
    Normal,
    Panic,
}

impl RuntimeSupervisorExitReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Panic => "panic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSupervisorRestartDecision {
    Restarted,
    NotRestarted,
    Denied,
}

impl RuntimeSupervisorRestartDecision {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Restarted => "restarted",
            Self::NotRestarted => "not_restarted",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAuthorityResult {
    Accepted,
    Denied,
}

impl RuntimeAuthorityResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Denied => "denied",
        }
    }
}

impl RuntimeBranchScope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NextState => "next_state",
            Self::Action => "action",
        }
    }
}

#[derive(Debug)]
pub struct RuntimeEventRecord {
    event: RuntimeEvent,
    composition_context: Option<RuntimeEventCompositionContext>,
    jsonl_line_len: usize,
}

impl RuntimeEventRecord {
    pub fn new(event: RuntimeEvent) -> Result<Self> {
        Self::new_with_composition(event, None)
    }

    pub(crate) fn new_with_composition(
        event: RuntimeEvent,
        composition_context: Option<RuntimeEventCompositionContext>,
    ) -> Result<Self> {
        let jsonl_line_len = jsonl::encoded_json_line_len(&event, composition_context)?;
        Ok(Self {
            event,
            composition_context,
            jsonl_line_len,
        })
    }

    pub fn event(&self) -> &RuntimeEvent {
        &self.event
    }

    pub(crate) fn into_event(self) -> RuntimeEvent {
        self.event
    }

    pub(crate) fn write_jsonl_line(&self, writer: &mut impl std::io::Write) -> Result<()> {
        jsonl::write_json_line_to_io(&self.event, self.composition_context, writer)
    }

    pub(crate) fn jsonl_line_bytes_with_newline(&self) -> Result<usize> {
        self.jsonl_line_len
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime trace event size overflowed"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeOutputStream {
    Stdout,
}

impl RuntimeOutputStream {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStepResult {
    Continue,
    Stop,
    Panic,
}

impl RuntimeStepResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
            Self::Panic => "Panic",
        }
    }
}

impl From<StepResult> for RuntimeStepResult {
    fn from(value: StepResult) -> Self {
        match value {
            StepResult::Continue => Self::Continue,
            StepResult::Stop => Self::Stop,
            StepResult::Panic => Self::Panic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStopReason {
    Normal,
    SupervisorShutdown,
    SupervisorFailure,
}

impl RuntimeStopReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::SupervisorShutdown => "supervisor_shutdown",
            Self::SupervisorFailure => "supervisor_failure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFailureReason {
    Panic,
    SupervisorRestartCapacityExceeded,
    SupervisorRestartIntensityExceeded,
    SupervisorRestartThrottled,
}

impl RuntimeFailureReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::SupervisorRestartCapacityExceeded => "supervisor_restart_capacity_exceeded",
            Self::SupervisorRestartIntensityExceeded => "supervisor_restart_intensity_exceeded",
            Self::SupervisorRestartThrottled => "supervisor_restart_throttled",
        }
    }
}
