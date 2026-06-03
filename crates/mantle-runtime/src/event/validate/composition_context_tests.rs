use super::*;

const VALID_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",\"module\":\"hello\",",
    "\"entry_process_id\":0,\"entry_process\":\"Main\",\"entry_message_id\":0,",
    "\"process_count\":1,\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"message_accepted\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"message_id\":0,\"message\":\"Start\",\"queue_depth\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

const BOUND_ARTIFACT_CONTEXT: &str = "\"deployment_id\":0,\"composition_id\":0,\"trace_schema\"";
const BOUND_PROCESS_CONTEXT: &str =
    "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":0,\"trace_schema\"";

#[test]
fn validates_optional_composition_correlation_fields() {
    let trace = trace_with_contexts([
        BOUND_ARTIFACT_CONTEXT,
        BOUND_PROCESS_CONTEXT,
        BOUND_PROCESS_CONTEXT,
    ]);

    validate_runtime_trace_jsonl(&trace).expect("composition correlation fields should validate");
}

#[test]
fn validates_bound_non_process_supervisor_event_without_component_instance() {
    let trace = concat!(
        "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
        "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",\"module\":\"hello\",",
        "\"entry_process_id\":0,\"entry_process\":\"Main\",\"entry_message_id\":0,",
        "\"process_count\":2,\"deployment_id\":0,\"composition_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
        "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"supervisor_child_started\",\"supervisor_pid\":1,",
        "\"supervisor_process_id\":0,\"supervisor_process\":\"Main\",\"supervisor_id\":0,",
        "\"child_id\":0,\"child\":\"worker\",\"child_pid\":2,\"child_process_id\":1,",
        "\"child_process\":\"Worker\",\"spawn_site_id\":0,",
        "\"spawn_kind\":\"lexical_supervisor_child\",",
        "\"deployment_id\":0,\"composition_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );

    validate_runtime_trace_jsonl(trace)
        .expect("bound supervisor trace should validate without component_instance_id");
}

#[test]
fn rejects_partial_composition_correlation_fields() {
    assert_rejects(
        &VALID_TRACE.replacen(
            "\"trace_schema\"",
            "\"component_instance_id\":0,\"trace_schema\"",
            1,
        ),
        "component_instance_id requires deployment_id and composition_id",
    );
    assert_rejects(
        &VALID_TRACE.replacen(
            "\"trace_schema\"",
            "\"deployment_id\":0,\"trace_schema\"",
            1,
        ),
        "must include both deployment_id and composition_id",
    );
}

#[test]
fn rejects_component_instance_on_non_process_scoped_event() {
    assert_rejects(
        &VALID_TRACE.replacen(
            "\"trace_schema\"",
            "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":0,\"trace_schema\"",
            1,
        ),
        "component_instance_id requires a process-scoped event",
    );
}

#[test]
fn rejects_inconsistent_composition_correlation_fields() {
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            "\"deployment_id\":0,\"composition_id\":1,\"component_instance_id\":0,\"trace_schema\"",
            BOUND_PROCESS_CONTEXT,
        ]),
        "composition context changed after artifact_loaded",
    );
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            "\"trace_schema\"",
            BOUND_PROCESS_CONTEXT,
        ]),
        "composition context must appear on every event",
    );
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            BOUND_PROCESS_CONTEXT,
            "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":1,\"trace_schema\"",
        ])
        .replace("\"process_count\":1", "\"process_count\":2"),
        "component_instance_id changed for process_id 0",
    );
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            "\"deployment_id\":0,\"composition_id\":0,\"trace_schema\"",
            BOUND_PROCESS_CONTEXT,
        ]),
        "process_id 0 requires component_instance_id",
    );
}

#[test]
fn rejects_out_of_range_component_instance_correlation() {
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":1,\"trace_schema\"",
            "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":1,\"trace_schema\"",
        ]),
        "component_instance_id 1 is outside runtime trace composition component table",
    );
}

#[test]
fn rejects_duplicate_component_instance_correlation() {
    let trace = concat!(
        "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
        "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",\"module\":\"hello\",",
        "\"entry_process_id\":0,\"entry_process\":\"Main\",\"entry_message_id\":0,",
        "\"process_count\":2,\"deployment_id\":0,\"composition_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
        "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"deployment_id\":0,\"composition_id\":0,\"component_instance_id\":0,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );

    assert_rejects(
        trace,
        "component_instance_id 0 is already correlated with process_id 0",
    );
}

#[test]
fn rejects_bound_process_trace_that_never_establishes_component_correlation() {
    assert_rejects(
        &trace_with_contexts([
            BOUND_ARTIFACT_CONTEXT,
            "\"deployment_id\":0,\"composition_id\":0,\"trace_schema\"",
            "\"deployment_id\":0,\"composition_id\":0,\"trace_schema\"",
        ]),
        "process_id 0 requires component_instance_id",
    );
}

#[test]
fn rejects_nonzero_composition_deployment_id() {
    assert_rejects(
        &trace_with_contexts([
            "\"deployment_id\":7,\"composition_id\":0,\"trace_schema\"",
            "\"deployment_id\":7,\"composition_id\":0,\"component_instance_id\":0,\"trace_schema\"",
            "\"deployment_id\":7,\"composition_id\":0,\"component_instance_id\":0,\"trace_schema\"",
        ]),
        "deployment_id must be 0",
    );
}

fn trace_with_contexts(contexts: [&str; 3]) -> String {
    let mut trace = String::new();
    for (line, context) in VALID_TRACE.lines().zip(contexts) {
        trace.push_str(&line.replace("\"trace_schema\"", context));
        trace.push('\n');
    }
    trace
}

fn assert_rejects(trace: &str, expected: &str) {
    let err = validate_runtime_trace_jsonl(trace).expect_err("trace should be rejected");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}
