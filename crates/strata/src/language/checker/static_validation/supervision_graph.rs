use super::super::super::checked::{CheckedProcess, CheckedProcessId};
use super::super::super::diagnostic::{Error, Result};

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Visiting,
    Done,
}

pub(super) fn validate_supervision_graph_acyclic(processes: &[CheckedProcess]) -> Result<()> {
    if !processes
        .iter()
        .any(|process| !process.supervisor_plans().is_empty())
    {
        return Ok(());
    }

    let mut states = vec![VisitState::Unseen; processes.len()];
    let mut stack = Vec::with_capacity(processes.len());
    for index in 0..processes.len() {
        visit_process(index, processes, &mut states, &mut stack)?;
    }
    Ok(())
}

fn visit_process(
    index: usize,
    processes: &[CheckedProcess],
    states: &mut [VisitState],
    stack: &mut Vec<usize>,
) -> Result<()> {
    match states[index] {
        VisitState::Done => return Ok(()),
        VisitState::Visiting => {
            return Err(cycle_error(processes, stack, index));
        }
        VisitState::Unseen => {}
    }

    states[index] = VisitState::Visiting;
    stack.push(index);
    for target in supervised_targets(&processes[index]) {
        let target_index = target.index();
        if target_index >= processes.len() {
            return Err(Error::new(format!(
                "process {} supervisor child targets undefined process id {}",
                processes[index].debug_name(),
                target.as_u32()
            )));
        }
        if states[target_index] == VisitState::Visiting {
            return Err(cycle_error(processes, stack, target_index));
        }
        visit_process(target_index, processes, states, stack)?;
    }
    stack.pop();
    states[index] = VisitState::Done;
    Ok(())
}

fn supervised_targets(process: &CheckedProcess) -> impl Iterator<Item = CheckedProcessId> + '_ {
    process
        .supervisor_plans()
        .iter()
        .flat_map(|plan| plan.children().iter().map(|child| child.target()))
}

fn cycle_error(processes: &[CheckedProcess], stack: &[usize], cycle_start: usize) -> Error {
    let Some(first_cycle_entry) = stack.iter().position(|index| *index == cycle_start) else {
        return Error::new("local supervisor graph cycle stack is inconsistent");
    };
    let mut path = stack[first_cycle_entry..]
        .iter()
        .map(|index| processes[*index].debug_name().to_string())
        .collect::<Vec<_>>();
    path.push(processes[cycle_start].debug_name().to_string());
    Error::new(format!(
        "local supervisor graph contains cycle {}",
        path.join(" -> ")
    ))
}
