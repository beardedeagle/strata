use super::validate_runtime_trace_jsonl;

const TWO_PROCESS_PREFIX: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
    "\"module\":\"process_lifecycle\",\"entry_process_id\":0,",
    "\"entry_process\":\"Main\",\"entry_message_id\":0,\"process_count\":2,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

const MAIN_STOPPED: &str = concat!(
    "{\"event\":\"process_stopped\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
);

const WORKER_STOPPED: &str = concat!(
    "{\"event\":\"process_stopped\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
    "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
);

const WORKER_FAILED: &str = concat!(
    "{\"event\":\"process_failed\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"reason\":\"panic\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

const SUPERVISOR_SCOPE_FAILURE_REASONS: &[&str] = &[
    "supervisor_restart_intensity_exceeded",
    "supervisor_restart_capacity_exceeded",
    "supervisor_restart_throttled",
];

const CHILD_STARTED: &str = concat!(
    "{\"event\":\"supervisor_child_started\",\"supervisor_pid\":1,",
    "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",",
    "\"supervisor_id\":0,\"child_id\":0,\"child\":\"worker\",",
    "\"child_pid\":2,\"child_process_id\":1,\"child_process\":\"Worker\",",
    "\"spawn_site_id\":0,\"spawn_kind\":\"lexical_supervisor_child\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

const RESTARTED_WORKER_SPAWNED: &str = concat!(
    "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

#[test]
fn rejects_spawn_from_terminated_parent_pid() {
    let trace = concat!(
        "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
        "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
        "\"module\":\"spawn_after_stop\",\"entry_process_id\":0,",
        "\"entry_process\":\"Main\",\"entry_message_id\":0,\"process_count\":2,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_stopped\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
        "\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );

    assert_rejects(trace, "spawned_by_pid 1 references terminated");
}

#[test]
fn rejects_sender_pid_after_process_stop() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{MAIN_STOPPED}{}",
        concat!(
            "{\"event\":\"message_accepted\",\"pid\":2,\"process_id\":1,",
            "\"process\":\"Worker\",\"message_id\":0,\"message\":\"Work\",",
            "\"queue_depth\":1,\"sender_pid\":1,",
            "\"trace_schema\":\"mantle-runtime-observability\",",
            "\"trace_schema_version\":1}\n",
        )
    );

    assert_rejects(&trace, "sender_pid 1 references terminated");
}

#[test]
fn rejects_supervisor_event_after_supervisor_stop() {
    let trace = format!("{TWO_PROCESS_PREFIX}{MAIN_STOPPED}{CHILD_STARTED}");

    assert_rejects(&trace, "supervisor_pid 1 references terminated");
}

#[test]
fn rejects_child_started_for_terminated_child_pid() {
    let trace = format!("{TWO_PROCESS_PREFIX}{WORKER_STOPPED}{CHILD_STARTED}");

    assert_rejects(&trace, "child_pid 2 references terminated");
}

#[test]
fn rejects_restart_decision_when_new_child_pid_already_stopped() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{}{}{}",
        concat!(
            "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
            "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
            "\"trace_schema\":\"mantle-runtime-observability\",",
            "\"trace_schema_version\":1}\n",
        ),
        concat!(
            "{\"event\":\"process_stopped\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
            "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
            "\"trace_schema_version\":1}\n",
        ),
        concat!(
            "{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":1,",
            "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",",
            "\"supervisor_id\":0,\"child_id\":0,\"child\":\"worker\",",
            "\"child_pid\":2,\"child_process_id\":1,\"child_process\":\"Worker\",",
            "\"reason\":\"panic\",\"decision\":\"restarted\",\"restart_time_ms\":0,",
            "\"restart_window_count\":1,\"restart_window_limit\":3,",
            "\"restart_window_ms\":1000,\"new_child_pid\":3,",
            "\"trace_schema\":\"mantle-runtime-observability\",",
            "\"trace_schema_version\":1}\n",
        )
    );

    assert_rejects(&trace, "new_child_pid 3 references terminated");
}

#[test]
fn rejects_restart_decision_reusing_child_pid_as_new_child() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{}",
        concat!(
            "{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":1,",
            "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",",
            "\"supervisor_id\":0,\"child_id\":0,\"child\":\"worker\",",
            "\"child_pid\":2,\"child_process_id\":1,\"child_process\":\"Worker\",",
            "\"reason\":\"panic\",\"decision\":\"restarted\",\"restart_time_ms\":0,",
            "\"restart_window_count\":1,\"restart_window_limit\":3,",
            "\"restart_window_ms\":1000,\"new_child_pid\":2,",
            "\"trace_schema\":\"mantle-runtime-observability\",",
            "\"trace_schema_version\":1}\n",
        )
    );

    assert_rejects(&trace, "new_child_pid distinct from child_pid");
}

#[test]
fn permits_restart_decision_for_terminated_supervised_child() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_STOPPED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line(2, Some(3), "normal", Some(0), 1)
    );

    validate_runtime_trace_jsonl(&trace)
        .expect("supervisor evidence may reference a terminated child process");
}

#[test]
fn permits_restart_decision_for_supervisor_scope_failure_reasons() {
    for reason in SUPERVISOR_SCOPE_FAILURE_REASONS {
        let worker_failed = worker_failed_with_reason(reason);
        let trace = format!(
            "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{worker_failed}{RESTARTED_WORKER_SPAWNED}{}",
            supervisor_restart_decision_line(2, Some(3), "panic", Some(0), 1)
        );

        validate_runtime_trace_jsonl(&trace)
            .expect("process_failed failure-class reasons map to supervisor panic exits");
    }
}

#[test]
fn permits_not_restarted_decision_with_zero_restart_window_count() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_STOPPED}{}",
        not_restarted_supervisor_restart_decision_line(0)
    );

    validate_runtime_trace_jsonl(&trace)
        .expect("non-restarting supervisor decisions must retain zero restart-window count");
}

#[test]
fn rejects_zero_supervisor_restart_window_limit() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line_with_restart_window(1, 0, 1000)
    );

    assert_rejects(&trace, "restart window limit must be greater than zero");
}

#[test]
fn rejects_zero_supervisor_restart_window_duration() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line_with_restart_window(1, 3, 0)
    );

    assert_rejects(&trace, "restart window duration must be greater than zero");
}

#[test]
fn rejects_supervisor_restart_window_count_above_limit() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line_with_restart_window(4, 3, 1000)
    );

    assert_rejects(
        &trace,
        "restart window count must not exceed restart_window_limit",
    );
}

#[test]
fn rejects_restarted_decision_with_zero_restart_window_count() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line_with_restart_window(0, 3, 1000)
    );

    assert_rejects(
        &trace,
        "restarted supervisor decision requires nonzero restart_window_count",
    );
}

#[test]
fn rejects_not_restarted_decision_with_nonzero_restart_window_count() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_STOPPED}{}",
        not_restarted_supervisor_restart_decision_line(1)
    );

    assert_rejects(
        &trace,
        "not_restarted supervisor decision requires zero restart_window_count",
    );
}

#[test]
fn rejects_supervisor_restart_decision_without_child_started_evidence() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line(2, Some(3), "panic", Some(0), 1)
    );

    assert_rejects(
        &trace,
        "supervisor restart decision requires prior supervisor_child_started evidence",
    );
}

#[test]
fn rejects_supervisor_restart_decision_before_child_terminal_event() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line(2, Some(3), "panic", Some(0), 1)
    );

    assert_rejects(
        &trace,
        "must emit process_stopped or process_failed before supervisor_restart_decision",
    );
}

#[test]
fn rejects_supervisor_restart_reason_that_disagrees_with_terminal_event() {
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_STOPPED}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line(2, Some(3), "panic", Some(0), 1)
    );

    assert_rejects(&trace, "does not match child terminal event");
}

#[test]
fn rejects_normal_restart_reason_after_supervisor_scope_failure() {
    let worker_failed = worker_failed_with_reason("supervisor_restart_intensity_exceeded");
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{worker_failed}{RESTARTED_WORKER_SPAWNED}{}",
        supervisor_restart_decision_line(2, Some(3), "normal", Some(0), 1)
    );

    assert_rejects(&trace, "does not match child terminal event");
}

#[test]
fn rejects_supervisor_restart_replacement_spawned_by_unrelated_parent() {
    let unrelated_parent_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let unrelated_replacement_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":4,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":3,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{unrelated_parent_spawn}{unrelated_replacement_spawn}{}",
        supervisor_restart_decision_line(2, Some(4), "panic", Some(0), 1)
    );

    assert_rejects(
        &trace,
        "new_child_pid 4 was spawned by runtime process id 3, not supervisor_pid 1",
    );
}

#[test]
fn rejects_stale_child_pid_after_prior_restart_decision() {
    let next_child_failure = concat!(
        "{\"event\":\"process_failed\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"reason\":\"panic\",",
        "\"trace_schema\":\"mantle-runtime-observability\",",
        "\"trace_schema_version\":1}\n",
    );
    let next_child_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":4,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",",
        "\"trace_schema_version\":1}\n",
    );
    let trace = format!(
        "{TWO_PROCESS_PREFIX}{CHILD_STARTED}{WORKER_FAILED}{RESTARTED_WORKER_SPAWNED}{}{next_child_failure}{next_child_spawn}{}",
        supervisor_restart_decision_line(2, Some(3), "panic", Some(0), 1),
        supervisor_restart_decision_line(2, Some(4), "panic", Some(1), 2)
    );

    assert_rejects(&trace, "is not the current child_pid");
}

fn worker_failed_with_reason(reason: &str) -> String {
    format!(
        "{{\"event\":\"process_failed\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",\
         \"state_id\":0,\"state\":\"Ready\",\"reason\":\"{reason}\",\
         \"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}}\n"
    )
}

fn supervisor_restart_decision_line(
    child_pid: u64,
    new_child_pid: Option<u64>,
    reason: &str,
    restart_time_ms: Option<u64>,
    restart_window_count: u64,
) -> String {
    render_supervisor_restart_decision_line(RestartDecisionLine {
        child_pid,
        new_child_pid,
        reason,
        decision: "restarted",
        restart_time_ms,
        restart_window_count,
        restart_window_limit: 3,
        restart_window_ms: 1000,
    })
}

fn supervisor_restart_decision_line_with_restart_window(
    restart_window_count: u64,
    restart_window_limit: u64,
    restart_window_ms: u64,
) -> String {
    render_supervisor_restart_decision_line(RestartDecisionLine {
        child_pid: 2,
        new_child_pid: Some(3),
        reason: "panic",
        decision: "restarted",
        restart_time_ms: Some(0),
        restart_window_count,
        restart_window_limit,
        restart_window_ms,
    })
}

fn not_restarted_supervisor_restart_decision_line(restart_window_count: u64) -> String {
    render_supervisor_restart_decision_line(RestartDecisionLine {
        child_pid: 2,
        new_child_pid: None,
        reason: "normal",
        decision: "not_restarted",
        restart_time_ms: None,
        restart_window_count,
        restart_window_limit: 3,
        restart_window_ms: 1000,
    })
}

struct RestartDecisionLine<'a> {
    child_pid: u64,
    new_child_pid: Option<u64>,
    reason: &'a str,
    decision: &'a str,
    restart_time_ms: Option<u64>,
    restart_window_count: u64,
    restart_window_limit: u64,
    restart_window_ms: u64,
}

fn render_supervisor_restart_decision_line(fields: RestartDecisionLine<'_>) -> String {
    let restart_time = fields
        .restart_time_ms
        .map_or_else(|| "null".to_owned(), |value| value.to_string());
    let new_child = fields
        .new_child_pid
        .map_or_else(|| "null".to_owned(), |value| value.to_string());
    format!(
        "{{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":1,\
         \"supervisor_process_id\":0,\"supervisor_process\":\"Main\",\"supervisor_id\":0,\
         \"child_id\":0,\"child\":\"worker\",\"child_pid\":{},\
         \"child_process_id\":1,\"child_process\":\"Worker\",\"reason\":\"{}\",\
         \"decision\":\"{}\",\"restart_time_ms\":{restart_time},\
         \"restart_window_count\":{},\
         \"restart_window_limit\":{},\
         \"restart_window_ms\":{},\"new_child_pid\":{new_child},\
         \"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}}\n",
        fields.child_pid,
        fields.reason,
        fields.decision,
        fields.restart_window_count,
        fields.restart_window_limit,
        fields.restart_window_ms
    )
}

fn assert_rejects(trace: &str, expected: &str) {
    let err = validate_runtime_trace_jsonl(trace).expect_err("trace should be rejected");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}
