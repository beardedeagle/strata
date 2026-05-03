use super::ast::{Identifier, Module, TypeRef};
use super::diagnostic::{Error, Result};

macro_rules! define_checked_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            pub fn from_index(index: usize) -> Result<Self> {
                let value = u32::try_from(index).map_err(|_| {
                    Error::new(format!("{} index {index} is too large", stringify!($name)))
                })?;
                Ok(Self(value))
            }

            pub const fn as_u32(self) -> u32 {
                self.0
            }

            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

define_checked_id!(CheckedProcessId);
define_checked_id!(CheckedStateId);
define_checked_id!(CheckedMessageId);
define_checked_id!(CheckedOutputId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckedStepResult {
    Continue,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckedNextState {
    Current,
    Value(CheckedStateId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedAction {
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
pub struct CheckedProcess {
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
    pub module: Module,
    pub entry_process: CheckedProcessId,
    pub entry_message: CheckedMessageId,
    pub outputs: Vec<String>,
    pub processes: Vec<CheckedProcess>,
}
