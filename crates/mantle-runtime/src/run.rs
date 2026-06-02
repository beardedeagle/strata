use mantle_artifact::{
    ArtifactBranch, ArtifactValue, Error, MantleArtifact, OutputId, ProcessId, Result, StateId,
    StepResult,
};

use accounting::{checked_output_bytes, checked_trace_event_bytes};
use model::{ActiveStep, ProcessInstance, RuntimeLoopElement, RuntimeMessageEnvelope};
use process_refs::LocalProcessRefs;
use templates::{RuntimeTemplateContext, evaluate_runtime_template};

use crate::event::{
    RuntimeAuthorityResult, RuntimeBranchPath, RuntimeBranchPathSegment, RuntimeBranchScope,
    RuntimeEvent, RuntimeEventRecord, RuntimeFailureReason, RuntimeOutputStream, RuntimeProcessId,
    RuntimeSpawnKind, RuntimeStepResult, RuntimeStopReason, RuntimeSupervisorExitReason,
};
use crate::executable::{
    ExecutableActionPlan, ExecutableNextState, ExecutableProgram, ExecutableSpawnSite,
    ExecutableValueTemplateRef,
};
use crate::host::RuntimeHost;
use crate::limits::{LocalSpawnBackend, RunLimits, SpawnAuthorityPolicy};
use crate::program::{LoadedProgram, LoadedSpawnKind, RuntimePayload};
use crate::report::{MessageDelivery, ProcessReport, ProcessStatus, RuntimeReport, SpawnReport};

mod accounting;
mod action_scope;
mod boundaries;
mod control_flow;
mod delivery;
mod effect_outcomes;
mod model;
mod process_lifecycle;
mod process_refs;
mod supervision;
mod templates;

use action_scope::{BranchSelection, RuntimeActionScope};
use boundaries::BoundarySendContext;
use effect_outcomes::RuntimeEffectOutcome;

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
    let executable = ExecutableProgram::from_admitted(program)?;
    let mut run = RuntimeRun::new(program, &executable, host, limits);
    let entry = executable.entry();
    run.record_event(RuntimeEvent::ArtifactLoaded {
        format: program.format.clone(),
        schema_version: program.schema_version.clone(),
        source_language: program.source_language.clone(),
        module: program.module.clone(),
        entry_process_id: entry.process_id,
        entry_process: entry.process_label.to_string(),
        entry_message_id: entry.message_id,
        process_count: run.executable.process_count(),
    })?;
    let entry_pid = run.spawn_process(entry.process_id, None)?;
    run.send_message(
        entry_pid,
        RuntimeMessageEnvelope::new(entry.message_id, None),
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
        entry_process: entry.process_label.to_string(),
        entry_message: entry.message_label.to_string(),
        spawned_processes: run.spawned_processes,
        delivered_messages: run.delivered_messages,
        processes: process_reports,
        emitted_outputs: run.emitted_outputs,
    })
}

struct RuntimeRun<'program, 'plan, 'host, H: RuntimeHost> {
    program: &'program LoadedProgram,
    executable: &'plan ExecutableProgram<'program>,
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
    local_spawn_backend: LocalSpawnBackend,
    spawned_processes: Vec<SpawnReport>,
    delivered_messages: Vec<MessageDelivery>,
    emitted_outputs: Vec<String>,
}

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    fn new(
        program: &'program LoadedProgram,
        executable: &'plan ExecutableProgram<'program>,
        host: &'host mut H,
        limits: RunLimits,
    ) -> Self {
        Self {
            program,
            executable,
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
            local_spawn_backend: limits.local_spawn_backend,
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

    pub(super) fn is_local_spawn_backend_available(&self) -> bool {
        matches!(self.local_spawn_backend, LocalSpawnBackend::Available)
    }

    pub(super) fn ensure_local_spawn_backend_available(
        &self,
        process_name: &str,
        target: ProcessId,
    ) -> Result<()> {
        if self.is_local_spawn_backend_available() {
            return Ok(());
        }
        Err(Error::new(format!(
            "process {} local spawn backend unavailable for process id {}",
            process_name,
            target.as_u32()
        )))
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
        spawn: ExecutableSpawnSite,
    ) -> Result<bool> {
        let admitted = self.is_spawn_authority_admitted();
        self.record_event(RuntimeEvent::SpawnAuthorityChecked {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            target_process_id: target,
            spawn_site_id: spawn.id,
            authority_id: spawn.authority,
            spawn_kind: match spawn.kind {
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
        let executable = self.executable;
        let executable_actions = executable.actions();
        let executable_process = executable.process(step.process_id)?;
        let executable_transition = executable.transition_for_dispatch(
            step.process_id,
            step.message,
            step.current_state,
            step.payload.as_ref(),
        )?;
        let process_ref_count = executable_process.process_ref_count();
        let next_state = executable_transition.next_state();
        let step_result = executable_transition.step_result();
        let actions = executable_transition.actions();
        let mut local_process_refs = LocalProcessRefs::new(process_ref_count);
        let mut effect_outcomes = Vec::new();

        for action in actions.prestate_actions(executable_actions) {
            self.execute_prestate_plan_action(
                &mut local_process_refs,
                &step,
                action,
                executable.templates(),
                &mut effect_outcomes,
            )?;
        }

        let final_state = self.resolve_next_state(
            process_index,
            &step,
            next_state,
            RuntimeBranchPath::root(),
            &effect_outcomes,
        )?;

        for (action_index, action) in actions.poststate_actions(executable_actions) {
            self.execute_action_plan(
                &mut local_process_refs,
                &step,
                action,
                RuntimeBranchPath::root().child(RuntimeBranchPathSegment::action(action_index)?)?,
                RuntimeActionScope {
                    executable_actions,
                    executable_templates: executable.templates(),
                    loop_elements: &[],
                    effect_outcomes: &effect_outcomes,
                },
            )?;
        }

        self.apply_next_state(process_index, &step, final_state)?;
        self.record_step_completion(process_index, &step, step_result)
    }

    fn execute_action_plan<'template>(
        &mut self,
        local_process_refs: &mut LocalProcessRefs,
        step: &ActiveStep,
        action: &ExecutableActionPlan<'template>,
        branch_path: RuntimeBranchPath,
        scope: RuntimeActionScope<'_, 'template>,
    ) -> Result<()> {
        match action {
            ExecutableActionPlan::Emit { output } => self.emit_output(step, *output),
            ExecutableActionPlan::Spawn {
                target,
                process_ref,
                spawn,
            } => {
                if process_ref.target_process != *target {
                    return Err(Error::new(format!(
                        "process {} executable process reference id {} targets process id {}, expected {}",
                        step.process_name,
                        process_ref.id.as_u32(),
                        process_ref.target_process.as_u32(),
                        target.as_u32()
                    )));
                }
                if !self.record_spawn_authority(step, *target, *spawn)? {
                    return Err(Error::new(format!(
                        "process {} spawn authority denied for process id {}",
                        step.process_name,
                        target.as_u32()
                    )));
                }
                self.ensure_local_spawn_backend_available(&step.process_name, *target)?;
                self.ensure_process_ref_unbound(local_process_refs, step, process_ref.id)?;
                let pid = self.spawn_process(*target, Some(step.pid))?;
                self.bind_process_ref(local_process_refs, step, process_ref.id, pid)?;
                Ok(())
            }
            ExecutableActionPlan::SpawnOutcome { .. }
            | ExecutableActionPlan::SendOutcome { .. } => Err(Error::new(format!(
                "process {} nested effect outcome action reached ordinary execution",
                step.process_name
            ))),
            ExecutableActionPlan::Send {
                target,
                port,
                message,
                payload,
            } => {
                let pid = self.resolve_send_target(local_process_refs, step, target)?;
                let target_process_index = self.preflight_delivery_target(pid)?;
                let target_process_id = self.processes[target_process_index].process_id;
                self.program
                    .message_payload_type(target_process_id, *message)?;
                if let Some(port) = port {
                    self.program.validate_boundary_send(
                        step.process_name.as_str(),
                        *port,
                        target_process_id,
                        *message,
                    )?;
                }
                let prepared_payload = match payload {
                    Some(payload) => Some(evaluate_runtime_template(
                        RuntimeTemplateContext {
                            program: self.program,
                            templates: scope.executable_templates,
                            received_payload: step.payload.as_ref(),
                            step,
                            process_refs: local_process_refs,
                            loop_elements: scope.loop_elements,
                            effect_outcomes: scope.effect_outcomes,
                        },
                        *payload,
                    )?),
                    None => None,
                };
                let envelope = RuntimeMessageEnvelope::new(*message, prepared_payload);
                match port {
                    Some(port) => self.send_message_with_boundary(
                        pid,
                        envelope,
                        Some(step.pid),
                        BoundarySendContext {
                            step,
                            port_id: *port,
                        },
                    ),
                    None => self.send_message(pid, envelope, Some(step.pid)),
                }
            }
            ExecutableActionPlan::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                let branch = self.select_branch(BranchSelection {
                    step,
                    scope: RuntimeBranchScope::Action,
                    branch_path,
                    condition: *condition,
                    executable_templates: scope.executable_templates,
                    local_process_refs,
                    loop_elements: scope.loop_elements,
                    effect_outcomes: scope.effect_outcomes,
                })?;
                let selected_actions = match branch {
                    ArtifactBranch::Then => then_actions,
                    ArtifactBranch::Else => else_actions,
                };
                for (action_index, action) in selected_actions.all_actions(scope.executable_actions)
                {
                    let selected_branch_path = branch_path.child(
                        RuntimeBranchPathSegment::branch_action(branch, action_index)?,
                    )?;
                    self.execute_action_plan(
                        local_process_refs,
                        step,
                        action,
                        selected_branch_path,
                        scope,
                    )?;
                }
                Ok(())
            }
            ExecutableActionPlan::ForEach {
                element,
                collection,
                max_items,
                body,
            } => {
                let collection = evaluate_runtime_template(
                    RuntimeTemplateContext {
                        program: self.program,
                        templates: scope.executable_templates,
                        received_payload: step.payload.as_ref(),
                        step,
                        process_refs: local_process_refs,
                        loop_elements: scope.loop_elements,
                        effect_outcomes: scope.effect_outcomes,
                    },
                    *collection,
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
                    scope,
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
                    let active_scope = RuntimeActionScope {
                        executable_actions: scope.executable_actions,
                        executable_templates: scope.executable_templates,
                        loop_elements: &active,
                        effect_outcomes: scope.effect_outcomes,
                    };
                    for (action_index, action) in body.all_actions(scope.executable_actions) {
                        let body_action_path = branch_path
                            .child(RuntimeBranchPathSegment::loop_body_action(action_index)?)?;
                        self.execute_action_plan(
                            local_process_refs,
                            step,
                            action,
                            body_action_path,
                            active_scope,
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
        template: ExecutableValueTemplateRef,
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
        template: ExecutableValueTemplateRef,
        step: &ActiveStep,
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<RuntimePayload> {
        let process_refs = LocalProcessRefs::empty();
        evaluate_runtime_template(
            RuntimeTemplateContext {
                program: self.program,
                templates: self.executable.templates(),
                received_payload: step.payload.as_ref(),
                step,
                process_refs: &process_refs,
                loop_elements: &[],
                effect_outcomes,
            },
            template,
        )
    }

    fn resolve_next_state(
        &mut self,
        process_index: usize,
        step: &ActiveStep,
        next_state: &ExecutableNextState,
        branch_path: RuntimeBranchPath,
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<StateId> {
        match next_state {
            ExecutableNextState::Current => Ok(self.processes[process_index].state),
            ExecutableNextState::Value(state) => Ok(*state),
            ExecutableNextState::Template(template) => {
                self.resolve_template_state(step, *template, effect_outcomes)
            }
            ExecutableNextState::IfElse {
                condition,
                then_state,
                else_state,
            } => {
                let empty_process_refs = LocalProcessRefs::empty();
                let branch = self.select_branch(BranchSelection {
                    step,
                    scope: RuntimeBranchScope::NextState,
                    branch_path,
                    condition: *condition,
                    executable_templates: self.executable.templates(),
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
                self.processes[process_index].stop(RuntimeStopReason::Normal);
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
                self.processes[process_index].fail();
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

#[cfg(test)]
mod tests;
