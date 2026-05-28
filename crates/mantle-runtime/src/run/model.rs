use std::collections::VecDeque;

use mantle_artifact::{
    Error, MessageId, ProcessId, Result, StateId, SupervisorChildId, SupervisorId,
};

use crate::event::RuntimeProcessId;
use crate::program::{LoadedProgram, LoadedSupervisorPlan, RuntimePayload};
use crate::report::ProcessStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeSupervisorRef {
    pub(super) supervisor: SupervisorId,
    pub(super) child: SupervisorChildId,
}

pub(super) struct RuntimeSupervisorChildState {
    pub(super) current_pid: Option<RuntimeProcessId>,
}

pub(super) struct RuntimeSupervisorState {
    pub(super) children: Vec<RuntimeSupervisorChildState>,
    pub(super) restart_window: VecDeque<u64>,
}

impl RuntimeSupervisorState {
    pub(super) fn from_plan(plan: &LoadedSupervisorPlan) -> Self {
        Self {
            children: (0..plan.children.len())
                .map(|_| RuntimeSupervisorChildState { current_pid: None })
                .collect(),
            restart_window: VecDeque::new(),
        }
    }
}

pub(super) struct ProcessInstance {
    pub(super) pid: RuntimeProcessId,
    pub(super) process_id: ProcessId,
    pub(super) state: StateId,
    pub(super) status: ProcessStatus,
    pub(super) supervisor_parent: Option<(RuntimeProcessId, RuntimeSupervisorRef)>,
    pub(super) supervisors: Vec<RuntimeSupervisorState>,
    pub(super) mailbox_bound: usize,
    pub(super) mailbox: VecDeque<RuntimeMessageEnvelope>,
}

impl ProcessInstance {
    pub(super) fn dequeue(&mut self, program: &LoadedProgram) -> Result<DequeuedMessage> {
        if self.mailbox.is_empty() {
            return Err(Error::new(format!(
                "process {} mailbox is empty",
                program
                    .process_label(self.process_id)
                    .unwrap_or("<unknown>")
            )));
        }
        let queue_depth = self.mailbox.len() - 1;
        let removed = self.mailbox.pop_front().ok_or_else(|| {
            Error::new(format!(
                "process {} mailbox changed during dequeue",
                program
                    .process_label(self.process_id)
                    .unwrap_or("<unknown>")
            ))
        })?;
        Ok(DequeuedMessage {
            pid: self.pid,
            process_id: self.process_id,
            envelope: removed,
            queue_depth,
        })
    }
}

pub(super) struct DequeuedMessage {
    pub(super) pid: RuntimeProcessId,
    pub(super) process_id: ProcessId,
    pub(super) envelope: RuntimeMessageEnvelope,
    pub(super) queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeMessageEnvelope {
    pub(super) message: MessageId,
    pub(super) payload: Option<RuntimePayload>,
}

impl RuntimeMessageEnvelope {
    pub(super) fn new(message: MessageId, payload: Option<RuntimePayload>) -> Self {
        Self { message, payload }
    }

    pub(super) fn validate_for_process(
        &self,
        program: &LoadedProgram,
        process_id: ProcessId,
    ) -> Result<()> {
        let payload_type = program.message_payload_type(process_id, self.message)?;
        match (payload_type, &self.payload) {
            (None, None) => Ok(()),
            (None, Some(_)) => Err(Error::new(format!(
                "message id {} for process id {} does not accept a payload",
                self.message.as_u32(),
                process_id.as_u32()
            ))),
            (Some(_), None) => Err(Error::new(format!(
                "message id {} for process id {} requires a payload",
                self.message.as_u32(),
                process_id.as_u32()
            ))),
            (Some(expected_type), Some(payload)) => {
                program.validate_runtime_payload_matches_type(
                    &format!(
                        "message id {} for process id {} payload",
                        self.message.as_u32(),
                        process_id.as_u32()
                    ),
                    expected_type,
                    payload,
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn display_label(&self, message_label: &str) -> String {
        match &self.payload {
            Some(payload) => {
                let payload_len = payload.label_len().unwrap_or(0);
                let mut label = String::with_capacity(message_label.len() + 2 + payload_len);
                label.push_str(message_label);
                label.push('(');
                payload.write_label(&mut label);
                label.push(')');
                label
            }
            None => message_label.to_string(),
        }
    }
}

pub(super) struct ActiveStep {
    pub(super) pid: RuntimeProcessId,
    pub(super) process_id: ProcessId,
    pub(super) process_name: String,
    pub(super) current_state: StateId,
    pub(super) message: MessageId,
    pub(super) message_label: String,
    pub(super) payload: Option<RuntimePayload>,
}

impl ActiveStep {
    pub(super) fn new(
        program: &LoadedProgram,
        process: &ProcessInstance,
        envelope: RuntimeMessageEnvelope,
    ) -> Result<Self> {
        let definition = program.process(process.process_id)?;
        let process_name = definition.debug_name.clone();
        definition
            .state_values
            .get(process.state.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} current_state id {} is not loaded",
                    process_name,
                    process.state.as_u32()
                ))
            })?;
        let message_label = definition
            .message_variants
            .get(envelope.message.index())
            .map(|message| message.label.clone())
            .ok_or_else(|| {
                Error::new(format!(
                    "message id {} is not loaded for process id {}",
                    envelope.message.as_u32(),
                    process.process_id.as_u32()
                ))
            })?;
        Ok(Self {
            pid: process.pid,
            process_id: process.process_id,
            process_name,
            current_state: process.state,
            message: envelope.message,
            message_label,
            payload: envelope.payload,
        })
    }

    pub(super) fn current_state_payload<'program>(
        &self,
        program: &'program LoadedProgram,
    ) -> Result<Option<&'program RuntimePayload>> {
        let state_value = program
            .process(self.process_id)?
            .state_values
            .get(self.current_state.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} current_state id {} is not loaded",
                    self.process_name,
                    self.current_state.as_u32()
                ))
            })?;
        Ok(state_value.payload.as_ref())
    }
}
