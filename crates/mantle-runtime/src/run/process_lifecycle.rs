use mantle_artifact::{Error, MAX_PROCESS_COUNT, ProcessId, Result};

use super::RuntimeRun;
use super::model::{ProcessInstance, RuntimeSupervisorRef, RuntimeSupervisorState};
use crate::event::{RuntimeEvent, RuntimeProcessId};
use crate::host::RuntimeHost;
use crate::report::{ProcessStatus, SpawnReport};

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    pub(super) fn spawn_process(
        &mut self,
        process_id: ProcessId,
        spawned_by_pid: Option<RuntimeProcessId>,
    ) -> Result<RuntimeProcessId> {
        self.spawn_process_with_parent(process_id, spawned_by_pid, None)
    }

    pub(super) fn spawn_process_with_parent(
        &mut self,
        process_id: ProcessId,
        spawned_by_pid: Option<RuntimeProcessId>,
        supervisor_parent: Option<(RuntimeProcessId, RuntimeSupervisorRef)>,
    ) -> Result<RuntimeProcessId> {
        self.ensure_spawn_capacity(process_id)?;

        let definition = self.program.process(process_id)?;
        let pid = self.next_pid;
        self.next_pid = self.next_pid.checked_next()?;
        let process = ProcessInstance {
            pid,
            process_id,
            state: definition.init_state,
            status: ProcessStatus::Running,
            supervisor_parent,
            supervisors: definition
                .supervisor_plans
                .iter()
                .map(RuntimeSupervisorState::from_plan)
                .collect(),
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
        self.start_supervisor_children(pid, process_id)?;
        Ok(pid)
    }

    pub(super) fn spawn_capacity_available(&self, process_id: ProcessId) -> Result<bool> {
        let process = self.program.process(process_id)?;
        let required = if process.supervisor_plans.is_empty() {
            1
        } else {
            self.supervised_subtree_size(process_id)?
        };
        let Some(total) = self.processes.len().checked_add(required) else {
            return Err(Error::new("runtime process instance count overflowed"));
        };
        Ok(total <= self.max_runtime_processes)
    }

    fn ensure_spawn_capacity(&self, process_id: ProcessId) -> Result<()> {
        if !self.spawn_capacity_available(process_id)? {
            return Err(Error::new(format!(
                "runtime process instance limit exceeded at {} process instance(s)",
                self.max_runtime_processes
            )));
        }
        Ok(())
    }

    fn supervised_subtree_size(&self, process_id: ProcessId) -> Result<usize> {
        let mut active = [false; MAX_PROCESS_COUNT];
        self.supervised_subtree_size_inner(process_id, &mut active)
    }

    fn supervised_subtree_size_inner(
        &self,
        process_id: ProcessId,
        active: &mut [bool; MAX_PROCESS_COUNT],
    ) -> Result<usize> {
        let process = self.program.process(process_id)?;
        let index = process_id.index();
        let Some(is_active) = active.get_mut(index) else {
            return Err(Error::new(format!(
                "process id {} cannot be tracked for supervised subtree sizing",
                process_id.as_u32()
            )));
        };
        if *is_active {
            return Err(Error::new(format!(
                "loaded local supervisor graph contains cycle at process {}",
                process.debug_name
            )));
        }
        *is_active = true;

        let mut total = 1usize;
        for plan in &process.supervisor_plans {
            for child in &plan.children {
                let child_total = self.supervised_subtree_size_inner(child.target, active)?;
                total = total.checked_add(child_total).ok_or_else(|| {
                    Error::new("supervised process subtree size overflowed runtime limits")
                })?;
            }
        }

        active[index] = false;
        Ok(total)
    }
}
