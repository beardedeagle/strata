use std::collections::BTreeSet;

use super::{validate_count, validate_ident_field};
use crate::{
    ArtifactProcess, ArtifactSpawnKind, Error, MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR,
    MantleArtifact, ProcessId, Result, SupervisorChildId, SupervisorId,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Visiting,
    Done,
}

impl MantleArtifact {
    pub(in crate::artifact) fn validate_supervision_graph_acyclic(&self) -> Result<()> {
        if !self
            .processes
            .iter()
            .any(|process| !process.supervisor_plans.is_empty())
        {
            return Ok(());
        }

        let mut states = vec![VisitState::Unseen; self.processes.len()];
        let mut stack = Vec::with_capacity(self.processes.len());
        for index in 0..self.processes.len() {
            self.visit_supervision_process(index, &mut states, &mut stack)?;
        }
        Ok(())
    }

    fn visit_supervision_process(
        &self,
        index: usize,
        states: &mut [VisitState],
        stack: &mut Vec<usize>,
    ) -> Result<()> {
        match states[index] {
            VisitState::Done => return Ok(()),
            VisitState::Visiting => return Err(self.supervision_cycle_error(stack, index)),
            VisitState::Unseen => {}
        }

        states[index] = VisitState::Visiting;
        stack.push(index);
        for target in self.processes[index]
            .supervisor_plans
            .iter()
            .flat_map(|plan| plan.children.iter().map(|child| child.target))
        {
            let target_index = target.index();
            if target_index >= self.processes.len() {
                return Err(Error::new(format!(
                    "process {} supervisor child targets undefined process id {}",
                    self.processes[index].debug_name,
                    target.as_u32()
                )));
            }
            if states[target_index] == VisitState::Visiting {
                return Err(self.supervision_cycle_error(stack, target_index));
            }
            self.visit_supervision_process(target_index, states, stack)?;
        }
        stack.pop();
        states[index] = VisitState::Done;
        Ok(())
    }

    fn supervision_cycle_error(&self, stack: &[usize], cycle_start: usize) -> Error {
        let Some(first_cycle_entry) = stack.iter().position(|index| *index == cycle_start) else {
            return Error::new("local supervisor graph cycle stack is inconsistent");
        };
        let mut path = stack[first_cycle_entry..]
            .iter()
            .map(|index| self.processes[*index].debug_name.clone())
            .collect::<Vec<_>>();
        path.push(self.processes[cycle_start].debug_name.clone());
        Error::new(format!(
            "local supervisor graph contains cycle {}",
            path.join(" -> ")
        ))
    }
}

impl ArtifactProcess {
    pub(super) fn validate_supervisors(
        &self,
        artifact: &MantleArtifact,
        process_id: ProcessId,
    ) -> Result<()> {
        if self.supervisor_plans.is_empty() {
            return Ok(());
        }

        let mut child_names = BTreeSet::new();
        for (supervisor_index, supervisor) in self.supervisor_plans.iter().enumerate() {
            validate_count(
                &format!(
                    "process {} supervisor {supervisor_index} child_count",
                    self.debug_name
                ),
                supervisor.children.len(),
                1,
                MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR,
            )?;
            if supervisor.intensity.max_restarts == 0 {
                return Err(Error::new(format!(
                    "process {} supervisor {supervisor_index} max_restarts must be greater than zero",
                    self.debug_name
                )));
            }
            if supervisor.intensity.within_ms == 0 {
                return Err(Error::new(format!(
                    "process {} supervisor {supervisor_index} within_ms must be greater than zero",
                    self.debug_name
                )));
            }
            self.validate_supervisor_children(
                artifact,
                process_id,
                supervisor_index,
                &mut child_names,
            )?;
        }
        Ok(())
    }

    fn validate_supervisor_children<'a>(
        &'a self,
        artifact: &MantleArtifact,
        process_id: ProcessId,
        supervisor_index: usize,
        child_names: &mut BTreeSet<&'a str>,
    ) -> Result<()> {
        let supervisor = self
            .supervisor_plans
            .get(supervisor_index)
            .ok_or_else(|| Error::new("supervisor plan is not loaded"))?;
        for (child_index, child) in supervisor.children.iter().enumerate() {
            validate_ident_field(
                &format!(
                    "process {} supervisor {supervisor_index} child {child_index} debug_name",
                    self.debug_name
                ),
                &child.debug_name,
            )?;
            if !child_names.insert(child.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "process {} duplicates supervisor child {}",
                    self.debug_name, child.debug_name
                )));
            }
            artifact
                .processes
                .get(child.target.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} supervisor child {} targets undefined process id {}",
                        self.debug_name,
                        child.debug_name,
                        child.target.as_u32()
                    ))
                })?;
            if child.target == process_id {
                return Err(Error::new(format!(
                    "process {} supervisor child {} targets its owning process",
                    self.debug_name, child.debug_name
                )));
            }
            if child.target == artifact.entry_process {
                return Err(Error::new(format!(
                    "process {} supervisor child {} targets entry process id {}",
                    self.debug_name,
                    child.debug_name,
                    child.target.as_u32()
                )));
            }
            self.validate_supervisor_child_spawn_site(supervisor_index, child_index)?;
        }
        Ok(())
    }

    fn validate_supervisor_child_spawn_site(
        &self,
        supervisor_index: usize,
        child_index: usize,
    ) -> Result<()> {
        let supervisor = self
            .supervisor_plans
            .get(supervisor_index)
            .ok_or_else(|| Error::new("supervisor plan is not loaded"))?;
        let child = supervisor
            .children
            .get(child_index)
            .ok_or_else(|| Error::new("supervisor child is not loaded"))?;
        let spawn_site = self
            .spawn_sites
            .get(child.spawn_site.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} supervisor child {} references undefined spawn site id {}",
                    self.debug_name,
                    child.debug_name,
                    child.spawn_site.as_u32()
                ))
            })?;
        if spawn_site.kind != ArtifactSpawnKind::LexicalSupervisorChild {
            return Err(Error::new(format!(
                "process {} supervisor child {} references non-lexical spawn site id {}",
                self.debug_name,
                child.debug_name,
                child.spawn_site.as_u32()
            )));
        }
        if spawn_site.target != child.target {
            return Err(Error::new(format!(
                "process {} supervisor child {} spawn site targets process id {}, expected {}",
                self.debug_name,
                child.debug_name,
                spawn_site.target.as_u32(),
                child.target.as_u32()
            )));
        }
        if spawn_site.supervisor != Some(SupervisorId::from_index(supervisor_index)?) {
            return Err(Error::new(format!(
                "process {} supervisor child {} spawn site references wrong supervisor id",
                self.debug_name, child.debug_name
            )));
        }
        if spawn_site.child != Some(SupervisorChildId::from_index(child_index)?) {
            return Err(Error::new(format!(
                "process {} supervisor child {} spawn site references wrong child id",
                self.debug_name, child.debug_name
            )));
        }
        Ok(())
    }
}
