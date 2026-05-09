use std::collections::{BTreeMap, VecDeque};

use mantle_artifact::{
    ArtifactPayload, ArtifactProcessRefPayload, ArtifactValueTemplate, Error, MantleArtifact,
    MessageId, NextState, OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult,
    validate_payload_value_label,
};

use crate::event::{
    RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason, RuntimeOutputStream, RuntimeProcessId,
    RuntimeStepResult, RuntimeStopReason,
};
use crate::host::RuntimeHost;
use crate::limits::RunLimits;
use crate::program::{LoadedAction, LoadedProgram, LoadedSendTarget};
use crate::report::{MessageDelivery, ProcessReport, ProcessStatus, RuntimeReport, SpawnReport};

pub fn run_artifact_with_host<H: RuntimeHost>(
    artifact: &MantleArtifact,
    host: &mut H,
    limits: RunLimits,
) -> Result<RuntimeReport> {
    limits.validate()?;
    let program = LoadedProgram::from_artifact(artifact)?;
    run_loaded_program_with_host(&program, host, limits)
}

pub(crate) fn run_loaded_program_with_host<H: RuntimeHost>(
    program: &LoadedProgram,
    host: &mut H,
    limits: RunLimits,
) -> Result<RuntimeReport> {
    program.validate_admission()?;
    let mut run = RuntimeRun::new(
        program,
        host,
        limits.max_runtime_processes,
        limits.max_trace_bytes,
        limits.max_emitted_output_bytes,
    );
    run.record_event(RuntimeEvent::ArtifactLoaded {
        format: program.format.clone(),
        schema_version: program.schema_version.clone(),
        source_language: program.source_language.clone(),
        module: program.module.clone(),
        entry_process_id: program.entry_process,
        entry_process: program.process_label(program.entry_process)?.to_string(),
        entry_message_id: program.entry_message,
        process_count: program.processes.len(),
    })?;
    let entry_pid = run.spawn_process(program.entry_process, None)?;
    run.send_message(
        entry_pid,
        RuntimeMessageEnvelope::new(program.entry_message, None),
        None,
    )?;
    run.drain_mailboxes(limits.max_dispatches)?;
    run.reject_unhandled_messages()?;
    run.flush_host()?;

    let process_reports = run
        .processes
        .into_iter()
        .map(|process| {
            Ok(ProcessReport {
                pid: process.pid,
                process: program.process_label(process.process_id)?.to_string(),
                state: program
                    .state_label(process.process_id, process.state)?
                    .to_string(),
                status: process.status,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(RuntimeReport {
        entry_process: program.process_label(program.entry_process)?.to_string(),
        entry_message: program
            .message_label(program.entry_process, program.entry_message)?
            .to_string(),
        spawned_processes: run.spawned_processes,
        delivered_messages: run.delivered_messages,
        processes: process_reports,
        emitted_outputs: run.emitted_outputs,
    })
}

struct RuntimeRun<'program, 'host, H: RuntimeHost> {
    program: &'program LoadedProgram,
    host: &'host mut H,
    processes: Vec<ProcessInstance>,
    next_pid: RuntimeProcessId,
    max_runtime_processes: usize,
    trace_bytes: usize,
    max_trace_bytes: usize,
    emitted_output_bytes: usize,
    max_emitted_output_bytes: usize,
    spawned_processes: Vec<SpawnReport>,
    delivered_messages: Vec<MessageDelivery>,
    emitted_outputs: Vec<String>,
}

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
    fn new(
        program: &'program LoadedProgram,
        host: &'host mut H,
        max_runtime_processes: usize,
        max_trace_bytes: usize,
        max_emitted_output_bytes: usize,
    ) -> Self {
        Self {
            program,
            host,
            processes: Vec::new(),
            next_pid: RuntimeProcessId::FIRST,
            max_runtime_processes,
            trace_bytes: 0,
            max_trace_bytes,
            emitted_output_bytes: 0,
            max_emitted_output_bytes,
            spawned_processes: Vec::new(),
            delivered_messages: Vec::new(),
            emitted_outputs: Vec::new(),
        }
    }

    fn record_event(&mut self, event: RuntimeEvent) -> Result<()> {
        let record = RuntimeEventRecord::new(event);
        let event_bytes = checked_trace_event_bytes(self.trace_bytes, &record)?;
        if event_bytes > self.max_trace_bytes {
            return Err(Error::new(format!(
                "runtime trace exceeded maximum size of {} bytes",
                self.max_trace_bytes
            )));
        }
        self.host.record_event(&record)?;
        self.trace_bytes = event_bytes;
        Ok(())
    }

    fn flush_host(&mut self) -> Result<()> {
        self.host.flush()
    }

    fn spawn_process(
        &mut self,
        process_id: ProcessId,
        spawned_by_pid: Option<RuntimeProcessId>,
    ) -> Result<RuntimeProcessId> {
        if self.processes.len() >= self.max_runtime_processes {
            return Err(Error::new(format!(
                "runtime process instance limit exceeded at {} process instance(s)",
                self.max_runtime_processes
            )));
        }
        let definition = self.program.process(process_id)?;
        let pid = self.next_pid;
        self.next_pid = self.next_pid.checked_next()?;
        let process = ProcessInstance {
            pid,
            process_id,
            state: definition.init_state,
            status: ProcessStatus::Running,
            mailbox_bound: definition.mailbox_bound,
            mailbox: VecDeque::new(),
        };

        self.record_event(RuntimeEvent::ProcessSpawned {
            pid,
            process_id,
            process: definition.debug_name.clone(),
            state_id: process.state,
            state: self
                .program
                .state_label(process_id, process.state)?
                .to_string(),
            mailbox_bound: process.mailbox_bound,
            spawned_by_pid,
        })?;
        self.spawned_processes.push(SpawnReport {
            pid,
            process: definition.debug_name.clone(),
        });
        self.processes.push(process);
        Ok(pid)
    }

    fn send_message(
        &mut self,
        target: RuntimeProcessId,
        envelope: RuntimeMessageEnvelope,
        sender_pid: Option<RuntimeProcessId>,
    ) -> Result<()> {
        let process_index = self.process_index_for_pid(target)?;
        let process = &self.processes[process_index];
        let target_process = self.program.process(process.process_id)?;
        envelope.validate_for_process(self.program, process.process_id)?;
        self.validate_envelope_process_ref(&envelope)?;
        let message_label = self
            .program
            .message_label(process.process_id, envelope.message)?
            .to_string();
        let process_label = target_process.debug_name.clone();
        if process.status != ProcessStatus::Running {
            return Err(Error::new(format!(
                "send to process {} failed because it is not running",
                process_label
            )));
        }
        if process.mailbox.len() >= process.mailbox_bound {
            return Err(Error::new(format!(
                "mailbox for process {} is full; message was not accepted",
                process_label
            )));
        }
        let pid = process.pid;
        let queue_depth = process.mailbox.len() + 1;

        self.record_event(RuntimeEvent::MessageAccepted {
            pid,
            process_id: process.process_id,
            process: process_label.clone(),
            message_id: envelope.message,
            message: message_label.clone(),
            payload: envelope.payload.clone(),
            queue_depth,
            sender_pid,
        })?;
        self.processes[process_index]
            .mailbox
            .push_back(envelope.clone());
        self.delivered_messages.push(MessageDelivery {
            pid,
            process: process_label,
            message: envelope.display_label(&message_label),
        });
        Ok(())
    }

    fn validate_envelope_process_ref(&self, envelope: &RuntimeMessageEnvelope) -> Result<()> {
        let Some(payload) = &envelope.payload else {
            return Ok(());
        };
        let expected_target = self
            .program
            .process_ref_target_for_type_id("payload type", payload.ty);
        let (expected_target, process_ref) = match (expected_target, payload.process_ref) {
            (Ok(expected_target), Some(process_ref)) => (expected_target, process_ref),
            (Ok(_), None) => {
                return Err(Error::new(format!(
                    "payload type id {} requires process reference runtime metadata",
                    payload.ty.as_u32()
                )));
            }
            (Err(_), Some(_)) => {
                return Err(Error::new(format!(
                    "payload type id {} must not carry process reference runtime metadata",
                    payload.ty.as_u32()
                )));
            }
            (Err(_), None) => {
                self.program
                    .validate_value_type("payload type", payload.ty)?;
                return Ok(());
            }
        };
        let process_index =
            self.process_index_for_pid(RuntimeProcessId::from_u64(process_ref.pid)?)?;
        let referenced = &self.processes[process_index];
        if referenced.process_id != process_ref.target_process {
            return Err(Error::new(format!(
                "payload process reference pid {} targets process id {}, but runtime pid has process id {}",
                process_ref.pid,
                process_ref.target_process.as_u32(),
                referenced.process_id.as_u32()
            )));
        }
        if process_ref.target_process != expected_target {
            return Err(Error::new(format!(
                "payload process reference metadata targets process id {}, expected {} for type id {}",
                process_ref.target_process.as_u32(),
                expected_target.as_u32(),
                payload.ty.as_u32()
            )));
        }
        Ok(())
    }

    fn process_index_for_pid(&self, pid: RuntimeProcessId) -> Result<usize> {
        let raw_index = pid
            .as_u64()
            .checked_sub(1)
            .ok_or_else(|| Error::new("runtime process id index underflowed"))?;
        let process_index = usize::try_from(raw_index).map_err(|_| {
            Error::new(format!(
                "runtime process {pid} cannot be indexed on this platform"
            ))
        })?;
        let process = self
            .processes
            .get(process_index)
            .ok_or_else(|| Error::new(format!("runtime process {pid} is not spawned")))?;
        if process.pid != pid {
            return Err(Error::new(format!(
                "runtime process index for pid {pid} is inconsistent"
            )));
        }
        Ok(process_index)
    }

    fn drain_mailboxes(&mut self, max_dispatches: usize) -> Result<()> {
        let mut dispatches = 0usize;
        while let Some(process_index) = self.next_runnable_process() {
            if dispatches >= max_dispatches {
                return Err(Error::new(format!(
                    "runtime dispatch budget exceeded after {max_dispatches} process step(s)"
                )));
            }
            let dequeued = self.processes[process_index].dequeue(self.program)?;
            self.record_event(RuntimeEvent::MessageDequeued {
                pid: dequeued.pid,
                process_id: dequeued.process_id,
                process: self.program.process_label(dequeued.process_id)?.to_string(),
                message_id: dequeued.envelope.message,
                message: self
                    .program
                    .message_label(dequeued.process_id, dequeued.envelope.message)?
                    .to_string(),
                payload: dequeued.envelope.payload.clone(),
                queue_depth: dequeued.queue_depth,
            })?;
            self.step_process(process_index, dequeued.envelope)?;
            dispatches += 1;
        }
        Ok(())
    }

    fn next_runnable_process(&self) -> Option<usize> {
        self.processes.iter().position(|process| {
            process.status == ProcessStatus::Running && !process.mailbox.is_empty()
        })
    }

    fn step_process(
        &mut self,
        process_index: usize,
        envelope: RuntimeMessageEnvelope,
    ) -> Result<()> {
        if self.processes[process_index].status != ProcessStatus::Running {
            let process_name = self
                .program
                .process_label(self.processes[process_index].process_id)?;
            return Err(Error::new(format!(
                "process {process_name} cannot step because it is not running"
            )));
        }

        let step = ActiveStep::new(self.program, &self.processes[process_index], envelope)?;
        let definition = self.program.process(step.process_id)?;
        let transition = definition.transition_for_dispatch(step.message, step.current_state)?;
        let next_state = transition.next_state.clone();
        let step_result = transition.step_result;
        let final_state = self.resolve_next_state(process_index, &step, &next_state)?;
        let mut local_process_refs = BTreeMap::new();

        for action in &transition.actions {
            self.execute_action(&mut local_process_refs, &step, action)?;
        }

        self.apply_next_state(process_index, &step, final_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action(
        &mut self,
        local_process_refs: &mut BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        action: &LoadedAction,
    ) -> Result<()> {
        match action {
            LoadedAction::Emit { output } => self.emit_output(step, *output),
            LoadedAction::Spawn {
                target,
                process_ref,
            } => {
                let declared_target = self.process_ref_target(step, *process_ref)?;
                if declared_target != *target {
                    return Err(Error::new(format!(
                        "process {} spawn process reference id {} targets process id {}, expected {}",
                        step.process_name,
                        process_ref.as_u32(),
                        target.as_u32(),
                        declared_target.as_u32()
                    )));
                }
                self.ensure_process_ref_unbound(local_process_refs, step, *process_ref)?;
                let pid = self.spawn_process(*target, Some(step.pid))?;
                self.bind_process_ref(local_process_refs, step, *process_ref, pid)?;
                Ok(())
            }
            LoadedAction::Send {
                target,
                message,
                payload,
            } => {
                let pid = self.resolve_send_target(local_process_refs, step, target)?;
                let prepared_payload = match payload {
                    Some(payload) => Some(evaluate_runtime_template(
                        self.program,
                        payload,
                        step.payload.as_ref(),
                        step,
                        local_process_refs,
                    )?),
                    None => None,
                };
                self.send_message(
                    pid,
                    RuntimeMessageEnvelope::new(*message, prepared_payload),
                    Some(step.pid),
                )
            }
        }
    }

    fn resolve_send_target(
        &self,
        local_process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        target: &LoadedSendTarget,
    ) -> Result<RuntimeProcessId> {
        match target {
            LoadedSendTarget::ProcessRef(process_ref) => {
                self.resolve_process_ref(local_process_refs, step, *process_ref)
            }
            LoadedSendTarget::ReceivedPayload { ty, target_process } => {
                let payload = step.payload.as_ref().ok_or_else(|| {
                    Error::new("received process reference send target requires a payload")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received process reference send target has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                let process_ref = payload.process_ref.ok_or_else(|| {
                    Error::new("received payload is not a process reference value")
                })?;
                if process_ref.target_process != *target_process {
                    return Err(Error::new(format!(
                        "received process reference targets process id {}, expected {}",
                        process_ref.target_process.as_u32(),
                        target_process.as_u32()
                    )));
                }
                Ok(RuntimeProcessId::from_u64(process_ref.pid)?)
            }
        }
    }

    fn process_ref_target(
        &self,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<ProcessId> {
        self.program
            .process(step.process_id)?
            .process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process reference id {}",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })
    }

    fn ensure_process_ref_unbound(
        &self,
        local_process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        self.process_ref_target(step, process_ref)?;
        if local_process_refs.contains_key(&process_ref) {
            return Err(Error::new(format!(
                "process {} rebinds process reference id {}",
                step.process_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    fn bind_process_ref(
        &self,
        local_process_refs: &mut BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        process_ref: ProcessRefId,
        pid: RuntimeProcessId,
    ) -> Result<()> {
        self.process_ref_target(step, process_ref)?;
        if local_process_refs.insert(process_ref, pid).is_some() {
            return Err(Error::new(format!(
                "process {} rebinds process reference id {}",
                step.process_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    fn resolve_process_ref(
        &self,
        local_process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<RuntimeProcessId> {
        self.process_ref_target(step, process_ref)?;
        local_process_refs
            .get(&process_ref)
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} sends to unbound process reference id {}",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })
    }

    fn emit_output(&mut self, step: &ActiveStep, output: OutputId) -> Result<()> {
        let text = self.program.output(output)?.to_string();
        let emitted_output_bytes = checked_output_bytes(self.emitted_output_bytes, text.len())?;
        if emitted_output_bytes > self.max_emitted_output_bytes {
            return Err(Error::new(format!(
                "emitted output exceeded maximum size of {} bytes",
                self.max_emitted_output_bytes
            )));
        }
        self.record_event(RuntimeEvent::ProgramOutput {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            stream: RuntimeOutputStream::Stdout,
            output_id: output,
            text: text.clone(),
        })?;
        self.host.emit_stdout(&text)?;
        self.emitted_output_bytes = emitted_output_bytes;
        self.emitted_outputs.push(text);
        Ok(())
    }

    fn resolve_template_state(
        &self,
        step: &ActiveStep,
        template: &ArtifactValueTemplate,
    ) -> Result<StateId> {
        let value = self.evaluate_state_template(template, step)?;
        let process = self.program.process(step.process_id)?;
        let state_index = process
            .state_values
            .iter()
            .position(|state| state.ty == value.ty && state.value == value.value)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} next_state template produced value {} not admitted by state table",
                    step.process_name, value.value
                ))
            })?;
        StateId::from_index(state_index)
    }

    fn evaluate_state_template(
        &self,
        template: &ArtifactValueTemplate,
        step: &ActiveStep,
    ) -> Result<ArtifactPayload> {
        evaluate_runtime_template(
            self.program,
            template,
            step.payload.as_ref(),
            step,
            &BTreeMap::new(),
        )
    }

    fn resolve_next_state(
        &self,
        process_index: usize,
        step: &ActiveStep,
        next_state: &NextState,
    ) -> Result<StateId> {
        match next_state {
            NextState::Current => Ok(self.processes[process_index].state),
            NextState::Value(state) => Ok(*state),
            NextState::Template(template) => self.resolve_template_state(step, template),
        }
    }

    fn apply_next_state(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        final_state: StateId,
    ) -> Result<()> {
        let previous_state = self.processes[process_index].state;
        if previous_state == final_state {
            return Ok(());
        }

        self.record_event(RuntimeEvent::StateUpdated {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            from_state_id: previous_state,
            from: self
                .program
                .state_label(step.process_id, previous_state)?
                .to_string(),
            to_state_id: final_state,
            to: self
                .program
                .state_label(step.process_id, final_state)?
                .to_string(),
        })?;
        self.processes[process_index].state = final_state;
        Ok(())
    }

    fn record_step_completion(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        step_result: StepResult,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::ProcessStepped {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            payload: step.payload.clone(),
            result: RuntimeStepResult::from(step_result),
            state_id: self.processes[process_index].state,
            state: self
                .program
                .state_label(step.process_id, self.processes[process_index].state)?
                .to_string(),
        })?;
        match step_result {
            StepResult::Continue => Ok(()),
            StepResult::Stop => {
                self.record_event(RuntimeEvent::ProcessStopped {
                    pid: step.pid,
                    process_id: step.process_id,
                    process: step.process_name.clone(),
                    reason: RuntimeStopReason::Normal,
                })?;
                self.processes[process_index].status = ProcessStatus::Stopped;
                Ok(())
            }
            StepResult::Panic => {
                let state_id = self.processes[process_index].state;
                let state = self
                    .program
                    .state_label(step.process_id, state_id)?
                    .to_string();
                self.record_event(RuntimeEvent::ProcessFailed {
                    pid: step.pid,
                    process_id: step.process_id,
                    process: step.process_name.clone(),
                    state_id,
                    state,
                    reason: RuntimeFailureReason::Panic,
                })?;
                self.processes[process_index].status = ProcessStatus::Failed;
                self.flush_host()?;
                Err(Error::new(format!(
                    "process {} panicked after consuming message {}; message will not be replayed",
                    step.process_name, step.message_label
                )))
            }
        }
    }

    fn reject_unhandled_messages(&self) -> Result<()> {
        for process in &self.processes {
            if !process.mailbox.is_empty() {
                return Err(Error::new(format!(
                    "process {} has {} unhandled message(s)",
                    self.program.process_label(process.process_id)?,
                    process.mailbox.len()
                )));
            }
        }
        Ok(())
    }
}

struct ProcessInstance {
    pid: RuntimeProcessId,
    process_id: ProcessId,
    state: StateId,
    status: ProcessStatus,
    mailbox_bound: usize,
    mailbox: VecDeque<RuntimeMessageEnvelope>,
}

impl ProcessInstance {
    fn dequeue(&mut self, program: &LoadedProgram) -> Result<DequeuedMessage> {
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

struct DequeuedMessage {
    pid: RuntimeProcessId,
    process_id: ProcessId,
    envelope: RuntimeMessageEnvelope,
    queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeMessageEnvelope {
    message: MessageId,
    payload: Option<ArtifactPayload>,
}

impl RuntimeMessageEnvelope {
    fn new(message: MessageId, payload: Option<ArtifactPayload>) -> Self {
        Self { message, payload }
    }

    fn validate_for_process(&self, program: &LoadedProgram, process_id: ProcessId) -> Result<()> {
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
                if payload.ty != expected_type {
                    return Err(Error::new(format!(
                        "message id {} for process id {} payload has type id {}, expected {}",
                        self.message.as_u32(),
                        process_id.as_u32(),
                        payload.ty.as_u32(),
                        expected_type.as_u32()
                    )));
                }
                Ok(())
            }
        }
    }

    fn display_label(&self, message_label: &str) -> String {
        match &self.payload {
            Some(payload) => format!("{message_label}({})", payload.value),
            None => message_label.to_string(),
        }
    }
}

struct ActiveStep {
    pid: RuntimeProcessId,
    process_id: ProcessId,
    process_name: String,
    current_state: StateId,
    message: MessageId,
    message_label: String,
    payload: Option<ArtifactPayload>,
}

impl ActiveStep {
    fn new(
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

    fn current_state_payload<'program>(
        &self,
        program: &'program LoadedProgram,
    ) -> Result<Option<&'program ArtifactPayload>> {
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

fn evaluate_runtime_template(
    program: &LoadedProgram,
    template: &ArtifactValueTemplate,
    received_payload: Option<&ArtifactPayload>,
    step: &ActiveStep,
    process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
) -> Result<ArtifactPayload> {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => Ok(ArtifactPayload {
            ty: *ty,
            value: value.clone(),
            process_ref: None,
        }),
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            let payload = received_payload.ok_or_else(|| {
                Error::new("received payload template requires a payload-bearing message")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "received payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        ArtifactValueTemplate::CurrentStatePayload { ty } => {
            let payload = step.current_state_payload(program)?.ok_or_else(|| {
                Error::new("current state payload template requires a payload-bearing state")
            })?;
            if payload.ty != *ty {
                return Err(Error::new(format!(
                    "current state payload has type id {}, expected {}",
                    payload.ty.as_u32(),
                    ty.as_u32()
                )));
            }
            Ok(payload.clone())
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            let pid = process_refs.get(process_ref).copied().ok_or_else(|| {
                Error::new(format!(
                    "process {} sends unbound process reference id {} as payload",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })?;
            Ok(ArtifactPayload {
                ty: *ty,
                value: format!("type{}#{}", ty.as_u32(), pid.as_u64()),
                process_ref: Some(ArtifactProcessRefPayload {
                    target_process: *target_process,
                    pid: pid.as_u64(),
                }),
            })
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            let payload =
                evaluate_runtime_template(program, payload, received_payload, step, process_refs)?;
            let value = format!("{variant}({})", payload.value);
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            let type_label = program.type_label(*ty)?;
            let mut parts = Vec::with_capacity(fields.len());
            for field in fields {
                let value = evaluate_runtime_template(
                    program,
                    &field.value,
                    received_payload,
                    step,
                    process_refs,
                )?;
                parts.push(format!("{}:{}", field.name, value.value));
            }
            let value = format!("{type_label}{{{}}}", parts.join(","));
            validate_payload_value_label(&value)?;
            Ok(ArtifactPayload {
                ty: *ty,
                value,
                process_ref: None,
            })
        }
    }
}

fn checked_trace_event_bytes(current: usize, event: &RuntimeEventRecord) -> Result<usize> {
    let event_line_bytes = event.jsonl_line_bytes_with_newline()?;
    current
        .checked_add(event_line_bytes)
        .ok_or_else(|| Error::new("runtime trace size overflowed"))
}

fn checked_output_bytes(current: usize, next_output_len: usize) -> Result<usize> {
    let next_output_with_newline = next_output_len
        .checked_add(1)
        .ok_or_else(|| Error::new("emitted output size overflowed"))?;
    current
        .checked_add(next_output_with_newline)
        .ok_or_else(|| Error::new("emitted output size overflowed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::InMemoryRuntimeHost;
    use crate::limits::{
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_PROCESSES, DEFAULT_MAX_TRACE_BYTES,
    };
    use mantle_artifact::{
        ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactEffect, ArtifactMessageVariant,
        ArtifactProcess, ArtifactProcessRef, ArtifactStateValue, ArtifactTransition, ArtifactType,
        ArtifactValueTemplateField, MAX_FIELD_VALUE_BYTES, MAX_PROCESS_REFS_PER_PROCESS,
        MAX_VALUE_TEMPLATE_DEPTH, StepResult, TypeId,
    };

    const TEST_SOURCE_LANGUAGE: &str = "test_frontend";
    const MAIN_STATE: TypeId = TypeId::new(0);
    const MAIN_MSG: TypeId = TypeId::new(1);
    const WORKER_STATE: TypeId = TypeId::new(2);
    const WORKER_MSG: TypeId = TypeId::new(3);
    const JOB: TypeId = TypeId::new(4);
    const BOX: TypeId = TypeId::new(5);
    const LEAF: TypeId = TypeId::new(6);
    const START_PAYLOAD: TypeId = TypeId::new(7);
    const PROCESS_REF_WORKER: TypeId = TypeId::new(8);

    #[test]
    fn loaded_program_stores_large_process_ref_tables_without_runtime_instance_maps() {
        let artifact = artifact_with_large_unbound_process_ref_table();
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let mut host = InMemoryRuntimeHost::default();
        let mut run = RuntimeRun::new(
            &program,
            &mut host,
            DEFAULT_MAX_RUNTIME_PROCESSES,
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        );

        let pid = run
            .spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");

        assert_eq!(pid, RuntimeProcessId::FIRST);
        assert_eq!(
            program
                .process(ProcessId::new(0))
                .expect("entry process should load")
                .process_refs
                .len(),
            MAX_PROCESS_REFS_PER_PROCESS
        );
    }

    #[test]
    fn runtime_rejects_loaded_action_without_effect_authority_before_emit() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.outputs.push("forbidden output".to_string());
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Emit {
                output: OutputId::new(0),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 uses effect emit without admitted authority",
        );
    }

    #[test]
    fn runtime_rejects_loaded_unused_effect_authority_before_state_update() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 admits effect emit but no action uses it",
        );
    }

    #[test]
    fn runtime_rejects_loaded_duplicate_effect_authority_before_emit() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.outputs.push("forbidden output".to_string());
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[
                ArtifactEffect::Emit,
                ArtifactEffect::Emit,
            ]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Emit {
                output: OutputId::new(0),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 admits duplicate effect emit",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_artifact_identity_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.format = "unexpected-format".to_string();

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "loaded artifact format \"unexpected-format\"; expected \"mantle-target-artifact\"",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_schema_version_with_field_name_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.schema_version = "0".to_string();

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "loaded artifact schema_version \"0\"; expected",
        );
    }

    #[test]
    fn runtime_rejects_loaded_control_character_artifact_identity_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.format = "bad\nformat".to_string();

        let err = loaded_admission_error_before_artifact_loaded(&program);

        assert!(err.contains(
            "loaded artifact format must be non-empty and contain no control characters"
        ));
        assert!(err.contains("\"bad\\nformat\""));
        assert!(!err.contains("bad\nformat"));
    }

    #[test]
    fn runtime_rejects_loaded_oversized_artifact_identity_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.schema_version = "x".repeat(MAX_FIELD_VALUE_BYTES + 1);

        let err = loaded_admission_error_before_artifact_loaded(&program);

        assert!(err.contains("loaded artifact schema_version exceeds maximum length"));
        assert!(!err.contains(&"x".repeat(256)));
    }

    #[test]
    fn runtime_rejects_loaded_control_character_process_name_before_duplicate_check() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].debug_name = "bad\nprocess".to_string();
        program.processes[1].debug_name = "bad\nprocess".to_string();

        let err = loaded_admission_error_before_artifact_loaded(&program);

        assert!(err.contains("process debug_name must be an identifier"));
        assert!(err.contains("\"bad\\nprocess\""));
        assert!(!err.contains("bad\nprocess"));
        assert!(!err.contains("duplicate loaded process debug_name"));
    }

    #[test]
    fn runtime_rejects_loaded_control_character_state_type_before_mismatch_diagnostic() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.types[MAIN_STATE.index()].label = "Bad\nState".to_string();

        let err = loaded_admission_error_before_artifact_loaded(&program);

        assert!(err.contains("type.0.label must be an identifier"));
        assert!(err.contains("\"Bad\\nState\""));
        assert!(!err.contains("Bad\nState"));
        assert!(!err.contains("loaded state value MainState has type"));
    }

    #[test]
    fn runtime_rejects_loaded_process_ref_state_type_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].state_type = PROCESS_REF_WORKER;

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "state_type type id 8 must be a value type",
        );
    }

    #[test]
    fn runtime_rejects_loaded_payload_bearing_entry_message_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].message_variants[0].payload_type = Some(START_PAYLOAD);

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "entry message id 0 must not require a payload",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_message_payload_type_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[1].message_variants[0].payload_type = Some(TypeId::new(99));

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Worker message payload_type: loaded type id 99 is not loaded",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_init_state_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].init_state = StateId::new(1);

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main init_state id 1 is not a loaded state value",
        );
    }

    #[test]
    fn runtime_rejects_loaded_unknown_next_state_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].next_state = NextState::Value(StateId::new(1));

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 next_state id 1 is not a loaded state value",
        );
    }

    #[test]
    fn runtime_rejects_loaded_unadmitted_template_state_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].next_state =
            NextState::Template(ArtifactValueTemplate::Literal {
                ty: MAIN_STATE,
                value: "UnadmittedState".to_string(),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 next_state_template produced value UnadmittedState not admitted by loaded state table",
        );
    }

    #[test]
    fn runtime_rejects_loaded_process_ref_payload_enum_next_state_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
        program.processes[1].transitions[0].next_state =
            NextState::Template(ArtifactValueTemplate::EnumVariant {
                ty: WORKER_STATE,
                variant: "Routed".to_string(),
                payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                    ty: PROCESS_REF_WORKER,
                }),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Worker transition 0 next_state_template.payload process reference template must be a direct message payload",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_template_field_type_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].next_state =
            NextState::Template(ArtifactValueTemplate::Record {
                ty: MAIN_STATE,
                fields: vec![ArtifactValueTemplateField {
                    name: "item".to_string(),
                    value: ArtifactValueTemplate::Literal {
                        ty: TypeId::new(99),
                        value: "Item".to_string(),
                    },
                }],
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "loaded type id 99 is not loaded",
        );
    }

    #[test]
    fn runtime_rejects_loaded_template_depth_overflow_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].next_state =
            NextState::Template(record_template_with_depth(MAX_VALUE_TEMPLATE_DEPTH + 2));

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "exceeds maximum value template depth",
        );
    }

    #[test]
    fn runtime_rejects_loaded_unknown_emit_output_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Emit {
                output: OutputId::new(0),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "output id 0 is not loaded",
        );
    }

    #[test]
    fn runtime_rejects_loaded_invalid_output_text_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.outputs.push(String::new());
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Emit {
                output: OutputId::new(0),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "loaded output must be non-empty and contain no control characters",
        );
    }

    #[test]
    fn runtime_rejects_loaded_spawn_target_mismatch_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Spawn]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Spawn {
                target: ProcessId::new(0),
                process_ref: ProcessRefId::new(0),
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 spawn process reference id 0 targets process id 0, expected 1",
        );
    }

    #[test]
    fn runtime_rejects_loaded_send_before_spawn_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Send]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Send {
                target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: None,
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 sends through unbound process reference id 0",
        );
    }

    #[test]
    fn runtime_rejects_loaded_send_missing_payload_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[1].message_variants[0].payload_type = Some(JOB);
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[
                ArtifactEffect::Spawn,
                ArtifactEffect::Send,
            ]);
        program.processes[0].transitions[0].actions = vec![
            LoadedAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            },
            LoadedAction::Send {
                target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: None,
            },
        ];

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 sends process id 1 message id 0 without required payload",
        );
    }

    #[test]
    fn runtime_rejects_loaded_process_ref_payload_target_mismatch_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[
                ArtifactEffect::Spawn,
                ArtifactEffect::Send,
            ]);
        program.processes[0].transitions[0].actions = vec![
            LoadedAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            },
            LoadedAction::Send {
                target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::ProcessRef {
                    ty: PROCESS_REF_WORKER,
                    target_process: ProcessId::new(0),
                    process_ref: ProcessRefId::new(0),
                }),
            },
        ];

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process reference payload type type id 8 targets process id 1, expected 0",
        );
    }

    #[test]
    fn runtime_rejects_loaded_received_process_ref_send_without_payload_before_artifact_loaded() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        program.processes[0].transitions[0].effect_authority =
            crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Send]);
        program.processes[0].transitions[0]
            .actions
            .push(LoadedAction::Send {
                target: LoadedSendTarget::ReceivedPayload {
                    ty: PROCESS_REF_WORKER,
                    target_process: ProcessId::new(1),
                },
                message: MessageId::new(0),
                payload: None,
            });

        assert_loaded_admission_rejects_before_artifact_loaded(
            &program,
            "process Main transition 0 send target requires a payload-bearing message",
        );
    }

    #[test]
    fn runtime_process_lookup_indexes_by_pid() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let mut host = InMemoryRuntimeHost::default();
        let mut run = RuntimeRun::new(
            &program,
            &mut host,
            DEFAULT_MAX_RUNTIME_PROCESSES,
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        );
        let main_pid = run
            .spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(ProcessId::new(1), Some(main_pid))
            .expect("worker process should spawn");

        assert_eq!(run.process_index_for_pid(main_pid).expect("main pid"), 0);
        assert_eq!(
            run.process_index_for_pid(worker_pid).expect("worker pid"),
            1
        );

        run.send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(MessageId::new(0), None),
            Some(main_pid),
        )
        .expect("send should address worker by pid index");
        assert_eq!(run.processes[1].mailbox.len(), 1);
    }

    #[test]
    fn runtime_process_lookup_rejects_unspawned_pid() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let mut host = InMemoryRuntimeHost::default();
        let mut run = RuntimeRun::new(
            &program,
            &mut host,
            DEFAULT_MAX_RUNTIME_PROCESSES,
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        );
        run.spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");
        let missing_pid = RuntimeProcessId::from_u64(2).expect("valid pid should construct");

        let err = run
            .process_index_for_pid(missing_pid)
            .expect_err("unspawned pid should be rejected");

        assert!(err.to_string().contains("runtime process 2 is not spawned"));
    }

    #[test]
    fn runtime_rejects_unspawned_process_ref_payload() {
        let mut artifact = artifact_with_unbound_worker_process_ref();
        artifact.processes[1].message_variants =
            vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let mut host = InMemoryRuntimeHost::default();
        let mut run = RuntimeRun::new(
            &program,
            &mut host,
            DEFAULT_MAX_RUNTIME_PROCESSES,
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        );
        let main_pid = run
            .spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(ProcessId::new(1), Some(main_pid))
            .expect("worker process should spawn");

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    MessageId::new(0),
                    Some(ArtifactPayload {
                        ty: PROCESS_REF_WORKER,
                        value: "type8#99".to_string(),
                        process_ref: Some(ArtifactProcessRefPayload {
                            target_process: ProcessId::new(1),
                            pid: 99,
                        }),
                    }),
                ),
                Some(main_pid),
            )
            .expect_err("unspawned process ref payload should fail closed");

        assert!(
            err.to_string()
                .contains("runtime process 99 is not spawned")
        );
    }

    #[test]
    fn runtime_rejects_process_ref_payload_target_type_mismatch() {
        let mut artifact = artifact_with_unbound_worker_process_ref();
        artifact.processes[1].message_variants =
            vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let mut host = InMemoryRuntimeHost::default();
        let mut run = RuntimeRun::new(
            &program,
            &mut host,
            DEFAULT_MAX_RUNTIME_PROCESSES,
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        );
        let main_pid = run
            .spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");
        let worker_pid = run
            .spawn_process(ProcessId::new(1), Some(main_pid))
            .expect("worker process should spawn");

        let err = run
            .send_message(
                worker_pid,
                RuntimeMessageEnvelope::new(
                    MessageId::new(0),
                    Some(ArtifactPayload {
                        ty: PROCESS_REF_WORKER,
                        value: "type8#1".to_string(),
                        process_ref: Some(ArtifactProcessRefPayload {
                            target_process: ProcessId::new(0),
                            pid: main_pid.as_u64(),
                        }),
                    }),
                ),
                Some(main_pid),
            )
            .expect_err("process ref target type mismatch should fail closed");

        assert!(err.to_string().contains(
            "payload process reference metadata targets process id 0, expected 1 for type id 8"
        ));
    }

    #[test]
    fn runtime_rejects_oversized_record_payload_template_value() {
        let artifact = artifact_with_unbound_worker_process_ref();
        let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
        let template = ArtifactValueTemplate::Record {
            ty: BOX,
            fields: vec![ArtifactValueTemplateField {
                name: "item".to_string(),
                value: ArtifactValueTemplate::ReceivedPayload { ty: JOB },
            }],
        };
        let received = ArtifactPayload {
            ty: JOB,
            value: "a".repeat(MAX_FIELD_VALUE_BYTES),
            process_ref: None,
        };
        let step = ActiveStep {
            pid: RuntimeProcessId::FIRST,
            process_id: ProcessId::new(0),
            process_name: "Main".to_string(),
            message: MessageId::new(0),
            message_label: "Start".to_string(),
            payload: Some(received.clone()),
            current_state: StateId::new(0),
        };

        let err = evaluate_runtime_template(
            &program,
            &template,
            Some(&received),
            &step,
            &BTreeMap::new(),
        )
        .expect_err("oversized record payload labels should fail closed");

        assert!(
            err.to_string()
                .contains("payload value exceeds maximum length")
        );
    }

    fn assert_loaded_admission_rejects_before_artifact_loaded(
        program: &LoadedProgram,
        expected: &str,
    ) {
        let err = loaded_admission_error_before_artifact_loaded(program);

        assert!(
            err.contains(expected),
            "expected error containing {expected:?}, got {err}"
        );
    }

    fn loaded_admission_error_before_artifact_loaded(program: &LoadedProgram) -> String {
        let mut host = InMemoryRuntimeHost::default();

        let err = run_loaded_program_with_host(program, &mut host, RunLimits::default())
            .expect_err("loaded runtime admission should fail closed");
        let err = err.to_string();

        assert!(
            host.stdout().is_empty(),
            "loaded runtime admission failure must happen before host output"
        );
        assert!(
            host.events().is_empty(),
            "loaded runtime admission failure must happen before ArtifactLoaded"
        );
        err
    }

    fn artifact_with_unbound_worker_process_ref() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
            source_language: TEST_SOURCE_LANGUAGE.to_string(),
            module: "unbound_worker_process_ref".to_string(),
            entry_process: ProcessId::new(0),
            entry_message: MessageId::new(0),
            types: vec![
                ArtifactType::value("MainState"),
                ArtifactType::value("MainMsg"),
                ArtifactType::value("WorkerState"),
                ArtifactType::value("WorkerMsg"),
                ArtifactType::value("Job"),
                ArtifactType::value("Box"),
                ArtifactType::value("Leaf"),
                ArtifactType::value("StartPayload"),
                ArtifactType::process_ref("ProcessRef_Worker", ProcessId::new(1)),
                ArtifactType::process_ref("ProcessRef_Main", ProcessId::new(0)),
            ],
            outputs: Vec::new(),
            processes: vec![
                ArtifactProcess {
                    debug_name: "Main".to_string(),
                    state_type: MAIN_STATE,
                    state_values: state_values(MAIN_STATE, &["MainState"]),
                    message_type: MAIN_MSG,
                    message_variants: vec![ArtifactMessageVariant::unit("Start")],
                    process_refs: vec![ArtifactProcessRef {
                        debug_name: "worker".to_string(),
                        target: ProcessId::new(1),
                    }],
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(0),
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        effects: Vec::new(),
                        actions: Vec::new(),
                    }],
                },
                ArtifactProcess {
                    debug_name: "Worker".to_string(),
                    state_type: WORKER_STATE,
                    state_values: state_values(WORKER_STATE, &["Idle"]),
                    message_type: WORKER_MSG,
                    message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                    process_refs: Vec::new(),
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(0),
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        effects: Vec::new(),
                        actions: Vec::new(),
                    }],
                },
            ],
            source_hash_fnv1a64: "0000000000000000".to_string(),
        }
    }

    fn artifact_with_large_unbound_process_ref_table() -> MantleArtifact {
        let mut artifact = artifact_with_unbound_worker_process_ref();
        artifact.module = "large_process_ref_table".to_string();
        artifact.processes[0].process_refs = (0..MAX_PROCESS_REFS_PER_PROCESS)
            .map(|index| ArtifactProcessRef {
                debug_name: format!("worker_{index}"),
                target: ProcessId::new(1),
            })
            .collect();
        artifact
    }

    fn state_values(ty: TypeId, values: &[&str]) -> Vec<ArtifactStateValue> {
        values
            .iter()
            .map(|value| ArtifactStateValue::new(ty, *value))
            .collect()
    }

    fn record_template_with_depth(depth: usize) -> ArtifactValueTemplate {
        let mut template = ArtifactValueTemplate::Literal {
            ty: LEAF,
            value: "Leaf".to_string(),
        };
        for _ in 0..depth {
            template = ArtifactValueTemplate::Record {
                ty: BOX,
                fields: vec![ArtifactValueTemplateField {
                    name: "item".to_string(),
                    value: template,
                }],
            };
        }
        match &mut template {
            ArtifactValueTemplate::Record { ty, .. } => *ty = MAIN_STATE,
            _ => unreachable!("depth above zero produces a record"),
        }
        template
    }
}
