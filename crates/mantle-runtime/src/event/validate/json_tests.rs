use super::validate_runtime_trace_jsonl;

const RESTARTED_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
    "\"module\":\"json-whitespace\",\"entry_process_id\":0,",
    "\"entry_process\":\"Main\",\"entry_message_id\":0,\"process_count\":2,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"supervisor_child_started\",\"supervisor_pid\":1,",
    "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",\"supervisor_id\":0,",
    "\"child_id\":0,\"child\":\"worker\",\"child_pid\":2,\"child_process_id\":1,",
    "\"child_process\":\"Worker\",\"spawn_site_id\":0,",
    "\"spawn_kind\":\"lexical_supervisor_child\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_failed\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"reason\":\"panic\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":1,",
    "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",\"supervisor_id\":0,",
    "\"child_id\":0,\"child\":\"worker\",\"child_pid\":2,\"child_process_id\":1,",
    "\"child_process\":\"Worker\",\"reason\":\"panic\",\"decision\":\"restarted\",",
    "\"restart_time_ms\":0,\"restart_window_count\":1,\"restart_window_limit\":3,",
    "\"restart_window_ms\":1000,\"new_child_pid\":3,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

#[test]
fn accepts_json_whitespace_around_object_and_scalar_values() {
    let trace = RESTARTED_TRACE
        .replace("\"process_count\":2,", "\"process_count\" : 2 ,")
        .replace(
            "\"trace_schema_version\":1}",
            "\"trace_schema_version\" : 1 }",
        )
        .replace("\"restart_time_ms\":0,", "\"restart_time_ms\" : 0 ,")
        .replace(
            "\"restart_window_count\":1,",
            "\"restart_window_count\" : 1 ,",
        )
        .replace("\"new_child_pid\":3,", "\"new_child_pid\" : 3 ,");
    let padded_trace = trace
        .lines()
        .map(|line| format!(" \t{line}\t \n"))
        .collect::<String>();

    validate_runtime_trace_jsonl(&padded_trace)
        .expect("runtime trace validator must accept JSON whitespace");
}

#[test]
fn accepts_json_whitespace_around_nullable_scalar_values() {
    let trace = RESTARTED_TRACE
        .replace(
            concat!(
                "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
                "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
                "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
            ),
            "",
        )
        .replace(
            "\"event\":\"process_failed\"",
            "\"event\":\"process_stopped\"",
        )
        .replace(
            "\"state_id\":0,\"state\":\"Ready\",\"reason\":\"panic\",",
            "\"reason\":\"normal\",",
        )
        .replace(
            "\"reason\":\"panic\",\"decision\"",
            "\"reason\":\"normal\",\"decision\"",
        )
        .replace(
            "\"decision\":\"restarted\"",
            "\"decision\" : \"not_restarted\"",
        )
        .replace("\"restart_time_ms\":0,", "\"restart_time_ms\" : null ,")
        .replace(
            "\"restart_window_count\":1,",
            "\"restart_window_count\" : 0 ,",
        )
        .replace("\"new_child_pid\":3,", "\"new_child_pid\" : null ,");

    validate_runtime_trace_jsonl(&trace)
        .expect("runtime trace validator must accept whitespace after JSON null values");
}
