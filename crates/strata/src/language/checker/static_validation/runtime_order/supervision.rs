use std::collections::{BTreeMap, VecDeque};

use mantle_artifact::MAX_PROCESS_COUNT;

use super::{
    StaticProcessId, StaticProcessInstance, StaticProcessStatus, ensure_static_process_capacity,
};
use crate::language::checked::{
    CheckedProcess, CheckedProcessId, CheckedSupervisorChildId, CheckedSupervisorId,
};
use crate::language::checker::static_validation::process_refs::process_by_id;
use crate::language::diagnostic::{Error, Result};

pub(super) type StaticSupervisorChildKey = (
    StaticProcessId,
    CheckedSupervisorId,
    CheckedSupervisorChildId,
);
pub(super) type StaticSupervisorKey = (StaticProcessId, CheckedSupervisorId);

pub(super) fn static_spawn_capacity_available(
    processes: &[CheckedProcess],
    instance_count: usize,
    process_id: CheckedProcessId,
) -> Result<bool> {
    let required = supervised_subtree_size(processes, process_id)?;
    let Some(total) = instance_count.checked_add(required) else {
        return Err(Error::new(
            "static runtime process instance count overflowed",
        ));
    };
    Ok(total <= crate::language::STATIC_RUNTIME_PROCESS_LIMIT)
}

pub(super) fn spawn_static_instance(
    processes: &[CheckedProcess],
    instances: &mut Vec<StaticProcessInstance>,
    next_pid: &mut StaticProcessId,
    supervisor_children: &mut BTreeMap<StaticSupervisorChildKey, StaticProcessId>,
    process_id: CheckedProcessId,
    pid: StaticProcessId,
    supervisor_parent: Option<StaticSupervisorChildKey>,
) -> Result<()> {
    let process = process_by_id(processes, process_id)?;
    ensure_static_process_capacity(instances.len())?;
    instances.push(StaticProcessInstance {
        pid,
        process_id,
        state: process.init_state(),
        status: StaticProcessStatus::Running,
        supervisor_parent,
        mailbox: VecDeque::new(),
    });
    for (supervisor_index, plan) in process.supervisor_plans().iter().enumerate() {
        let supervisor_id = CheckedSupervisorId::from_index(supervisor_index)?;
        for (child_index, child) in plan.children().iter().enumerate() {
            let child_id = CheckedSupervisorChildId::from_index(child_index)?;
            let child_pid = *next_pid;
            *next_pid = next_pid.checked_next()?;
            if supervisor_children
                .insert((pid, supervisor_id, child_id), child_pid)
                .is_some()
            {
                return Err(Error::new(format!(
                    "static runtime supervisor child id {} was started twice",
                    child_id.as_u32()
                )));
            }
            spawn_static_instance(
                processes,
                instances,
                next_pid,
                supervisor_children,
                child.target(),
                child_pid,
                Some((pid, supervisor_id, child_id)),
            )?;
        }
    }
    Ok(())
}

fn supervised_subtree_size(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
) -> Result<usize> {
    let mut active = [false; MAX_PROCESS_COUNT];
    supervised_subtree_size_inner(processes, process_id, &mut active)
}

fn supervised_subtree_size_inner(
    processes: &[CheckedProcess],
    process_id: CheckedProcessId,
    active: &mut [bool; MAX_PROCESS_COUNT],
) -> Result<usize> {
    let process = process_by_id(processes, process_id)?;
    let index = process_id.index();
    let Some(is_active) = active.get_mut(index) else {
        return Err(Error::new(format!(
            "process id {} cannot be tracked for supervised subtree sizing",
            process_id.as_u32()
        )));
    };
    if *is_active {
        return Err(Error::new(format!(
            "static local supervisor graph contains cycle at process {}",
            process.debug_name()
        )));
    }
    *is_active = true;

    let mut total = 1usize;
    for plan in process.supervisor_plans() {
        for child in plan.children() {
            let child_total = supervised_subtree_size_inner(processes, child.target(), active)?;
            total = total.checked_add(child_total).ok_or_else(|| {
                Error::new("supervised process subtree size overflowed static runtime limits")
            })?;
        }
    }

    active[index] = false;
    Ok(total)
}
