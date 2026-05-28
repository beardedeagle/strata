use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR, MAX_SUPERVISORS_PER_PROCESS};

use super::super::ast::{Identifier, Module, Process, SupervisorChildMode, SupervisorStrategy};
use super::super::checked::{
    CheckedProcessId, CheckedSupervisorChild, CheckedSupervisorChildId, CheckedSupervisorChildMode,
    CheckedSupervisorId, CheckedSupervisorPlan, CheckedSupervisorRestartIntensity,
    CheckedSupervisorStrategy,
};
use super::super::diagnostic::{Error, Result};
use super::authority::SpawnSiteAllocator;
use super::symbols::SemanticIndex;
use super::validate_count;

#[derive(Debug, Clone, Copy)]
pub(in crate::language::checker) struct SupervisorChildBinding {
    pub(in crate::language::checker) supervisor: CheckedSupervisorId,
    pub(in crate::language::checker) child: CheckedSupervisorChildId,
    pub(in crate::language::checker) target: CheckedProcessId,
}

pub(in crate::language::checker) fn check_supervisors(
    _module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    spawn_sites: &mut SpawnSiteAllocator,
) -> Result<(
    Vec<CheckedSupervisorPlan>,
    BTreeMap<Identifier, SupervisorChildBinding>,
)> {
    if process.supervisors.is_empty() {
        return Ok((Vec::new(), BTreeMap::new()));
    }

    validate_count(
        &format!("process {} supervisor_count", process.name),
        process.supervisors.len(),
        0,
        MAX_SUPERVISORS_PER_PROCESS,
    )?;

    let mut plans = Vec::with_capacity(process.supervisors.len());
    let mut child_index = BTreeMap::new();
    let mut child_names = BTreeSet::new();

    for (supervisor_index, supervisor) in process.supervisors.iter().enumerate() {
        let supervisor_id = CheckedSupervisorId::from_index(supervisor_index)?;
        validate_count(
            &format!(
                "process {} supervisor {} child_count",
                process.name,
                supervisor_id.as_u32()
            ),
            supervisor.children.len(),
            1,
            MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR,
        )?;

        let intensity =
            CheckedSupervisorRestartIntensity::new(supervisor.max_restarts, supervisor.within_ms)?;
        let strategy = checked_supervisor_strategy(supervisor.strategy);
        let mut children = Vec::with_capacity(supervisor.children.len());

        for (child_index_value, child) in supervisor.children.iter().enumerate() {
            if !child_names.insert(child.name.as_str()) {
                return Err(Error::new(format!(
                    "process {} declares duplicate supervisor child {}",
                    process.name, child.name
                )));
            }
            if semantic_index.process_id(&child.name).is_ok() {
                return Err(Error::new(format!(
                    "process {} supervisor child {} conflicts with a process declaration",
                    process.name, child.name
                )));
            }
            if semantic_index.identifier_conflicts_with_declared_value(&child.name) {
                return Err(Error::new(format!(
                    "process {} supervisor child {} conflicts with a declared type or value constructor",
                    process.name, child.name
                )));
            }

            let declared_target = semantic_index.process_id(&child.process)?;
            let spawn_target = semantic_index.process_id(&child.spawn_target)?;
            if declared_target != spawn_target {
                return Err(Error::new(format!(
                    "process {} supervisor child {} declares target {} but spawns {}",
                    process.name, child.name, child.process, child.spawn_target
                )));
            }
            if declared_target == process_id {
                return Err(Error::new(format!(
                    "process {} supervisor child {} cannot target its owning process",
                    process.name, child.name
                )));
            }
            if declared_target == entry_process {
                return Err(Error::new(format!(
                    "process {} supervisor child {} cannot target the entry process",
                    process.name, child.name
                )));
            }

            let child_id = CheckedSupervisorChildId::from_index(child_index_value)?;
            let spawn_site = spawn_sites.push_lexical_supervisor_child(
                declared_target,
                supervisor_id,
                child_id,
            )?;
            children.push(CheckedSupervisorChild::new(
                child.name.clone(),
                declared_target,
                checked_child_mode(child.mode),
                spawn_site,
            ));
            child_index.insert(
                child.name.clone(),
                SupervisorChildBinding {
                    supervisor: supervisor_id,
                    child: child_id,
                    target: declared_target,
                },
            );
        }

        plans.push(CheckedSupervisorPlan::new(strategy, intensity, children)?);
    }

    Ok((plans, child_index))
}

fn checked_supervisor_strategy(strategy: SupervisorStrategy) -> CheckedSupervisorStrategy {
    match strategy {
        SupervisorStrategy::OneForOne => CheckedSupervisorStrategy::OneForOne,
    }
}

fn checked_child_mode(mode: SupervisorChildMode) -> CheckedSupervisorChildMode {
    match mode {
        SupervisorChildMode::Permanent => CheckedSupervisorChildMode::Permanent,
        SupervisorChildMode::Transient => CheckedSupervisorChildMode::Transient,
        SupervisorChildMode::Temporary => CheckedSupervisorChildMode::Temporary,
    }
}
