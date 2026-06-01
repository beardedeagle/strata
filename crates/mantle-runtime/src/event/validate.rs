use mantle_artifact::{Error, Result};

use crate::limits::{DEFAULT_MAX_RUNTIME_PROCESSES, DEFAULT_MAX_TRACE_BYTES};

use super::{RUNTIME_TRACE_SCHEMA_ID, RUNTIME_TRACE_SCHEMA_VERSION, RuntimeTraceEventKind};
use json::JsonLine;
use process::RuntimeTraceProcessTable;

mod branch_path;
mod json;
mod process;

const DEFAULT_MAX_TRACE_VALIDATION_EVENTS: usize = 100_000;
const NUMERIC_TRACE_FIELDS: &[&str] = &[
    "process_count",
    "mailbox_bound",
    "queue_depth",
    "max_items",
    "item_count",
    "index",
    "iteration_count",
    "loop_index",
    "restart_window_count",
    "restart_window_limit",
    "restart_window_ms",
];
const NULLABLE_NUMERIC_TRACE_FIELDS: &[&str] = &["restart_time_ms"];
const NULLABLE_TYPED_ID_FIELDS: &[&str] = &["new_child_pid"];
const AUTHORITY_RESULT_VALUES: &[&str] = &["accepted", "denied"];
const BRANCH_VALUES: &[&str] = &["then", "else"];
const BRANCH_SCOPE_VALUES: &[&str] = &["next_state", "action"];
const FAILURE_REASON_VALUES: &[&str] = &[
    "panic",
    "supervisor_restart_capacity_exceeded",
    "supervisor_restart_intensity_exceeded",
    "supervisor_restart_throttled",
];
const OUTPUT_STREAM_VALUES: &[&str] = &["stdout"];
const SPAWN_KIND_VALUES: &[&str] = &["dynamic_local", "lexical_supervisor_child"];
const STEP_RESULT_VALUES: &[&str] = &["Continue", "Stop", "Panic"];
const STOP_REASON_VALUES: &[&str] = &["normal", "supervisor_shutdown", "supervisor_failure"];
const SUPERVISOR_EXIT_REASON_VALUES: &[&str] = &["normal", "panic"];
const SUPERVISOR_RESTART_DECISION_RESTARTED: &str = "restarted";
const SUPERVISOR_RESTART_DECISION_NOT_RESTARTED: &str = "not_restarted";
const SUPERVISOR_RESTART_DECISION_DENIED: &str = "denied";
const SUPERVISOR_RESTART_DECISION_VALUES: &[&str] = &[
    SUPERVISOR_RESTART_DECISION_RESTARTED,
    SUPERVISOR_RESTART_DECISION_NOT_RESTARTED,
    SUPERVISOR_RESTART_DECISION_DENIED,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTraceSummary {
    event_count: usize,
    process_count: usize,
    first_event: RuntimeTraceEventKind,
    last_event: RuntimeTraceEventKind,
}

impl RuntimeTraceSummary {
    pub const fn event_count(self) -> usize {
        self.event_count
    }

    pub const fn process_count(self) -> usize {
        self.process_count
    }

    pub const fn first_event(self) -> RuntimeTraceEventKind {
        self.first_event
    }

    pub const fn last_event(self) -> RuntimeTraceEventKind {
        self.last_event
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTraceValidationLimits {
    max_bytes: usize,
    max_events: usize,
    max_runtime_processes: usize,
}

impl RuntimeTraceValidationLimits {
    pub const fn new(max_bytes: usize, max_events: usize, max_runtime_processes: usize) -> Self {
        Self {
            max_bytes,
            max_events,
            max_runtime_processes,
        }
    }

    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    pub const fn max_events(self) -> usize {
        self.max_events
    }

    pub const fn max_runtime_processes(self) -> usize {
        self.max_runtime_processes
    }

    fn validate(self) -> Result<()> {
        if self.max_bytes == 0 {
            return Err(Error::new(
                "runtime trace validation max_bytes must be greater than zero",
            ));
        }
        if self.max_events == 0 {
            return Err(Error::new(
                "runtime trace validation max_events must be greater than zero",
            ));
        }
        if self.max_runtime_processes == 0 {
            return Err(Error::new(
                "runtime trace validation max_runtime_processes must be greater than zero",
            ));
        }
        Ok(())
    }
}

impl Default for RuntimeTraceValidationLimits {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_TRACE_BYTES,
            DEFAULT_MAX_TRACE_VALIDATION_EVENTS,
            DEFAULT_MAX_RUNTIME_PROCESSES,
        )
    }
}

/// Validates Mantle-owned runtime observability JSONL without executing or
/// authenticating trace data as source or artifact semantics.
///
/// The validator checks renderer-shaped schema fields, per-event field sets,
/// typed-ID widths, renderer-shaped branch paths, `artifact_loaded`
/// first/no-repeat ordering, Mantle's contiguous spawned PID sequence, runtime
/// process-ID correlation, supervisor-child start/restart causality, and
/// terminal process lifecycle boundaries. It remains a read-only evidence gate;
/// Mantle runtime behavior still comes from admitted artifacts and typed
/// runtime state.
pub fn validate_runtime_trace_jsonl(trace: &str) -> Result<RuntimeTraceSummary> {
    validate_runtime_trace_jsonl_with_limits(trace, RuntimeTraceValidationLimits::default())
}

/// Validates Mantle-owned runtime observability JSONL with caller-provided
/// positive validation limits.
pub fn validate_runtime_trace_jsonl_with_limits(
    trace: &str,
    limits: RuntimeTraceValidationLimits,
) -> Result<RuntimeTraceSummary> {
    limits.validate()?;
    RuntimeTraceValidator::new(limits).validate(trace)
}

struct RuntimeTraceValidator {
    limits: RuntimeTraceValidationLimits,
    event_count: usize,
    processes: RuntimeTraceProcessTable,
    first_event: Option<RuntimeTraceEventKind>,
    last_event: Option<RuntimeTraceEventKind>,
}

impl RuntimeTraceValidator {
    fn new(limits: RuntimeTraceValidationLimits) -> Self {
        Self {
            limits,
            event_count: 0,
            processes: RuntimeTraceProcessTable::default(),
            first_event: None,
            last_event: None,
        }
    }

    fn validate(mut self, trace: &str) -> Result<RuntimeTraceSummary> {
        if trace.len() > self.limits.max_bytes {
            return Err(Error::new(format!(
                "runtime trace is {} bytes and exceeds validation byte limit {}",
                trace.len(),
                self.limits.max_bytes
            )));
        }
        for (index, line) in trace.lines().enumerate() {
            let line_number = index
                .checked_add(1)
                .ok_or_else(|| Error::new("runtime trace line number overflowed"))?;
            if self.event_count >= self.limits.max_events {
                return Err(Error::new(format!(
                    "runtime trace exceeds validation event limit {} before line {line_number}",
                    self.limits.max_events
                )));
            }
            self.validate_line(line_number, line)?;
        }

        let first_event = self
            .first_event
            .ok_or_else(|| Error::new("runtime trace is empty"))?;
        let last_event = self
            .last_event
            .ok_or_else(|| Error::new("runtime trace is empty"))?;
        if self.processes.is_empty() {
            return Err(Error::new("runtime trace did not spawn the entry process"));
        }

        Ok(RuntimeTraceSummary {
            event_count: self.event_count,
            process_count: self.processes.len(),
            first_event,
            last_event,
        })
    }

    fn validate_line(&mut self, line_number: usize, line: &str) -> Result<()> {
        let line = JsonLine::new(line_number, line)?;
        let kind = required_event_kind(&line)?;

        if self.event_count == 0 && kind != RuntimeTraceEventKind::ArtifactLoaded {
            return Err(line.error("first runtime trace event must be artifact_loaded"));
        }
        if self.event_count > 0 && kind == RuntimeTraceEventKind::ArtifactLoaded {
            return Err(line.error("artifact_loaded must appear only as the first trace event"));
        }

        validate_allowed_fields(kind, &line)?;

        let schema = line.required_string("trace_schema")?;
        if schema != RUNTIME_TRACE_SCHEMA_ID {
            return Err(line.error(format!(
                "runtime trace schema {schema:?} does not match {RUNTIME_TRACE_SCHEMA_ID:?}"
            )));
        }
        let schema_version = line.required_u64("trace_schema_version")?;
        if schema_version != u64::from(RUNTIME_TRACE_SCHEMA_VERSION) {
            return Err(line.error(format!(
                "runtime trace schema version {schema_version} does not match {RUNTIME_TRACE_SCHEMA_VERSION}"
            )));
        }

        let contract = kind.contract();
        for field in contract.required_fields() {
            line.require_unique_field(field)?;
        }
        for field in contract.typed_id_fields() {
            let value = line.required_u64(field)?;
            validate_trace_typed_id_width(&line, field, value)?;
        }
        for field in contract.optional_typed_id_fields() {
            line.require_unique_optional_field(field)?;
            let value = if NULLABLE_TYPED_ID_FIELDS.contains(field) {
                line.optional_u64_or_null(field)?
            } else {
                line.optional_u64(field)?
            };
            if let Some(value) = value {
                validate_trace_typed_id_width(&line, field, value)?;
            }
        }
        for field in NUMERIC_TRACE_FIELDS {
            line.require_unique_optional_field(field)?;
            if line.value(field)?.is_some() {
                line.required_u64(field)?;
            }
        }
        for field in NULLABLE_NUMERIC_TRACE_FIELDS {
            line.require_unique_optional_field(field)?;
            line.optional_u64_or_null(field)?;
        }
        if kind == RuntimeTraceEventKind::BranchSelected {
            branch_path::validate_branch_path(&line)?;
        }
        for field in contract.metadata_fields() {
            line.required_string(field)?;
        }
        validate_coupled_fields(kind, &line)?;
        validate_runtime_value_domains(kind, &line)?;

        self.processes
            .validate_artifact_process_id_bounds(kind, &line)?;
        self.processes
            .validate_pid_correlation(kind, &line, self.limits.max_runtime_processes)?;
        self.event_count = self
            .event_count
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime trace event count overflowed"))?;
        self.first_event.get_or_insert(kind);
        self.last_event = Some(kind);
        Ok(())
    }
}

fn required_event_kind(line: &JsonLine<'_>) -> Result<RuntimeTraceEventKind> {
    let event = line.required_string("event")?;
    RuntimeTraceEventKind::from_event_name(event).ok_or_else(|| {
        line.error(format!(
            "runtime trace event kind {event:?} is not supported"
        ))
    })
}

fn validate_allowed_fields(kind: RuntimeTraceEventKind, line: &JsonLine<'_>) -> Result<()> {
    let contract = kind.contract();
    line.for_each_field(|field| {
        if contract.required_fields().contains(&field)
            || contract.typed_id_fields().contains(&field)
            || contract.optional_typed_id_fields().contains(&field)
            || contract.metadata_fields().contains(&field)
            || is_allowed_optional_metadata(kind, field)
            || is_allowed_optional_numeric(kind, field)
        {
            Ok(())
        } else {
            Err(line.error(format!(
                "runtime trace field {field:?} is not allowed for event {:?}",
                kind.as_str()
            )))
        }
    })
}

fn is_allowed_optional_metadata(kind: RuntimeTraceEventKind, field: &str) -> bool {
    matches!(
        (kind, field),
        (
            RuntimeTraceEventKind::MessageAccepted
                | RuntimeTraceEventKind::MessageDequeued
                | RuntimeTraceEventKind::ProcessStepped,
            "payload"
        )
    )
}

fn is_allowed_optional_numeric(kind: RuntimeTraceEventKind, field: &str) -> bool {
    matches!(
        (kind, field),
        (RuntimeTraceEventKind::BranchSelected, "loop_index")
    )
}

fn validate_coupled_fields(kind: RuntimeTraceEventKind, line: &JsonLine<'_>) -> Result<()> {
    if matches!(
        kind,
        RuntimeTraceEventKind::MessageAccepted
            | RuntimeTraceEventKind::MessageDequeued
            | RuntimeTraceEventKind::ProcessStepped
    ) {
        validate_payload_fields(line)?;
    }
    if kind == RuntimeTraceEventKind::BranchSelected {
        validate_loop_context_fields(line)?;
    }
    if kind == RuntimeTraceEventKind::SupervisorRestartDecision {
        validate_supervisor_restart_decision_fields(line)?;
    }
    Ok(())
}

fn validate_runtime_value_domains(kind: RuntimeTraceEventKind, line: &JsonLine<'_>) -> Result<()> {
    match kind {
        RuntimeTraceEventKind::ProgramOutput => {
            validate_value_domain(line, "stream", OUTPUT_STREAM_VALUES)
        }
        RuntimeTraceEventKind::SpawnAuthorityChecked => {
            validate_value_domain(line, "spawn_kind", SPAWN_KIND_VALUES)?;
            validate_value_domain(line, "authority_result", AUTHORITY_RESULT_VALUES)
        }
        RuntimeTraceEventKind::BoundarySendChecked => {
            validate_value_domain(line, "boundary_result", AUTHORITY_RESULT_VALUES)
        }
        RuntimeTraceEventKind::BranchSelected => {
            validate_value_domain(line, "branch", BRANCH_VALUES)?;
            validate_value_domain(line, "scope", BRANCH_SCOPE_VALUES)
        }
        RuntimeTraceEventKind::ProcessStepped => {
            validate_value_domain(line, "result", STEP_RESULT_VALUES)
        }
        RuntimeTraceEventKind::ProcessStopped => {
            validate_value_domain(line, "reason", STOP_REASON_VALUES)
        }
        RuntimeTraceEventKind::ProcessFailed => {
            validate_value_domain(line, "reason", FAILURE_REASON_VALUES)
        }
        RuntimeTraceEventKind::SupervisorChildStarted => {
            validate_value_domain(line, "spawn_kind", SPAWN_KIND_VALUES)
        }
        RuntimeTraceEventKind::SupervisorRestartDecision => {
            validate_value_domain(line, "reason", SUPERVISOR_EXIT_REASON_VALUES)?;
            validate_value_domain(line, "decision", SUPERVISOR_RESTART_DECISION_VALUES)
        }
        RuntimeTraceEventKind::ArtifactLoaded
        | RuntimeTraceEventKind::ProcessSpawned
        | RuntimeTraceEventKind::MessageAccepted
        | RuntimeTraceEventKind::MessageDequeued
        | RuntimeTraceEventKind::LoopStarted
        | RuntimeTraceEventKind::LoopIteration
        | RuntimeTraceEventKind::LoopCompleted
        | RuntimeTraceEventKind::StateUpdated => Ok(()),
    }
}

fn validate_value_domain(line: &JsonLine<'_>, field: &str, allowed: &[&str]) -> Result<()> {
    let value = line.required_string(field)?;
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(line.error(format!(
            "runtime trace field {field:?} value {value:?} is not supported"
        )))
    }
}

fn validate_trace_typed_id_width(line: &JsonLine<'_>, field: &str, value: u64) -> Result<()> {
    if is_runtime_pid_field(field) {
        return Ok(());
    }
    validate_trace_u32_value(line, field, value).map(|_| ())
}

fn required_trace_u32(line: &JsonLine<'_>, field: &str) -> Result<u32> {
    validate_trace_u32_value(line, field, line.required_u64(field)?)
}

fn optional_trace_u32(line: &JsonLine<'_>, field: &str) -> Result<Option<u32>> {
    line.optional_u64(field)?
        .map(|value| validate_trace_u32_value(line, field, value))
        .transpose()
}

fn validate_trace_u32_value(line: &JsonLine<'_>, field: &str, value: u64) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        line.error(format!(
            "runtime trace field {field:?} value {value} does not fit into u32"
        ))
    })
}

fn is_runtime_pid_field(field: &str) -> bool {
    matches!(
        field,
        "pid"
            | "spawned_by_pid"
            | "sender_pid"
            | "payload_pid"
            | "supervisor_pid"
            | "child_pid"
            | "new_child_pid"
    )
}

fn validate_payload_fields(line: &JsonLine<'_>) -> Result<()> {
    line.require_unique_optional_field("payload")?;
    let has_payload = line.optional_string("payload")?.is_some();
    let has_payload_type = line.optional_u64("payload_type_id")?.is_some();
    if has_payload != has_payload_type {
        return Err(line
            .error("runtime trace payload fields must include both payload_type_id and payload"));
    }

    let has_process_id = line.optional_u64("payload_process_id")?.is_some();
    let has_process_pid = line.optional_u64("payload_pid")?.is_some();
    if has_process_id != has_process_pid {
        return Err(line.error(
            "runtime trace process reference payload fields must include both payload_process_id and payload_pid",
        ));
    }
    if has_process_id && !has_payload {
        return Err(line.error(
            "runtime trace process reference payload fields require payload_type_id and payload",
        ));
    }
    Ok(())
}

fn validate_supervisor_restart_decision_fields(line: &JsonLine<'_>) -> Result<()> {
    let decision = line.required_string("decision")?;
    let new_child_pid = line.optional_u64_or_null("new_child_pid")?;
    let restart_time_ms = line.optional_u64_or_null("restart_time_ms")?;
    let restart_window_count = required_trace_u32(line, "restart_window_count")?;
    let restart_window_limit = required_trace_u32(line, "restart_window_limit")?;
    let restart_window_ms = line.required_u64("restart_window_ms")?;

    if restart_window_limit == 0 {
        return Err(
            line.error("runtime trace supervisor restart window limit must be greater than zero")
        );
    }
    if restart_window_ms == 0 {
        return Err(line
            .error("runtime trace supervisor restart window duration must be greater than zero"));
    }
    if restart_window_count > restart_window_limit {
        return Err(line.error(
            "runtime trace supervisor restart window count must not exceed restart_window_limit",
        ));
    }

    if decision == SUPERVISOR_RESTART_DECISION_RESTARTED {
        if new_child_pid.is_none() {
            return Err(
                line.error("runtime trace restarted supervisor decision requires new_child_pid")
            );
        }
        if new_child_pid == Some(line.required_u64("child_pid")?) {
            return Err(line.error(
                "runtime trace restarted supervisor decision requires new_child_pid distinct from child_pid",
            ));
        }
        if restart_time_ms.is_none() {
            return Err(
                line.error("runtime trace restarted supervisor decision requires restart_time_ms")
            );
        }
        if restart_window_count == 0 {
            return Err(line.error(
                "runtime trace restarted supervisor decision requires nonzero restart_window_count",
            ));
        }
        return Ok(());
    }

    if decision == SUPERVISOR_RESTART_DECISION_DENIED {
        if new_child_pid.is_some() {
            return Err(line.error(
                "runtime trace non-restart supervisor decision must set new_child_pid to null",
            ));
        }
        if restart_time_ms.is_none() {
            return Err(
                line.error("runtime trace denied supervisor decision requires restart_time_ms")
            );
        }
        return Ok(());
    }

    if decision == SUPERVISOR_RESTART_DECISION_NOT_RESTARTED {
        if new_child_pid.is_some() {
            return Err(line.error(
                "runtime trace non-restart supervisor decision must set new_child_pid to null",
            ));
        }
        if restart_time_ms.is_some() {
            return Err(line.error(
                "runtime trace not_restarted supervisor decision must set restart_time_ms to null",
            ));
        }
        if restart_window_count != 0 {
            return Err(line.error(
                "runtime trace not_restarted supervisor decision requires zero restart_window_count",
            ));
        }
        return Ok(());
    }

    Ok(())
}

fn validate_loop_context_fields(line: &JsonLine<'_>) -> Result<()> {
    let has_element_id = line.optional_u64("loop_element_id")?.is_some();
    let has_index = line.optional_u64("loop_index")?.is_some();
    if has_element_id != has_index {
        return Err(line.error(
            "runtime trace loop context fields must include both loop_element_id and loop_index",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod json_tests;
#[cfg(test)]
mod process_tests;
#[cfg(test)]
mod tests;
