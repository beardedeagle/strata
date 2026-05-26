use mantle_artifact::{ArtifactBranch, ArtifactValue, Error, LoopElementId, Result, TypeId};

use super::model::ActiveStep;
use super::process_refs::LocalProcessRefs;
use super::templates::evaluate_runtime_template;
use super::{BranchSelection, RuntimeEffectOutcome, RuntimeLoopElement, RuntimeRun};
use crate::event::{RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeLoopContext};
use crate::host::RuntimeHost;
use crate::program::{LoadedAction, LoadedLoopElement, LoadedValueTemplate, RuntimePayload};

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
    pub(super) fn ensure_loop_iteration_budget(&self, item_count: usize) -> Result<()> {
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

    pub(super) fn evaluate_bool_condition(
        &self,
        step: &ActiveStep,
        condition: &LoadedValueTemplate,
        local_process_refs: &LocalProcessRefs,
        loop_elements: &[RuntimeLoopElement],
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<(ArtifactBranch, RuntimePayload)> {
        let condition_value = evaluate_runtime_template(
            self.program,
            condition,
            step.payload.as_ref(),
            step,
            local_process_refs,
            loop_elements,
            effect_outcomes,
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

    pub(super) fn select_branch(
        &mut self,
        selection: BranchSelection<'_>,
    ) -> Result<ArtifactBranch> {
        let (branch, condition_value) = self.evaluate_bool_condition(
            selection.step,
            selection.condition,
            selection.local_process_refs,
            selection.loop_elements,
            selection.effect_outcomes,
        )?;
        self.record_branch_selected(
            selection.step,
            selection.scope,
            selection.branch_path,
            selection.loop_elements,
            branch,
            &condition_value,
        )?;
        Ok(branch)
    }

    fn record_branch_selected(
        &mut self,
        step: &ActiveStep,
        scope: RuntimeBranchScope,
        branch_path: RuntimeBranchPath,
        loop_elements: &[RuntimeLoopElement],
        branch: ArtifactBranch,
        condition: &RuntimePayload,
    ) -> Result<()> {
        let loop_context = loop_elements.last();
        self.record_event(RuntimeEvent::BranchSelected {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            message_id: step.message,
            message: step.message_label.clone(),
            branch,
            scope,
            branch_path,
            loop_context: loop_context.map(|element| RuntimeLoopContext {
                element_id: element.id,
                index: element.index,
            }),
            condition_type_id: condition.ty,
            condition: condition.label().to_string(),
        })
    }

    pub(super) fn consume_loop_iteration(&mut self) -> Result<()> {
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

    pub(super) fn record_loop_started(
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

    pub(super) fn record_loop_iteration(
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

    pub(super) fn record_loop_completed(
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

    pub(super) fn preflight_loop_body(
        &self,
        local_process_refs: &LocalProcessRefs,
        step: &ActiveStep,
        element: &LoadedLoopElement,
        body: &[LoadedAction],
        loop_payloads: &[RuntimePayload],
        effect_outcomes: &[RuntimeEffectOutcome],
    ) -> Result<()> {
        if loop_payloads.is_empty() {
            return Ok(());
        }

        let mut queued_mailbox_messages = None;
        for (index, payload) in loop_payloads.iter().enumerate() {
            let active = [RuntimeLoopElement {
                id: element.id,
                index,
                payload: payload.clone(),
            }];
            for action in body {
                self.preflight_loop_action(
                    local_process_refs,
                    step,
                    action,
                    &active,
                    effect_outcomes,
                    &mut queued_mailbox_messages,
                )?;
            }
        }
        Ok(())
    }

    fn preflight_loop_action(
        &self,
        local_process_refs: &LocalProcessRefs,
        step: &ActiveStep,
        action: &LoadedAction,
        loop_elements: &[RuntimeLoopElement],
        effect_outcomes: &[RuntimeEffectOutcome],
        queued_mailbox_messages: &mut Option<Vec<usize>>,
    ) -> Result<()> {
        match action {
            LoadedAction::Emit { .. } => Ok(()),
            LoadedAction::Spawn { .. } | LoadedAction::SpawnOutcome { .. } => {
                Err(Error::new(format!(
                    "process {} for loop body cannot bind process references or spawn outcomes",
                    step.process_name
                )))
            }
            LoadedAction::SendOutcome { .. } => Err(Error::new(format!(
                "process {} for loop body cannot bind send outcomes",
                step.process_name
            ))),
            LoadedAction::Send {
                target,
                message,
                payload,
            } => {
                let pid = self.resolve_send_target(local_process_refs, step, target)?;
                let queued_mailbox_messages = queued_mailbox_messages
                    .get_or_insert_with(|| vec![0usize; self.processes.len()]);
                let target_process_index = self
                    .preflight_delivery_target_with_queued_messages(pid, queued_mailbox_messages)?;
                let target_process_id = self.processes[target_process_index].process_id;
                self.program
                    .message_payload_type(target_process_id, *message)?;
                if let Some(payload) = payload {
                    evaluate_runtime_template(
                        self.program,
                        payload,
                        step.payload.as_ref(),
                        step,
                        local_process_refs,
                        loop_elements,
                        effect_outcomes,
                    )?;
                }
                let queued_messages = queued_mailbox_messages
                    .get_mut(target_process_index)
                    .ok_or_else(|| {
                        Error::new("runtime loop preflight mailbox accounting is inconsistent")
                    })?;
                *queued_messages = queued_messages
                    .checked_add(1)
                    .ok_or_else(|| Error::new("runtime mailbox preflight count overflowed"))?;
                Ok(())
            }
            LoadedAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                let (branch, _) = self.evaluate_bool_condition(
                    step,
                    condition,
                    local_process_refs,
                    loop_elements,
                    effect_outcomes,
                )?;
                let selected_actions = match branch {
                    ArtifactBranch::Then => then_actions,
                    ArtifactBranch::Else => else_actions,
                };
                for action in selected_actions {
                    self.preflight_loop_action(
                        local_process_refs,
                        step,
                        action,
                        loop_elements,
                        effect_outcomes,
                        queued_mailbox_messages,
                    )?;
                }
                Ok(())
            }
            LoadedAction::ForEach { .. } => Err(Error::new(format!(
                "process {} nested for loops are not supported in this artifact slice",
                step.process_name
            ))),
        }
    }
}
