use std::collections::BTreeMap;

use mantle_artifact::{
    ArtifactBranch, ArtifactValue, Error, LoopElementId, MAX_VALUE_TEMPLATE_DEPTH, MantleArtifact,
    OutputId, ProcessId, ProcessRefId, Result, StateId, StepResult, TypeId,
};

use accounting::{checked_output_bytes, checked_trace_event_bytes};
use model::{ActiveStep, ProcessInstance, RuntimeMessageEnvelope};
use templates::evaluate_runtime_template;

use crate::event::{
    RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason, RuntimeOutputStream, RuntimeProcessId,
    RuntimeStepResult, RuntimeStopReason,
};
use crate::host::RuntimeHost;
use crate::limits::RunLimits;
use crate::program::{
    LoadedAction, LoadedNextState, LoadedProgram, LoadedSendTarget, LoadedValueTemplate,
    RuntimePayload,
};
use crate::report::{MessageDelivery, ProcessReport, ProcessStatus, RuntimeReport, SpawnReport};

mod accounting;
mod model;
mod process_refs;
mod templates;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BranchDecisionPath {
    depth: u8,
    bits: u64,
}

impl BranchDecisionPath {
    const fn root() -> Self {
        Self { depth: 0, bits: 0 }
    }

    fn child(self, branch: ArtifactBranch) -> Result<Self> {
        if usize::from(self.depth) >= MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "runtime branch nesting exceeds maximum depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        let bit = match branch {
            ArtifactBranch::Then => 0,
            ArtifactBranch::Else => 1,
        };
        Ok(Self {
            depth: self.depth + 1,
            bits: self.bits | (bit << self.depth),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BranchDecision {
    path: BranchDecisionPath,
    condition: LoadedValueTemplate,
    branch: ArtifactBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeLoopElement {
    id: LoopElementId,
    payload: RuntimePayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchDecisionMode {
    StoreForAction,
    ReuseFromNextState,
}

struct BranchSelection<'a> {
    step: &'a ActiveStep,
    condition: &'a LoadedValueTemplate,
    local_process_refs: &'a BTreeMap<ProcessRefId, RuntimeProcessId>,
    path: BranchDecisionPath,
    mode: BranchDecisionMode,
    loop_elements: &'a [RuntimeLoopElement],
}

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
    let mut run = RuntimeRun::new(program, host, limits);
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
    loop_iterations: usize,
    max_loop_iterations: usize,
    trace_bytes: usize,
    max_trace_bytes: usize,
    emitted_output_bytes: usize,
    max_emitted_output_bytes: usize,
    spawned_processes: Vec<SpawnReport>,
    delivered_messages: Vec<MessageDelivery>,
    emitted_outputs: Vec<String>,
}

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
    fn new(program: &'program LoadedProgram, host: &'host mut H, limits: RunLimits) -> Self {
        Self {
            program,
            host,
            processes: Vec::new(),
            next_pid: RuntimeProcessId::FIRST,
            max_runtime_processes: limits.max_runtime_processes,
            loop_iterations: 0,
            max_loop_iterations: limits.max_dispatches,
            trace_bytes: 0,
            max_trace_bytes: limits.max_trace_bytes,
            emitted_output_bytes: 0,
            max_emitted_output_bytes: limits.max_emitted_output_bytes,
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
            mailbox: std::collections::VecDeque::new(),
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
        let process_index = self.preflight_delivery_target(target)?;
        let process = &self.processes[process_index];
        let process_id = process.process_id;
        let process_label = self.program.process_label(process_id)?;
        envelope.validate_for_process(self.program, process_id)?;
        self.validate_envelope_process_ref(&envelope)?;
        let pid = process.pid;
        let queue_depth = process.mailbox.len() + 1;
        let message_label = self
            .program
            .message_label(process_id, envelope.message)?
            .to_string();
        let process_label = process_label.to_string();

        self.record_event(RuntimeEvent::MessageAccepted {
            pid,
            process_id,
            process: process_label.clone(),
            message_id: envelope.message,
            message: message_label.clone(),
            payload: envelope.payload.clone(),
            queue_depth,
            sender_pid,
        })?;
        let delivered_message = envelope.display_label(&message_label);
        self.processes[process_index].mailbox.push_back(envelope);
        self.delivered_messages.push(MessageDelivery {
            pid,
            process: process_label,
            message: delivered_message,
        });
        Ok(())
    }

    fn preflight_delivery_target(&self, target: RuntimeProcessId) -> Result<usize> {
        let process_index = self.process_index_for_pid(target)?;
        let process = &self.processes[process_index];
        let process_label = self.program.process_label(process.process_id)?;
        match process.status {
            ProcessStatus::Running => {}
            ProcessStatus::Stopped => {
                return Err(Error::new(format!(
                    "send to process {process_label} failed because it is stopped"
                )));
            }
            ProcessStatus::Failed => {
                return Err(Error::new(format!(
                    "send to process {process_label} failed because it has failed"
                )));
            }
        }
        if process.mailbox.len() >= process.mailbox_bound {
            return Err(Error::new(format!(
                "mailbox for process {} is full; message was not accepted",
                process_label
            )));
        }
        Ok(process_index)
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
        let transition = definition.transition_for_dispatch(
            step.message,
            step.current_state,
            step.payload.as_ref(),
        )?;
        let next_state = transition.next_state.clone();
        let step_result = transition.step_result;
        let mut local_process_refs = BTreeMap::new();
        let mut branch_decisions = Vec::new();

        let final_state = self.resolve_next_state(
            process_index,
            &step,
            &next_state,
            &mut branch_decisions,
            BranchDecisionPath::root(),
        )?;

        for action in &transition.actions {
            self.execute_action(
                &mut local_process_refs,
                &mut branch_decisions,
                &step,
                action,
                BranchDecisionPath::root(),
                &[],
            )?;
        }

        self.apply_next_state(process_index, &step, final_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action(
        &mut self,
        local_process_refs: &mut BTreeMap<ProcessRefId, RuntimeProcessId>,
        branch_decisions: &mut Vec<BranchDecision>,
        step: &ActiveStep,
        action: &LoadedAction,
        branch_path: BranchDecisionPath,
        loop_elements: &[RuntimeLoopElement],
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
                let target_process_index = self.preflight_delivery_target(pid)?;
                let target_process_id = self.processes[target_process_index].process_id;
                self.program
                    .message_payload_type(target_process_id, *message)?;
                let prepared_payload = match payload {
                    Some(payload) => Some(evaluate_runtime_template(
                        self.program,
                        payload,
                        step.payload.as_ref(),
                        step,
                        local_process_refs,
                        loop_elements,
                    )?),
                    None => None,
                };
                self.send_message(
                    pid,
                    RuntimeMessageEnvelope::new(*message, prepared_payload),
                    Some(step.pid),
                )
            }
            LoadedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                let branch = self.select_branch(
                    BranchSelection {
                        step,
                        condition,
                        local_process_refs,
                        path: branch_path,
                        mode: BranchDecisionMode::ReuseFromNextState,
                        loop_elements,
                    },
                    branch_decisions,
                )?;
                let selected_actions = match branch {
                    ArtifactBranch::Then => then_actions,
                    ArtifactBranch::Else => else_actions,
                };
                let selected_branch_path = branch_path.child(branch)?;
                for action in selected_actions {
                    self.execute_action(
                        local_process_refs,
                        branch_decisions,
                        step,
                        action,
                        selected_branch_path,
                        loop_elements,
                    )?;
                }
                Ok(())
            }
            LoadedAction::ForEach {
                element,
                collection,
                max_items,
                body,
            } => {
                let collection = evaluate_runtime_template(
                    self.program,
                    collection,
                    step.payload.as_ref(),
                    step,
                    local_process_refs,
                    loop_elements,
                )?;
                let collection_type = collection.ty;
                let items = match collection.value {
                    ArtifactValue::List(items) => items,
                    value => {
                        return Err(Error::new(format!(
                            "process {} for loop collection produced non-list value {}",
                            step.process_name,
                            value.label()
                        )));
                    }
                };
                let item_count = items.len();
                if item_count > *max_items {
                    return Err(Error::new(format!(
                        "process {} for loop collection has {} item(s), max_items is {}",
                        step.process_name, item_count, max_items
                    )));
                }
                self.ensure_loop_iteration_budget(item_count)?;
                self.record_loop_started(
                    step,
                    element.id,
                    collection_type,
                    *max_items,
                    item_count,
                )?;
                for (index, item) in items.into_iter().enumerate() {
                    self.consume_loop_iteration()?;
                    let payload = RuntimePayload::value(element.ty, item)?;
                    self.record_loop_iteration(step, element.id, index, &payload)?;
                    let active = [RuntimeLoopElement {
                        id: element.id,
                        payload,
                    }];
                    for action in body {
                        self.execute_action(
                            local_process_refs,
                            branch_decisions,
                            step,
                            action,
                            branch_path,
                            &active,
                        )?;
                    }
                }
                self.record_loop_completed(step, element.id, item_count)?;
                Ok(())
            }
        }
    }

    fn ensure_loop_iteration_budget(&self, item_count: usize) -> Result<()> {
        let remaining = self
            .max_loop_iterations
            .checked_sub(self.loop_iterations)
            .ok_or_else(|| Error::new("runtime loop iteration counter exceeded its budget"))?;
        if item_count > remaining {
            return Err(Error::new(format!(
                "runtime loop iteration budget exceeded: loop requires {item_count} iteration(s), remaining budget is {remaining}"
            )));
        }
        Ok(())
    }

    fn evaluate_bool_condition(
        &self,
        step: &ActiveStep,
        condition: &LoadedValueTemplate,
        local_process_refs: &BTreeMap<ProcessRefId, RuntimeProcessId>,
        loop_elements: &[RuntimeLoopElement],
    ) -> Result<(ArtifactBranch, RuntimePayload)> {
        let condition_value = evaluate_runtime_template(
            self.program,
            condition,
            step.payload.as_ref(),
            step,
            local_process_refs,
            loop_elements,
        )?;
        if condition_value.ty != condition.result_type() {
            return Err(Error::new(format!(
                "process {} if condition produced type id {}, expected {}",
                step.process_name,
                condition_value.ty.as_u32(),
                condition.result_type().as_u32()
            )));
        }
        let ArtifactValue::Atom(value) = &condition_value.value else {
            return Err(Error::new(format!(
                "process {} if condition produced non-Bool value {}",
                step.process_name,
                condition_value.label()
            )));
        };
        let branch = match value.as_str() {
            "True" => ArtifactBranch::Then,
            "False" => ArtifactBranch::Else,
            _ => {
                return Err(Error::new(format!(
                    "process {} if condition produced invalid Bool value {}",
                    step.process_name,
                    condition_value.label()
                )));
            }
        };
        Ok((branch, condition_value))
    }

    fn select_branch(
        &mut self,
        selection: BranchSelection<'_>,
        branch_decisions: &mut Vec<BranchDecision>,
    ) -> Result<ArtifactBranch> {
        if selection.mode == BranchDecisionMode::ReuseFromNextState {
            if let Some(index) = branch_decisions.iter().rposition(|decision| {
                decision.path == selection.path && decision.condition == *selection.condition
            }) {
                return Ok(branch_decisions.remove(index).branch);
            }
        }

        let (branch, condition_value) = self.evaluate_bool_condition(
            selection.step,
            selection.condition,
            selection.local_process_refs,
            selection.loop_elements,
        )?;
        self.record_branch_selected(selection.step, branch, &condition_value)?;
        if selection.mode == BranchDecisionMode::StoreForAction {
            branch_decisions.push(BranchDecision {
                path: selection.path,
                condition: selection.condition.clone(),
                branch,
            });
        }
        Ok(branch)
    }

    fn record_branch_selected(
        &mut self,
        step: &ActiveStep,
        branch: ArtifactBranch,
        condition: &RuntimePayload,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::BranchSelected {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            branch,
            condition_type_id: condition.ty,
            condition: condition.label().to_string(),
        })
    }

    fn consume_loop_iteration(&mut self) -> Result<()> {
        if self.loop_iterations >= self.max_loop_iterations {
            return Err(Error::new(format!(
                "runtime loop iteration budget exceeded after {} iteration(s)",
                self.max_loop_iterations
            )));
        }
        self.loop_iterations = self
            .loop_iterations
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime loop iteration counter overflowed"))?;
        Ok(())
    }

    fn record_loop_started(
        &mut self,
        step: &ActiveStep,
        element_id: LoopElementId,
        collection_type_id: TypeId,
        max_items: usize,
        item_count: usize,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::LoopStarted {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            element_id,
            collection_type_id,
            max_items,
            item_count,
        })
    }

    fn record_loop_iteration(
        &mut self,
        step: &ActiveStep,
        element_id: LoopElementId,
        index: usize,
        element: &RuntimePayload,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::LoopIteration {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            element_id,
            index,
            element_type_id: element.ty,
            element: element.label().to_string(),
        })
    }

    fn record_loop_completed(
        &mut self,
        step: &ActiveStep,
        element_id: LoopElementId,
        iteration_count: usize,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::LoopCompleted {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            element_id,
            iteration_count,
        })
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
        template: &LoadedValueTemplate,
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
                    step.process_name,
                    value.label()
                ))
            })?;
        StateId::from_index(state_index)
    }

    fn evaluate_state_template(
        &self,
        template: &LoadedValueTemplate,
        step: &ActiveStep,
    ) -> Result<RuntimePayload> {
        evaluate_runtime_template(
            self.program,
            template,
            step.payload.as_ref(),
            step,
            &BTreeMap::new(),
            &[],
        )
    }

    fn resolve_next_state(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        next_state: &LoadedNextState,
        branch_decisions: &mut Vec<BranchDecision>,
        branch_path: BranchDecisionPath,
    ) -> Result<StateId> {
        match next_state {
            LoadedNextState::Current => Ok(self.processes[process_index].state),
            LoadedNextState::Value(state) => Ok(*state),
            LoadedNextState::Template(template) => self.resolve_template_state(step, template),
            LoadedNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                let empty_process_refs = BTreeMap::new();
                let branch = self.select_branch(
                    BranchSelection {
                        step,
                        condition,
                        local_process_refs: &empty_process_refs,
                        path: branch_path,
                        mode: BranchDecisionMode::StoreForAction,
                        loop_elements: &[],
                    },
                    branch_decisions,
                )?;
                let selected_state = match branch {
                    ArtifactBranch::Then => then_state,
                    ArtifactBranch::Else => else_state,
                };
                self.resolve_next_state(
                    process_index,
                    step,
                    selected_state,
                    branch_decisions,
                    branch_path.child(branch)?,
                )
            }
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

#[cfg(test)]
mod tests;
