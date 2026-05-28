use mantle_artifact::{Error, ProcessId, Result, SupervisorChildId, SupervisorId};

use super::RuntimeRun;
use super::model::{RuntimeSupervisorChildState, RuntimeSupervisorRef, RuntimeSupervisorState};
use crate::event::{
    RuntimeEvent, RuntimeFailureReason, RuntimeProcessId, RuntimeSpawnKind, RuntimeStopReason,
    RuntimeSupervisorExitReason, RuntimeSupervisorRestartDecision,
};
use crate::host::RuntimeHost;
use crate::program::LoadedSupervisorChildMode;
use crate::report::ProcessStatus;

const DEFAULT_RESTART_THROTTLE_MS: u64 = 1;

struct RestartDecisionRecord<'a> {
    supervisor_pid: RuntimeProcessId,
    supervisor_process_id: ProcessId,
    supervisor_process: &'a str,
    child_ref: RuntimeSupervisorRef,
    child: &'a str,
    child_pid: RuntimeProcessId,
    child_process_id: ProcessId,
    child_process: &'a str,
    reason: RuntimeSupervisorExitReason,
    decision: RuntimeSupervisorRestartDecision,
    restart_time_ms: Option<u64>,
    restart_window_count: usize,
    restart_window_limit: u32,
    restart_window_ms: u64,
    new_child_pid: Option<RuntimeProcessId>,
}

struct RestartFailureRecord<'a> {
    supervisor_index: usize,
    failure_reason: RuntimeFailureReason,
    decision: RestartDecisionRecord<'a>,
}

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
    pub(super) fn start_supervisor_children(
        &mut self,
        supervisor_pid: RuntimeProcessId,
        supervisor_process_id: ProcessId,
    ) -> Result<()> {
        let supervisor_index = self.process_index_for_pid(supervisor_pid)?;
        let (supervisor_process_name, supervisor_count) = {
            let supervisor_process = self.program.process(supervisor_process_id)?;
            if supervisor_process.supervisor_plans.is_empty() {
                return Ok(());
            }
            (
                supervisor_process.debug_name.clone(),
                supervisor_process.supervisor_plans.len(),
            )
        };

        for supervisor_index_value in 0..supervisor_count {
            let supervisor_id = SupervisorId::from_index(supervisor_index_value)?;
            let child_count = self
                .program
                .process(supervisor_process_id)?
                .supervisor_plans[supervisor_index_value]
                .children
                .len();
            for child_index_value in 0..child_count {
                let child_id = SupervisorChildId::from_index(child_index_value)?;
                let child_ref = RuntimeSupervisorRef {
                    supervisor: supervisor_id,
                    child: child_id,
                };
                let (child_target, child_name, spawn_site) = {
                    let child = &self
                        .program
                        .process(supervisor_process_id)?
                        .supervisor_plans[supervisor_index_value]
                        .children[child_index_value];
                    (child.target, child.debug_name.clone(), child.spawn_site)
                };
                let child_process = self.program.process_label(child_target)?.to_string();
                let child_pid = self.spawn_process_with_parent(
                    child_target,
                    Some(supervisor_pid),
                    Some((supervisor_pid, child_ref)),
                )?;
                self.supervisor_child_slot_mut(supervisor_index, child_ref)?
                    .current_pid = Some(child_pid);
                self.record_event(RuntimeEvent::SupervisorChildStarted {
                    supervisor_pid,
                    supervisor_process_id,
                    supervisor_process: supervisor_process_name.clone(),
                    supervisor_id,
                    child_id,
                    child: child_name,
                    child_pid,
                    child_process_id: child_target,
                    child_process,
                    spawn_site_id: spawn_site,
                    spawn_kind: RuntimeSpawnKind::LexicalSupervisorChild,
                })?;
            }
        }
        Ok(())
    }

    pub(super) fn stop_supervised_children(
        &mut self,
        supervisor_pid: RuntimeProcessId,
        reason: RuntimeStopReason,
    ) -> Result<()> {
        let supervisor_index = self.process_index_for_pid(supervisor_pid)?;
        let child_pids = self.processes[supervisor_index]
            .supervisors
            .iter()
            .rev()
            .flat_map(|supervisor| supervisor.children.iter().rev())
            .filter_map(|child| child.current_pid)
            .collect::<Vec<_>>();
        for child_pid in child_pids {
            self.stop_process_tree(child_pid, reason)?;
        }
        Ok(())
    }

    pub(super) fn handle_supervised_exit(
        &mut self,
        process_index: usize,
        child_pid: RuntimeProcessId,
        child_process_id: ProcessId,
        child_process_name: &str,
        reason: RuntimeSupervisorExitReason,
    ) -> Result<()> {
        let Some((supervisor_pid, child_ref)) = self.processes[process_index].supervisor_parent
        else {
            return match reason {
                RuntimeSupervisorExitReason::Normal => Ok(()),
                RuntimeSupervisorExitReason::Panic => {
                    Err(Error::new("unsupervised process failed"))
                }
            };
        };

        let supervisor_index = self.process_index_for_pid(supervisor_pid)?;
        let supervisor_process_id = self.processes[supervisor_index].process_id;
        let (supervisor_process_name, intensity, child_target, child_mode, child_name) = {
            let supervisor_process = self.program.process(supervisor_process_id)?;
            let plan = supervisor_process
                .supervisor_plans
                .get(child_ref.supervisor.index())
                .ok_or_else(|| Error::new("supervisor plan is not loaded"))?;
            let child_plan = plan
                .children
                .get(child_ref.child.index())
                .ok_or_else(|| Error::new("supervisor child is not loaded"))?;
            (
                supervisor_process.debug_name.clone(),
                plan.intensity,
                child_plan.target,
                child_plan.mode,
                child_plan.debug_name.clone(),
            )
        };
        let should_restart = should_restart_child(child_mode, reason);
        self.clear_supervisor_child_slot(supervisor_index, child_ref, child_pid)?;

        if !should_restart {
            self.record_supervisor_restart_decision(RestartDecisionRecord {
                supervisor_pid,
                supervisor_process_id,
                supervisor_process: &supervisor_process_name,
                child_ref,
                child: &child_name,
                child_pid,
                child_process_id,
                child_process: child_process_name,
                reason,
                decision: RuntimeSupervisorRestartDecision::NotRestarted,
                restart_time_ms: None,
                restart_window_count: 0,
                restart_window_limit: intensity.max_restarts,
                restart_window_ms: intensity.within_ms,
                new_child_pid: None,
            })?;
            return Ok(());
        }

        let now_ms = self.host.monotonic_ms()?;
        let restart_limit = usize::try_from(intensity.max_restarts)
            .map_err(|_| Error::new("supervisor restart intensity limit does not fit usize"))?;
        let restart_window_count = {
            let supervisor = self.supervisor_slot_mut(supervisor_index, child_ref.supervisor)?;
            while supervisor
                .restart_window
                .front()
                .is_some_and(|started| now_ms.saturating_sub(*started) >= intensity.within_ms)
            {
                supervisor.restart_window.pop_front();
            }
            supervisor.restart_window.len()
        };
        if restart_window_count >= restart_limit {
            return self.deny_supervisor_restart(RestartFailureRecord {
                supervisor_index,
                failure_reason: RuntimeFailureReason::SupervisorRestartIntensityExceeded,
                decision: RestartDecisionRecord {
                    supervisor_pid,
                    supervisor_process_id,
                    supervisor_process: &supervisor_process_name,
                    child_ref,
                    child: &child_name,
                    child_pid,
                    child_process_id,
                    child_process: child_process_name,
                    reason,
                    decision: RuntimeSupervisorRestartDecision::Denied,
                    restart_time_ms: Some(now_ms),
                    restart_window_count,
                    restart_window_limit: intensity.max_restarts,
                    restart_window_ms: intensity.within_ms,
                    new_child_pid: None,
                },
            });
        }
        let restart_throttled = {
            let supervisor = self.supervisor_slot_mut(supervisor_index, child_ref.supervisor)?;
            supervisor.restart_window.back().is_some_and(|started| {
                now_ms.saturating_sub(*started) < DEFAULT_RESTART_THROTTLE_MS
            })
        };
        if restart_throttled {
            return self.deny_supervisor_restart(RestartFailureRecord {
                supervisor_index,
                failure_reason: RuntimeFailureReason::SupervisorRestartThrottled,
                decision: RestartDecisionRecord {
                    supervisor_pid,
                    supervisor_process_id,
                    supervisor_process: &supervisor_process_name,
                    child_ref,
                    child: &child_name,
                    child_pid,
                    child_process_id,
                    child_process: child_process_name,
                    reason,
                    decision: RuntimeSupervisorRestartDecision::Denied,
                    restart_time_ms: Some(now_ms),
                    restart_window_count,
                    restart_window_limit: intensity.max_restarts,
                    restart_window_ms: intensity.within_ms,
                    new_child_pid: None,
                },
            });
        }
        if !self.spawn_capacity_available(child_target)? {
            return self.deny_supervisor_restart(RestartFailureRecord {
                supervisor_index,
                failure_reason: RuntimeFailureReason::SupervisorRestartCapacityExceeded,
                decision: RestartDecisionRecord {
                    supervisor_pid,
                    supervisor_process_id,
                    supervisor_process: &supervisor_process_name,
                    child_ref,
                    child: &child_name,
                    child_pid,
                    child_process_id,
                    child_process: child_process_name,
                    reason,
                    decision: RuntimeSupervisorRestartDecision::Denied,
                    restart_time_ms: Some(now_ms),
                    restart_window_count,
                    restart_window_limit: intensity.max_restarts,
                    restart_window_ms: intensity.within_ms,
                    new_child_pid: None,
                },
            });
        }
        let restart_window_count = {
            let supervisor = self.supervisor_slot_mut(supervisor_index, child_ref.supervisor)?;
            supervisor.restart_window.push_back(now_ms);
            supervisor.restart_window.len()
        };

        let new_pid = self.spawn_process_with_parent(
            child_target,
            Some(supervisor_pid),
            Some((supervisor_pid, child_ref)),
        )?;
        self.supervisor_child_slot_mut(supervisor_index, child_ref)?
            .current_pid = Some(new_pid);
        self.record_supervisor_restart_decision(RestartDecisionRecord {
            supervisor_pid,
            supervisor_process_id,
            supervisor_process: &supervisor_process_name,
            child_ref,
            child: &child_name,
            child_pid,
            child_process_id,
            child_process: child_process_name,
            reason,
            decision: RuntimeSupervisorRestartDecision::Restarted,
            restart_time_ms: Some(now_ms),
            restart_window_count,
            restart_window_limit: intensity.max_restarts,
            restart_window_ms: intensity.within_ms,
            new_child_pid: Some(new_pid),
        })
    }

    fn deny_supervisor_restart(&mut self, failure: RestartFailureRecord<'_>) -> Result<()> {
        let decision = failure.decision;
        let supervisor_pid = decision.supervisor_pid;
        let supervisor_process_id = decision.supervisor_process_id;
        let supervisor_process = decision.supervisor_process;
        let child = decision.child;
        self.record_supervisor_restart_decision(decision)?;
        let failure_text = match failure.failure_reason {
            RuntimeFailureReason::SupervisorRestartCapacityExceeded => "restart capacity exceeded",
            RuntimeFailureReason::SupervisorRestartIntensityExceeded => {
                "restart intensity exceeded"
            }
            RuntimeFailureReason::SupervisorRestartThrottled => "restart throttled",
            RuntimeFailureReason::Panic => "failed",
        };
        self.fail_supervisor_scope(
            failure.supervisor_index,
            supervisor_pid,
            supervisor_process_id,
            supervisor_process,
            failure.failure_reason,
            format!("supervisor {supervisor_process} {failure_text} for child {child}"),
        )
    }

    fn fail_supervisor_scope(
        &mut self,
        supervisor_index: usize,
        supervisor_pid: RuntimeProcessId,
        supervisor_process_id: ProcessId,
        supervisor_process_name: &str,
        reason: RuntimeFailureReason,
        error: String,
    ) -> Result<()> {
        if self.processes[supervisor_index].status == ProcessStatus::Running {
            self.stop_supervised_children(supervisor_pid, RuntimeStopReason::SupervisorFailure)?;
            let state_id = self.processes[supervisor_index].state;
            let state = self
                .program
                .state_label(supervisor_process_id, state_id)?
                .to_string();
            self.record_event(RuntimeEvent::ProcessFailed {
                pid: supervisor_pid,
                process_id: supervisor_process_id,
                process: supervisor_process_name.to_string(),
                state_id,
                state,
                reason,
            })?;
            self.processes[supervisor_index].status = ProcessStatus::Failed;
        }
        self.handle_supervised_exit(
            supervisor_index,
            supervisor_pid,
            supervisor_process_id,
            supervisor_process_name,
            RuntimeSupervisorExitReason::Panic,
        )
        .map_err(|err| Error::new(format!("{error}: {err}")))
    }

    fn stop_process_tree(
        &mut self,
        pid: RuntimeProcessId,
        reason: RuntimeStopReason,
    ) -> Result<()> {
        let index = self.process_index_for_pid(pid)?;
        if self.processes[index].status != ProcessStatus::Running {
            return Ok(());
        }
        self.stop_supervised_children(pid, reason)?;
        let process_id = self.processes[index].process_id;
        let process = self.program.process_label(process_id)?.to_string();
        self.record_event(RuntimeEvent::ProcessStopped {
            pid,
            process_id,
            process,
            reason,
        })?;
        self.processes[index].status = ProcessStatus::Stopped;
        Ok(())
    }

    fn clear_supervisor_child_slot(
        &mut self,
        supervisor_index: usize,
        child_ref: RuntimeSupervisorRef,
        child_pid: RuntimeProcessId,
    ) -> Result<()> {
        let slot = self.supervisor_child_slot_mut(supervisor_index, child_ref)?;
        match slot.current_pid {
            Some(current_pid) if current_pid == child_pid => {
                slot.current_pid = None;
                Ok(())
            }
            Some(current_pid) => Err(Error::new(format!(
                "runtime supervisor child slot for pid {} points at pid {}",
                child_pid.as_u64(),
                current_pid.as_u64()
            ))),
            None => Ok(()),
        }
    }

    fn supervisor_child_slot_mut(
        &mut self,
        supervisor_index: usize,
        child_ref: RuntimeSupervisorRef,
    ) -> Result<&mut RuntimeSupervisorChildState> {
        self.supervisor_slot_mut(supervisor_index, child_ref.supervisor)?
            .children
            .get_mut(child_ref.child.index())
            .ok_or_else(|| Error::new("supervisor child slot is not loaded"))
    }

    fn supervisor_slot_mut(
        &mut self,
        supervisor_index: usize,
        supervisor: SupervisorId,
    ) -> Result<&mut RuntimeSupervisorState> {
        self.processes
            .get_mut(supervisor_index)
            .and_then(|process| process.supervisors.get_mut(supervisor.index()))
            .ok_or_else(|| Error::new("supervisor slot is not loaded"))
    }

    fn record_supervisor_restart_decision(
        &mut self,
        record: RestartDecisionRecord<'_>,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::SupervisorRestartDecision {
            supervisor_pid: record.supervisor_pid,
            supervisor_process_id: record.supervisor_process_id,
            supervisor_process: record.supervisor_process.to_string(),
            supervisor_id: record.child_ref.supervisor,
            child_id: record.child_ref.child,
            child: record.child.to_string(),
            child_pid: record.child_pid,
            child_process_id: record.child_process_id,
            child_process: record.child_process.to_string(),
            reason: record.reason,
            decision: record.decision,
            restart_time_ms: record.restart_time_ms,
            restart_window_count: record.restart_window_count,
            restart_window_limit: record.restart_window_limit,
            restart_window_ms: record.restart_window_ms,
            new_child_pid: record.new_child_pid,
        })
    }
}

const fn should_restart_child(
    mode: LoadedSupervisorChildMode,
    reason: RuntimeSupervisorExitReason,
) -> bool {
    match (mode, reason) {
        (LoadedSupervisorChildMode::Permanent, _) => true,
        (LoadedSupervisorChildMode::Transient, RuntimeSupervisorExitReason::Panic) => true,
        (LoadedSupervisorChildMode::Transient, RuntimeSupervisorExitReason::Normal)
        | (LoadedSupervisorChildMode::Temporary, _) => false,
    }
}
