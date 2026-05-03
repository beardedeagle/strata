use std::collections::VecDeque;

use mantle_artifact::{
    Error, MantleArtifact, MessageId, NextState, OutputId, ProcessId, Result, StateId, StepResult,
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
        limits.max_trace_bytes,
        limits.max_emitted_output_bytes,
    );
    run.record_event(RuntimeEvent::ArtifactLoaded {
        format: program.format.clone(),
        format_version: program.format_version.clone(),
        source_language: program.source_language.clone(),
        module: program.module.clone(),
        entry_process_id: program.entry_process,
        entry_process: program.process_label(program.entry_process)?.to_string(),
        entry_message_id: program.entry_message,
        process_count: program.processes.len(),
    })?;
    run.spawn_process(program.entry_process, None)?;
    run.send_message(program.entry_process, program.entry_message, None)?;
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
        max_trace_bytes: usize,
        max_emitted_output_bytes: usize,
    ) -> Self {
        Self {
            program,
            host,
            processes: Vec::new(),
            next_pid: RuntimeProcessId::FIRST,
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
    ) -> Result<()> {
        let definition = self.program.process(process_id)?;
        if self
            .processes
            .iter()
            .any(|process| process.process_id == process_id)
        {
            return Err(Error::new(format!(
                "process {} is already spawned",
                definition.debug_name
            )));
        }

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
        Ok(())
    }

    fn send_message(
        &mut self,
        target: ProcessId,
        message: MessageId,
        sender_pid: Option<RuntimeProcessId>,
    ) -> Result<()> {
        let target_process = self.program.process(target)?;
        let message_label = self.program.message_label(target, message)?.to_string();
        let process_label = target_process.debug_name.clone();

        let process_index = self
            .processes
            .iter()
            .position(|process| process.process_id == target)
            .ok_or_else(|| Error::new(format!("process {process_label} is not spawned")))?;
        let process = &self.processes[process_index];
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
            process_id: target,
            process: process_label.clone(),
            message_id: message,
            message: message_label.clone(),
            queue_depth,
            sender_pid,
        })?;
        self.processes[process_index].mailbox.push_back(message);
        self.delivered_messages.push(MessageDelivery {
            pid,
            process: process_label,
            message: message_label,
        });
        Ok(())
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
                message_id: dequeued.message,
                message: self
                    .program
                    .message_label(dequeued.process_id, dequeued.message)?
                    .to_string(),
                queue_depth: dequeued.queue_depth,
            })?;
            self.step_process(process_index, dequeued.message)?;
            dispatches += 1;
        }
        Ok(())
    }

    fn next_runnable_process(&self) -> Option<usize> {
        self.processes.iter().position(|process| {
            process.status == ProcessStatus::Running && !process.mailbox.is_empty()
        })
    }

    fn step_process(&mut self, process_index: usize, message: MessageId) -> Result<()> {
        if self.processes[process_index].status != ProcessStatus::Running {
            let process_name = self
                .program
                .process_label(self.processes[process_index].process_id)?;
            return Err(Error::new(format!(
                "process {process_name} cannot step because it is not running"
            )));
        }

        let step = ActiveStep::new(self.program, &self.processes[process_index], message)?;
        let definition = self.program.process(step.process_id)?;
        let transition = definition.transition_for_message(message)?;
        let next_state = transition.next_state;
        let step_result = transition.step_result;

        for &action in &transition.actions {
            self.execute_action(&step, action)?;
        }

        self.apply_next_state(process_index, &step, next_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action(&mut self, step: &ActiveStep, action: LoadedAction) -> Result<()> {
        match action {
            LoadedAction::Emit { output } => self.emit_output(step, output),
            LoadedAction::Spawn { target } => self.spawn_process(target, Some(step.pid)),
            LoadedAction::Send { target, message } => {
                self.send_message(target, message, Some(step.pid))
            }
        }
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

    fn apply_next_state(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        next_state: NextState,
    ) -> Result<()> {
        let final_state = match next_state {
            NextState::Current => self.processes[process_index].state,
            NextState::Value(state) => state,
        };
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
    mailbox: VecDeque<MessageId>,
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
            message: removed,
            queue_depth,
        })
    }
}

struct DequeuedMessage {
    pid: RuntimeProcessId,
    process_id: ProcessId,
    message: MessageId,
    queue_depth: usize,
}

struct ActiveStep {
    pid: RuntimeProcessId,
    process_id: ProcessId,
    process_name: String,
    message: MessageId,
    message_label: String,
}

impl ActiveStep {
    fn new(program: &LoadedProgram, process: &ProcessInstance, message: MessageId) -> Result<Self> {
        Ok(Self {
            pid: process.pid,
            process_id: process.process_id,
            process_name: program.process_label(process.process_id)?.to_string(),
            message,
            message_label: program
                .message_label(process.process_id, message)?
                .to_string(),
        })
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
