use std::collections::{BTreeMap, VecDeque};

use mantle_artifact::{
    ArtifactPayload, ArtifactValueTemplate, Error, MantleArtifact, MessageId, NextState, OutputId,
    ProcessId, ProcessRefId, Result, StateId, StepResult,
};

use crate::event::{
    RuntimeEvent, RuntimeEventRecord, RuntimeOutputStream, RuntimeProcessId, RuntimeStepResult,
    RuntimeStopReason,
};
use crate::host::RuntimeHost;
use crate::limits::RunLimits;
use crate::program::{LoadedAction, LoadedProgram};
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
        let transition = definition.transition_for_message(step.message)?;
        let next_state = transition.next_state.clone();
        let step_result = transition.step_result;
        let final_state = self.resolve_next_state(process_index, &step, &next_state)?;
        let action_payloads = prepare_action_payloads(&step, &transition.actions)?;
        let mut local_process_refs = BTreeMap::new();

        for (action, prepared_payload) in transition.actions.iter().zip(action_payloads) {
            self.execute_action(&mut local_process_refs, &step, action, prepared_payload)?;
        }

        self.apply_next_state(process_index, &step, final_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action(
        &mut self,
        local_process_refs: &mut BTreeMap<ProcessRefId, RuntimeProcessId>,
        step: &ActiveStep,
        action: &LoadedAction,
        prepared_payload: Option<ArtifactPayload>,
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
                payload: _,
            } => {
                let pid = self.resolve_process_ref(local_process_refs, step, *target)?;
                self.send_message(
                    pid,
                    RuntimeMessageEnvelope::new(*message, prepared_payload),
                    Some(step.pid),
                )
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
        let value = evaluate_runtime_template(template, step.payload.as_ref())?;
        let process = self.program.process(step.process_id)?;
        let state_index = process
            .state_values
            .iter()
            .position(|state| state == &value.value)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} next_state template produced value {} not admitted by state table",
                    step.process_name, value.value
                ))
            })?;
        StateId::from_index(state_index)
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
        if step_result == StepResult::Stop {
            self.record_event(RuntimeEvent::ProcessStopped {
                pid: step.pid,
                process_id: step.process_id,
                process: step.process_name.clone(),
                reason: RuntimeStopReason::Normal,
            })?;
            self.processes[process_index].status = ProcessStatus::Stopped;
        }
        Ok(())
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
                        "message id {} for process id {} payload has type {}, expected {}",
                        self.message.as_u32(),
                        process_id.as_u32(),
                        payload.ty,
                        expected_type
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
        Ok(Self {
            pid: process.pid,
            process_id: process.process_id,
            process_name: program.process_label(process.process_id)?.to_string(),
            message: envelope.message,
            message_label: program
                .message_label(process.process_id, envelope.message)?
                .to_string(),
            payload: envelope.payload,
        })
    }
}

fn evaluate_runtime_template(
    template: &ArtifactValueTemplate,
    received_payload: Option<&ArtifactPayload>,
) -> Result<ArtifactPayload> {
    template.evaluate(received_payload)
}

fn prepare_action_payloads(
    step: &ActiveStep,
    actions: &[LoadedAction],
) -> Result<Vec<Option<ArtifactPayload>>> {
    actions
        .iter()
        .map(|action| match action {
            LoadedAction::Send { payload, .. } => payload
                .as_ref()
                .map(|payload| evaluate_runtime_template(payload, step.payload.as_ref()))
                .transpose(),
            _ => Ok(None),
        })
        .collect()
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
        ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef, ArtifactTransition,
        StepResult, ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, MAX_PROCESS_REFS_PER_PROCESS,
        STRATA_SOURCE_LANGUAGE,
    };

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
    fn runtime_process_lookup_indexes_by_pid() {
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
        run.spawn_process(ProcessId::new(0), None)
            .expect("entry process should spawn");
        let missing_pid = RuntimeProcessId::from_u64(2).expect("valid pid should construct");

        let err = run
            .process_index_for_pid(missing_pid)
            .expect_err("unspawned pid should be rejected");

        assert!(err.to_string().contains("runtime process 2 is not spawned"));
    }

    fn artifact_with_large_unbound_process_ref_table() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
            source_language: STRATA_SOURCE_LANGUAGE.to_string(),
            module: "large_process_ref_table".to_string(),
            entry_process: ProcessId::new(0),
            entry_message: MessageId::new(0),
            outputs: Vec::new(),
            processes: vec![
                ArtifactProcess {
                    debug_name: "Main".to_string(),
                    state_type: "MainState".to_string(),
                    state_values: vec!["MainState".to_string()],
                    message_type: "MainMsg".to_string(),
                    message_variants: vec![ArtifactMessageVariant::unit("Start")],
                    process_refs: (0..MAX_PROCESS_REFS_PER_PROCESS)
                        .map(|index| ArtifactProcessRef {
                            debug_name: format!("worker_{index}"),
                            target: ProcessId::new(1),
                        })
                        .collect(),
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        message: MessageId::new(0),
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        actions: Vec::new(),
                    }],
                },
                ArtifactProcess {
                    debug_name: "Worker".to_string(),
                    state_type: "WorkerState".to_string(),
                    state_values: vec!["Idle".to_string()],
                    message_type: "WorkerMsg".to_string(),
                    message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                    process_refs: Vec::new(),
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        message: MessageId::new(0),
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        actions: Vec::new(),
                    }],
                },
            ],
            source_hash_fnv1a64: "0000000000000000".to_string(),
        }
    }
}
