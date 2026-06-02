use super::*;

const VALID_EFFECT_OUTCOME_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
    "\"module\":\"effect_outcome\",\"entry_process_id\":0,\"entry_process\":\"Main\",",
    "\"entry_message_id\":0,\"process_count\":2,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"effect_outcome_bound\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"outcome_id\":0,\"action\":\"spawn\",\"target_process_id\":1,",
    "\"spawn_site_id\":0,\"outcome_result\":\"exhausted\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);

#[test]
fn validates_effect_outcome_bound_trace() {
    validate_runtime_trace_jsonl(VALID_EFFECT_OUTCOME_TRACE)
        .expect("effect outcome trace should validate");
    validate_runtime_trace_jsonl(&VALID_EFFECT_OUTCOME_TRACE.replace(
        "\"outcome_result\":\"exhausted\"",
        "\"outcome_result\":\"backend_unavailable\"",
    ))
    .expect("backend-unavailable spawn outcome trace should validate");
}

#[test]
fn rejects_effect_outcome_enum_metadata_outside_closed_domains() {
    assert_rejects(
        &VALID_EFFECT_OUTCOME_TRACE.replace("\"action\":\"spawn\"", "\"action\":\"driver\""),
        "value \"driver\" is not supported",
    );
    assert_rejects(
        &VALID_EFFECT_OUTCOME_TRACE.replace(
            "\"outcome_result\":\"exhausted\"",
            "\"outcome_result\":\"full\"",
        ),
        "value \"full\" is not supported",
    );
}

#[test]
fn rejects_effect_outcome_action_field_mismatches() {
    assert_rejects(
        &VALID_EFFECT_OUTCOME_TRACE.replace(
            "\"spawn_site_id\":0,",
            "\"spawn_site_id\":0,\"message_id\":0,",
        ),
        "spawn effect outcome must not include message_id",
    );

    let send_trace = VALID_EFFECT_OUTCOME_TRACE
        .replace("\"action\":\"spawn\"", "\"action\":\"send\"")
        .replace("\"spawn_site_id\":0,", "\"message_id\":0,")
        .replace(
            "\"outcome_result\":\"exhausted\"",
            "\"outcome_result\":\"mailbox_closed\"",
        );
    validate_runtime_trace_jsonl(&send_trace).expect("send outcome trace should validate");

    assert_rejects(
        &send_trace.replace("\"message_id\":0,", "\"message_id\":0,\"spawn_site_id\":0,"),
        "send effect outcome must not include spawn_site_id",
    );

    assert_rejects(
        &VALID_EFFECT_OUTCOME_TRACE.replace(
            "\"outcome_result\":\"exhausted\"",
            "\"outcome_result\":\"mailbox_closed\"",
        ),
        "value \"mailbox_closed\" is not supported",
    );
    assert_rejects(
        &send_trace.replace(
            "\"outcome_result\":\"mailbox_closed\"",
            "\"outcome_result\":\"backend_unavailable\"",
        ),
        "value \"backend_unavailable\" is not supported",
    );
}

fn assert_rejects(trace: &str, expected: &str) {
    let err = validate_runtime_trace_jsonl(trace).expect_err("trace should be rejected");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}
