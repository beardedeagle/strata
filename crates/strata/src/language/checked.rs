use super::ast::{Identifier, Module, TypeRef};
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
define_checked_id!(CheckedStateId);
define_checked_id!(CheckedMessageId);
define_checked_id!(CheckedOutputId);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedStepResult {
    Continue,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedNextState {
    Current,
    Value(CheckedStateId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedAction {
    Emit {
        output: CheckedOutputId,
    },
    Spawn {
        target: CheckedProcessId,
    },
    Send {
        target: CheckedProcessId,
        message: CheckedMessageId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcess {
    debug_name: Identifier,
    state_type: TypeRef,
    state_values: Vec<String>,
    message_type: TypeRef,
    message_variants: Vec<Identifier>,
    mailbox_bound: usize,
    init_state: CheckedStateId,
    step_result: CheckedStepResult,
    next_state: CheckedNextState,
    actions: Vec<CheckedAction>,
}

impl CheckedProcess {
    pub(in crate::language) fn new(parts: CheckedProcessParts) -> Self {
        Self {
            debug_name: parts.debug_name,
            state_type: parts.state_type,
            state_values: parts.state_values,
            message_type: parts.message_type,
            message_variants: parts.message_variants,
            mailbox_bound: parts.mailbox_bound,
            init_state: parts.init_state,
            step_result: parts.step_result,
            next_state: parts.next_state,
            actions: parts.actions,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn state_type(&self) -> &TypeRef {
        &self.state_type
    }

    pub(in crate::language) fn state_values(&self) -> &[String] {
        &self.state_values
    }

    pub(in crate::language) fn message_type(&self) -> &TypeRef {
        &self.message_type
    }

    pub(in crate::language) fn message_variants(&self) -> &[Identifier] {
        &self.message_variants
    }

    pub(in crate::language) fn mailbox_bound(&self) -> usize {
        self.mailbox_bound
    }

    pub(in crate::language) fn init_state(&self) -> CheckedStateId {
        self.init_state
    }

    pub(in crate::language) fn step_result(&self) -> CheckedStepResult {
        self.step_result
    }

    pub(in crate::language) fn next_state(&self) -> CheckedNextState {
        self.next_state
    }

    pub(in crate::language) fn actions(&self) -> &[CheckedAction] {
        &self.actions
    }
}

pub(in crate::language) struct CheckedProcessParts {
    pub debug_name: Identifier,
    pub state_type: TypeRef,
    pub state_values: Vec<String>,
    pub message_type: TypeRef,
    pub message_variants: Vec<Identifier>,
    pub mailbox_bound: usize,
    pub init_state: CheckedStateId,
    pub step_result: CheckedStepResult,
    pub next_state: CheckedNextState,
    pub actions: Vec<CheckedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    module: Module,
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
    outputs: Vec<String>,
    processes: Vec<CheckedProcess>,
}

impl CheckedProgram {
    pub(in crate::language) fn new(parts: CheckedProgramParts) -> Self {
        Self {
            module: parts.module,
            entry_process: parts.entry_process,
            entry_message: parts.entry_message,
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
    pub outputs: Vec<String>,
    pub processes: Vec<CheckedProcess>,
}
