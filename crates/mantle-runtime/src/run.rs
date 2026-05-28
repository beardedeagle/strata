use mantle_artifact::{
    ArtifactBranch, ArtifactValue, Error, LoopElementId, MantleArtifact, OutputId, ProcessId,
    Result, StateId, StepResult,
};

use accounting::{checked_output_bytes, checked_trace_event_bytes};
use model::{ActiveStep, ProcessInstance, RuntimeMessageEnvelope};
use process_refs::LocalProcessRefs;
use templates::evaluate_runtime_template;

use crate::event::{
    RuntimeAuthorityResult, RuntimeBranchPath, RuntimeBranchPathSegment, RuntimeBranchScope,
    RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason, RuntimeOutputStream, RuntimeProcessId,
    RuntimeSpawnKind, RuntimeStepResult, RuntimeStopReason, RuntimeSupervisorExitReason,
};
use crate::host::RuntimeHost;
use crate::limits::{RunLimits, SpawnAuthorityPolicy};
use crate::program::{
    LoadedAction, LoadedNextState, LoadedProgram, LoadedSpawnKind, LoadedValueTemplate,
    RuntimePayload,
};
use crate::report::{MessageDelivery, ProcessReport, ProcessStatus, RuntimeReport, SpawnReport};

mod accounting;
mod control_flow;
mod delivery;
mod effect_outcomes;
mod model;
mod process_lifecycle;
mod process_refs;
mod supervision;
mod templates;

use effect_outcomes::RuntimeEffectOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeLoopElement<'a> {
    id: LoopElementId,
    index: usize,
    payload: &'a RuntimePayload,
}

struct BranchSelection<'a> {
    step: &'a ActiveStep,
    scope: RuntimeBranchScope,
    branch_path: RuntimeBranchPath,
    condition: &'a LoadedValueTemplate,
    local_process_refs: &'a LocalProcessRefs,
    loop_elements: &'a [RuntimeLoopElement<'a>],
    effect_outcomes: &'a [RuntimeEffectOutcome],
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
    spawn_authority_policy: SpawnAuthorityPolicy,
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
            spawn_authority_policy: limits.spawn_authority_policy,
            spawned_processes: Vec::new(),
            delivered_messages: Vec::new(),
            emitted_outputs: Vec::new(),
        }
    }

    pub(super) fn is_spawn_authority_admitted(&self) -> bool {
        matches!(
            self.spawn_authority_policy,
            SpawnAuthorityPolicy::AdmitDeclared
        )
    }

    fn record_event(&mut self, event: RuntimeEvent) -> Result<()> {
        let record = RuntimeEventRecord::new(event)?;
        let event_bytes = checked_trace_event_bytes(self.trace_bytes, &record)?;
        if event_bytes > self.max_trace_bytes {
            return Err(Error::new(format!(
                "runtime trace exceeded maximum size of {} bytes",
                self.max_trace_bytes
            )));
        }
        self.host.record_event(record)?;
        self.trace_bytes = event_bytes;
        Ok(())
    }

    pub(super) fn record_spawn_authority(
        &mut self,
        step: &ActiveStep,
        target: ProcessId,
        spawn_site: mantle_artifact::SpawnSiteId,
    ) -> Result<bool> {
        let process = self.program.process(step.process_id)?;
        let site = process.validate_spawn_site(spawn_site, target)?;
        let authority_id = site.authority.ok_or_else(|| {
            Error::new(format!(
                "process {} dynamic spawn site id {} does not reference an authority",
                step.process_name,
                spawn_site.as_u32()
            ))
        })?;
        let admitted = self.is_spawn_authority_admitted();
        self.record_event(RuntimeEvent::SpawnAuthorityChecked {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            target_process_id: target,
            spawn_site_id: spawn_site,
            authority_id,
            spawn_kind: match site.kind {
                LoadedSpawnKind::DynamicLocal => RuntimeSpawnKind::DynamicLocal,
                LoadedSpawnKind::LexicalSupervisorChild => RuntimeSpawnKind::LexicalSupervisorChild,
            },
            authority_result: if admitted {
                RuntimeAuthorityResult::Accepted
            } else {
                RuntimeAuthorityResult::Denied
            },
        })?;
        Ok(admitted)
    }

    fn flush_host(&mut self) -> Result<()> {
        self.host.flush()
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
        let mut local_process_refs = LocalProcessRefs::new(definition.process_refs.len());
        let mut effect_outcomes = Vec::new();
        let mut preexecuted_prefix_len = 0usize;

        let mut prestate_prefix_open = true;
        for (action_index, action) in transition.actions.iter().enumerate() {
            if prestate_prefix_open && is_prestate_prefix_action(action) {
                self.execute_prestate_action(
                    &mut local_process_refs,
                    &step,
                    action,
                    &mut effect_outcomes,
                )?;
                preexecuted_prefix_len = action_index.saturating_add(1);
                continue;
            }
            prestate_prefix_open = false;
            if is_effect_outcome_action(action) {
                return Err(Error::new(format!(
                    "process {} effect outcome action appears after ordinary effects",
                    step.process_name
                )));
            }
        }

        let final_state = self.resolve_next_state(
            process_index,
            &step,
            &next_state,
            RuntimeBranchPath::root(),
            &effect_outcomes,
        )?;

        for (action_index, action) in transition.actions.iter().enumerate() {
            if action_index < preexecuted_prefix_len {
                continue;
            }
            self.execute_action(
                &mut local_process_refs,
                &step,
                action,
                RuntimeBranchPath::root().child(RuntimeBranchPathSegment::action(action_index)?)?,
                &[],
                &effect_outcomes,
            )?;
        }

        self.apply_next_state(process_index, &step, final_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action(
        &mut self,
        local_process_refs: &mut LocalProcessRefs,
        step: &ActiveStep,
        action: &LoadedAction,
        branch_path: RuntimeBranchPath,
        loop_elements: &[RuntimeLoopElement<'_>],
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<()> {
        match action {
            LoadedAction::Emit { output } => self.emit_output(step, *output),
            LoadedAction::Spawn {
                target,
                process_ref,
                spawn_site,
            } => {
                if !self.record_spawn_authority(step, *target, *spawn_site)? {
                    return Err(Error::new(format!(
                        "process {} spawn authority denied for process id {}",
                        step.process_name,
                        target.as_u32()
                    )));
                }
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
            LoadedAction::SpawnOutcome { .. } | LoadedAction::SendOutcome { .. } => {
                Err(Error::new(format!(
                    "process {} nested effect outcome action reached ordinary execution",
                    step.process_name
                )))
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
                        effect_outcomes,
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
                let branch = self.select_branch(BranchSelection {
                    step,
                    scope: RuntimeBranchScope::Action,
                    branch_path,
                    condition,
                    local_process_refs,
                    loop_elements,
                    effect_outcomes,
                })?;
                let selected_actions = match branch {
                    ArtifactBranch::Then => then_actions,
                    ArtifactBranch::Else => else_actions,
                };
                for (action_index, action) in selected_actions.iter().enumerate() {
                    let selected_branch_path = branch_path.child(
                        RuntimeBranchPathSegment::branch_action(branch, action_index)?,
                    )?;
                    self.execute_action(
                        local_process_refs,
                        step,
                        action,
                        selected_branch_path,
                        loop_elements,
                        effect_outcomes,
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
                    effect_outcomes,
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
                let loop_payloads = items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        self.program.runtime_payload_value(
                            &format!(
                                "process {} for loop element {} item {index}",
                                step.process_name,
                                element.id.as_u32()
                            ),
                            element.ty,
                            item,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.preflight_loop_body(
                    local_process_refs,
                    step,
                    element,
                    body,
                    &loop_payloads,
                    effect_outcomes,
                )?;
                self.record_loop_started(
                    step,
                    element.id,
                    collection_type,
                    *max_items,
                    item_count,
                )?;
                for (index, payload) in loop_payloads.iter().enumerate() {
                    self.consume_loop_iteration()?;
                    self.record_loop_iteration(step, element.id, index, payload)?;
                    let active = [RuntimeLoopElement {
                        id: element.id,
                        index,
                        payload,
                    }];
                    for (action_index, action) in body.iter().enumerate() {
                        let body_action_path = branch_path
                            .child(RuntimeBranchPathSegment::loop_body_action(action_index)?)?;
                        self.execute_action(
                            local_process_refs,
                            step,
                            action,
                            body_action_path,
                            &active,
                            effect_outcomes,
                        )?;
                    }
                }
                self.record_loop_completed(step, element.id, item_count)?;
                Ok(())
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

    fn resolve_template_state(
        &self,
        step: &ActiveStep,
        template: &LoadedValueTemplate,
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<StateId> {
        let value = self.evaluate_state_template(template, step, effect_outcomes)?;
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
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<RuntimePayload> {
        evaluate_runtime_template(
            self.program,
            template,
            step.payload.as_ref(),
            step,
            &LocalProcessRefs::empty(),
            &[],
            effect_outcomes,
        )
    }

    fn resolve_next_state(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        next_state: &LoadedNextState,
        branch_path: RuntimeBranchPath,
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<StateId> {
        match next_state {
            LoadedNextState::Current => Ok(self.processes[process_index].state),
            LoadedNextState::Value(state) => Ok(*state),
            LoadedNextState::Template(template) => {
                self.resolve_template_state(step, template, effect_outcomes)
            }
            LoadedNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                let empty_process_refs = LocalProcessRefs::empty();
                let branch = self.select_branch(BranchSelection {
                    step,
                    scope: RuntimeBranchScope::NextState,
                    branch_path,
                    condition,
                    local_process_refs: &empty_process_refs,
                    loop_elements: &[],
                    effect_outcomes,
                })?;
                let selected_state = match branch {
                    ArtifactBranch::Then => then_state,
                    ArtifactBranch::Else => else_state,
                };
                self.resolve_next_state(
                    process_index,
                    step,
                    selected_state,
                    branch_path.child(RuntimeBranchPathSegment::next_state_branch(branch))?,
                    effect_outcomes,
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
                self.stop_supervised_children(step.pid, RuntimeStopReason::SupervisorShutdown)?;
                self.record_event(RuntimeEvent::ProcessStopped {
                    pid: step.pid,
                    process_id: step.process_id,
                    process: step.process_name.clone(),
                    reason: RuntimeStopReason::Normal,
                })?;
                self.processes[process_index].status = ProcessStatus::Stopped;
                self.handle_supervised_exit(
                    process_index,
                    step.pid,
                    step.process_id,
                    &step.process_name,
                    RuntimeSupervisorExitReason::Normal,
                )
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
                self.stop_supervised_children(step.pid, RuntimeStopReason::SupervisorFailure)?;
                match self.handle_supervised_exit(
                    process_index,
                    step.pid,
                    step.process_id,
                    &step.process_name,
                    RuntimeSupervisorExitReason::Panic,
                ) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        self.flush_host()?;
                        Err(Error::new(format!(
                            "process {} panicked after consuming message {}; message will not be replayed: {err}",
                            step.process_name, step.message_label
                        )))
                    }
                }
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

const fn is_prestate_prefix_action(action: &LoadedAction) -> bool {
    matches!(
        action,
        LoadedAction::Spawn { .. }
            | LoadedAction::SpawnOutcome { .. }
            | LoadedAction::SendOutcome { .. }
    )
}

const fn is_effect_outcome_action(action: &LoadedAction) -> bool {
    matches!(
        action,
        LoadedAction::SpawnOutcome { .. } | LoadedAction::SendOutcome { .. }
    )
}

#[cfg(test)]
mod tests;
