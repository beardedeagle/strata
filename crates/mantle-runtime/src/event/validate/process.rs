use mantle_artifact::{MAX_PROCESS_COUNT, Result};

use super::json::JsonLine;
use super::{optional_trace_u32, required_trace_u32};
use crate::event::{RuntimeProcessId, RuntimeTraceEventKind};

const ARTIFACT_PROCESS_ID_FIELDS: &[&str] = &[
    "entry_process_id",
    "process_id",
    "target_process_id",
    "payload_process_id",
    "supervisor_process_id",
    "child_process_id",
];

#[derive(Debug, Default)]
pub(super) struct RuntimeTraceProcessTable {
    seen_processes: Vec<RuntimeTraceProcessBinding>,
    supervised_children: Vec<RuntimeTraceSupervisorChildBinding>,
    artifact_process_count: Option<u32>,
    entry_process_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeTraceProcessBinding {
    pid: u64,
    process_id: u32,
    spawned_by_pid: Option<u64>,
    state: RuntimeTraceProcessState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeTraceProcessState {
    Running,
    Terminated(RuntimeTraceTerminalReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeTraceTerminalReason {
    Normal,
    Panic,
    Other,
}

impl RuntimeTraceTerminalReason {
    fn from_terminal_event(kind: RuntimeTraceEventKind, line: &JsonLine<'_>) -> Result<Self> {
        match kind {
            RuntimeTraceEventKind::ProcessStopped => match line.required_string("reason")? {
                "normal" => Ok(Self::Normal),
                _ => Ok(Self::Other),
            },
            RuntimeTraceEventKind::ProcessFailed => {
                // `process_failed.reason` is detailed failure-class metadata;
                // supervisor restart decisions use the coarser exit class.
                line.required_string("reason")?;
                Ok(Self::Panic)
            }
            _ => Ok(Self::Other),
        }
    }

    const fn as_supervisor_reason(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Panic => "panic",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeTraceSupervisorChildBinding {
    key: RuntimeTraceSupervisorChildKey,
    current_child_pid: u64,
    closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeTraceSupervisorChildKey {
    supervisor_pid: u64,
    supervisor_process_id: u32,
    supervisor_id: u32,
    child_id: u32,
    child_process_id: u32,
}

impl RuntimeTraceSupervisorChildKey {
    fn from_line(line: &JsonLine<'_>) -> Result<Self> {
        Ok(Self {
            supervisor_pid: line.required_u64("supervisor_pid")?,
            supervisor_process_id: required_trace_u32(line, "supervisor_process_id")?,
            supervisor_id: required_trace_u32(line, "supervisor_id")?,
            child_id: required_trace_u32(line, "child_id")?,
            child_process_id: required_trace_u32(line, "child_process_id")?,
        })
    }
}

impl RuntimeTraceProcessTable {
    pub(super) fn is_empty(&self) -> bool {
        self.seen_processes.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.seen_processes.len()
    }

    pub(super) fn validate_artifact_process_id_bounds(
        &mut self,
        kind: RuntimeTraceEventKind,
        line: &JsonLine<'_>,
    ) -> Result<()> {
        if kind == RuntimeTraceEventKind::ArtifactLoaded {
            let process_count = required_trace_u32(line, "process_count")?;
            if process_count == 0 {
                return Err(line.error("runtime trace process_count must be greater than zero"));
            }
            if u64::from(process_count) > MAX_PROCESS_COUNT as u64 {
                return Err(line.error(format!(
                    "runtime trace process_count {process_count} exceeds Mantle artifact process limit {MAX_PROCESS_COUNT}"
                )));
            }
            let entry_process_id = required_trace_u32(line, "entry_process_id")?;
            if entry_process_id >= process_count {
                return Err(line.error(format!(
                    "entry_process_id {entry_process_id} is outside artifact process_count {process_count}"
                )));
            }
            self.artifact_process_count = Some(process_count);
            self.entry_process_id = Some(entry_process_id);
        }

        let process_count = self.artifact_process_count.ok_or_else(|| {
            line.error("runtime trace artifact process_count was not established")
        })?;
        for field in ARTIFACT_PROCESS_ID_FIELDS {
            if let Some(process_id) = optional_trace_u32(line, field)? {
                if process_id >= process_count {
                    return Err(line.error(format!(
                        "{field} {process_id} is outside artifact process_count {process_count}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_pid_correlation(
        &mut self,
        kind: RuntimeTraceEventKind,
        line: &JsonLine<'_>,
        max_runtime_processes: usize,
    ) -> Result<()> {
        let spawned_by_pid = line.optional_u64("spawned_by_pid")?;
        if let Some(parent_pid) = spawned_by_pid {
            self.require_running_seen_pid_index(line, "spawned_by_pid", parent_pid)?;
        }

        match kind {
            RuntimeTraceEventKind::ProcessSpawned => {
                self.register_spawn(line, spawned_by_pid, max_runtime_processes)?;
            }
            RuntimeTraceEventKind::ArtifactLoaded => {}
            _ => {
                if let Some(pid) = line.optional_u64("pid")? {
                    let process_id = required_trace_u32(line, "process_id")?;
                    let index = self.require_pid_process_binding(
                        line,
                        "pid",
                        pid,
                        "process_id",
                        process_id,
                    )?;
                    self.validate_subject_lifecycle(line, kind, index)?;
                }
            }
        }

        if let Some(pid) = line.optional_u64_or_null("sender_pid")? {
            self.require_running_seen_pid_index(line, "sender_pid", pid)?;
        }
        self.require_optional_pid_process_binding(line, "payload_pid", "payload_process_id")?;
        self.require_optional_running_pid_process_binding(
            line,
            "supervisor_pid",
            "supervisor_process_id",
        )?;
        if kind == RuntimeTraceEventKind::SupervisorChildStarted {
            self.require_optional_running_pid_process_binding(
                line,
                "child_pid",
                "child_process_id",
            )?;
        } else {
            self.require_optional_pid_process_binding(line, "child_pid", "child_process_id")?;
        }
        if let Some(pid) = line.optional_u64_or_null("new_child_pid")? {
            let process_id = required_trace_u32(line, "child_process_id")?;
            self.require_running_pid_process_binding(
                line,
                "new_child_pid",
                pid,
                "child_process_id",
                process_id,
            )?;
        }

        match kind {
            RuntimeTraceEventKind::SupervisorChildStarted => {
                self.register_supervisor_child_started(line)?;
            }
            RuntimeTraceEventKind::SupervisorRestartDecision => {
                self.validate_supervisor_restart_decision(line)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn register_spawn(
        &mut self,
        line: &JsonLine<'_>,
        spawned_by_pid: Option<u64>,
        max_runtime_processes: usize,
    ) -> Result<()> {
        let pid = line.required_u64("pid")?;
        let process_id = required_trace_u32(line, "process_id")?;
        if pid == 0 {
            return Err(line.error("runtime process id must be greater than zero"));
        }
        if self.seen_processes.is_empty() {
            let entry_process_id = self
                .entry_process_id
                .ok_or_else(|| line.error("runtime trace entry_process_id was not established"))?;
            if process_id != entry_process_id {
                return Err(line.error(format!(
                    "first spawned process_id {process_id} must match entry_process_id {entry_process_id}"
                )));
            }
        } else if spawned_by_pid.is_none() {
            return Err(line.error("non-entry process_spawned event requires spawned_by_pid"));
        }
        if self.binding_index_for_pid(pid).is_some() {
            return Err(line.error(format!("runtime process id {pid} was reused")));
        }
        let expected_pid = self
            .seen_processes
            .last()
            .map(|previous| {
                previous
                    .pid
                    .checked_add(1)
                    .ok_or_else(|| line.error("runtime process id sequence overflowed"))
            })
            .transpose()?
            .unwrap_or(RuntimeProcessId::FIRST.as_u64());
        if pid != expected_pid {
            return Err(line.error(format!(
                "runtime process id {pid} must be next spawned process id {expected_pid}"
            )));
        }
        if self.seen_processes.len() >= max_runtime_processes {
            return Err(line.error(format!(
                "runtime trace exceeds validation runtime process limit {max_runtime_processes}"
            )));
        }
        self.seen_processes.push(RuntimeTraceProcessBinding {
            pid,
            process_id,
            spawned_by_pid,
            state: RuntimeTraceProcessState::Running,
        });
        Ok(())
    }

    fn register_supervisor_child_started(&mut self, line: &JsonLine<'_>) -> Result<()> {
        let key = RuntimeTraceSupervisorChildKey::from_line(line)?;
        let child_pid = line.required_u64("child_pid")?;
        self.require_spawn_parent(line, "child_pid", child_pid, key.supervisor_pid)?;
        if self.supervisor_child_index_for_key(key).is_some() {
            return Err(line.error(format!(
                "supervisor child slot supervisor_pid {} supervisor_id {} child_id {} already has child evidence",
                key.supervisor_pid, key.supervisor_id, key.child_id
            )));
        }
        self.supervised_children
            .push(RuntimeTraceSupervisorChildBinding {
                key,
                current_child_pid: child_pid,
                closed: false,
            });
        Ok(())
    }

    fn validate_supervisor_restart_decision(&mut self, line: &JsonLine<'_>) -> Result<()> {
        let key = RuntimeTraceSupervisorChildKey::from_line(line)?;
        let child_pid = line.required_u64("child_pid")?;
        let slot_index = self.supervisor_child_index_for_key(key).ok_or_else(|| {
            line.error(
                "supervisor restart decision requires prior supervisor_child_started evidence",
            )
        })?;
        let slot = self.supervised_children[slot_index];
        if slot.closed {
            return Err(line.error(format!(
                "supervisor child slot supervisor_pid {} supervisor_id {} child_id {} is already closed",
                key.supervisor_pid, key.supervisor_id, key.child_id
            )));
        }
        if slot.current_child_pid != child_pid {
            return Err(line.error(format!(
                "supervisor restart child_pid {child_pid} is not the current child_pid {} for supervisor_pid {} supervisor_id {} child_id {}",
                slot.current_child_pid, key.supervisor_pid, key.supervisor_id, key.child_id
            )));
        }

        let child_index = self.require_pid_process_binding(
            line,
            "child_pid",
            child_pid,
            "child_process_id",
            key.child_process_id,
        )?;
        let terminal_reason = self.require_terminated_child_for_restart(line, child_index)?;
        let supervisor_reason = line.required_string("reason")?;
        if terminal_reason.as_supervisor_reason() != supervisor_reason {
            return Err(line.error(format!(
                "supervisor restart reason {supervisor_reason:?} does not match child terminal event {:?}",
                terminal_reason.as_supervisor_reason()
            )));
        }

        if line.required_string("decision")? == "restarted" {
            let new_child_pid = line.optional_u64_or_null("new_child_pid")?.ok_or_else(|| {
                line.error("runtime trace restarted supervisor decision requires new_child_pid")
            })?;
            self.require_spawn_parent(line, "new_child_pid", new_child_pid, key.supervisor_pid)?;
            self.supervised_children[slot_index].current_child_pid = new_child_pid;
        } else {
            self.supervised_children[slot_index].closed = true;
        }
        Ok(())
    }

    fn validate_subject_lifecycle(
        &mut self,
        line: &JsonLine<'_>,
        kind: RuntimeTraceEventKind,
        index: usize,
    ) -> Result<()> {
        let binding = self.seen_processes[index];
        match kind {
            RuntimeTraceEventKind::ProcessStopped | RuntimeTraceEventKind::ProcessFailed => {
                if matches!(binding.state, RuntimeTraceProcessState::Terminated(_)) {
                    return Err(line.error(format!(
                        "runtime process id {} already terminated before {:?}",
                        binding.pid,
                        kind.as_str()
                    )));
                }
                self.seen_processes[index].state = RuntimeTraceProcessState::Terminated(
                    RuntimeTraceTerminalReason::from_terminal_event(kind, line)?,
                );
            }
            _ => {
                if matches!(binding.state, RuntimeTraceProcessState::Terminated(_)) {
                    return Err(line.error(format!(
                        "runtime process id {} emitted {:?} after process termination",
                        binding.pid,
                        kind.as_str()
                    )));
                }
            }
        }
        Ok(())
    }

    fn require_optional_running_pid_process_binding(
        &self,
        line: &JsonLine<'_>,
        pid_field: &str,
        process_field: &str,
    ) -> Result<()> {
        if let Some(pid) = line.optional_u64(pid_field)? {
            let process_id = required_trace_u32(line, process_field)?;
            self.require_running_pid_process_binding(
                line,
                pid_field,
                pid,
                process_field,
                process_id,
            )?;
        }
        Ok(())
    }

    fn require_optional_pid_process_binding(
        &self,
        line: &JsonLine<'_>,
        pid_field: &str,
        process_field: &str,
    ) -> Result<()> {
        if let Some(pid) = line.optional_u64(pid_field)? {
            let process_id = required_trace_u32(line, process_field)?;
            self.require_pid_process_binding(line, pid_field, pid, process_field, process_id)?;
        }
        Ok(())
    }

    fn require_pid_process_binding(
        &self,
        line: &JsonLine<'_>,
        pid_field: &str,
        pid: u64,
        process_field: &str,
        process_id: u32,
    ) -> Result<usize> {
        let index = self.require_seen_pid_index(line, pid_field, pid)?;
        let binding = self.seen_processes[index];
        if binding.process_id != process_id {
            return Err(line.error(format!(
                "{pid_field} {pid} is bound to process_id {}, but {process_field} is {process_id}",
                binding.process_id
            )));
        }
        Ok(index)
    }

    fn require_running_pid_process_binding(
        &self,
        line: &JsonLine<'_>,
        pid_field: &str,
        pid: u64,
        process_field: &str,
        process_id: u32,
    ) -> Result<usize> {
        let index =
            self.require_pid_process_binding(line, pid_field, pid, process_field, process_id)?;
        self.require_running_binding(line, pid_field, index)?;
        Ok(index)
    }

    fn require_running_seen_pid_index(
        &self,
        line: &JsonLine<'_>,
        field: &str,
        pid: u64,
    ) -> Result<usize> {
        let index = self.require_seen_pid_index(line, field, pid)?;
        self.require_running_binding(line, field, index)?;
        Ok(index)
    }

    fn require_seen_pid_index(&self, line: &JsonLine<'_>, field: &str, pid: u64) -> Result<usize> {
        if pid == 0 {
            return Err(line.error(format!("{field} must be greater than zero")));
        }
        self.binding_index_for_pid(pid)
            .ok_or_else(|| line.error(format!("{field} {pid} was not previously spawned")))
    }

    fn require_running_binding(
        &self,
        line: &JsonLine<'_>,
        field: &str,
        index: usize,
    ) -> Result<()> {
        let binding = self.seen_processes[index];
        if matches!(binding.state, RuntimeTraceProcessState::Terminated(_)) {
            Err(line.error(format!(
                "{field} {} references terminated runtime process",
                binding.pid
            )))
        } else {
            Ok(())
        }
    }

    fn require_terminated_child_for_restart(
        &self,
        line: &JsonLine<'_>,
        index: usize,
    ) -> Result<RuntimeTraceTerminalReason> {
        let binding = self.seen_processes[index];
        match binding.state {
            RuntimeTraceProcessState::Terminated(reason) => Ok(reason),
            RuntimeTraceProcessState::Running => Err(line.error(format!(
                "child_pid {} must emit process_stopped or process_failed before supervisor_restart_decision",
                binding.pid
            ))),
        }
    }

    fn require_spawn_parent(
        &self,
        line: &JsonLine<'_>,
        pid_field: &str,
        pid: u64,
        expected_parent_pid: u64,
    ) -> Result<()> {
        let index = self.require_seen_pid_index(line, pid_field, pid)?;
        let binding = self.seen_processes[index];
        match binding.spawned_by_pid {
            Some(spawned_by_pid) if spawned_by_pid == expected_parent_pid => Ok(()),
            Some(spawned_by_pid) => Err(line.error(format!(
                "{pid_field} {pid} was spawned by runtime process id {spawned_by_pid}, not supervisor_pid {expected_parent_pid}"
            ))),
            None => Err(line.error(format!(
                "{pid_field} {pid} has no spawned_by_pid evidence for supervisor_pid {expected_parent_pid}"
            ))),
        }
    }

    fn binding_index_for_pid(&self, pid: u64) -> Option<usize> {
        self.seen_processes
            .binary_search_by_key(&pid, |binding| binding.pid)
            .ok()
    }

    fn supervisor_child_index_for_key(&self, key: RuntimeTraceSupervisorChildKey) -> Option<usize> {
        self.supervised_children
            .iter()
            .position(|binding| binding.key == key)
    }
}
