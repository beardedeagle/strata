use std::fmt;
use std::num::NonZeroU64;

use mantle_artifact::{Error, MessageId, OutputId, ProcessId, Result, StateId, StepResult};

mod jsonl;

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
        queue_depth: usize,
        sender_pid: Option<RuntimeProcessId>,
    },
    MessageDequeued {
        pid: RuntimeProcessId,
        process_id: ProcessId,
        process: String,
        message_id: MessageId,
        message: String,
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
}

#[derive(Debug)]
pub struct RuntimeEventRecord {
    event: RuntimeEvent,
    jsonl_line: String,
}

impl RuntimeEventRecord {
    pub fn new(event: RuntimeEvent) -> Self {
        let jsonl_line = jsonl::encode_json_line(&event);
        Self { event, jsonl_line }
    }

    pub fn event(&self) -> &RuntimeEvent {
        &self.event
    }

    pub(crate) fn jsonl_line(&self) -> &str {
        &self.jsonl_line
    }

    pub(crate) fn jsonl_line_bytes_with_newline(&self) -> Result<usize> {
        self.jsonl_line()
            .len()
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
}

impl RuntimeStepResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "Continue",
            Self::Stop => "Stop",
        }
    }
}

impl From<StepResult> for RuntimeStepResult {
    fn from(value: StepResult) -> Self {
        match value {
            StepResult::Continue => Self::Continue,
            StepResult::Stop => Self::Stop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStopReason {
    Normal,
}

impl RuntimeStopReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
        }
    }
}
