use super::*;
use crate::event::RUNTIME_BRANCH_PATH_CAPACITY;

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
const VALID_BRANCH_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",\"module\":\"branch\",",
    "\"entry_process_id\":0,\"entry_process\":\"Main\",\"entry_message_id\":0,",
    "\"process_count\":1,\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"branch_selected\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"message_id\":0,\"message\":\"Start\",\"branch\":\"then\",\"scope\":\"action\",",
    "\"branch_path\":[0],\"condition_type_id\":1,\"condition\":\"True\",",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
);
const VALID_STEP_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",\"module\":\"step\",",
    "\"entry_process_id\":0,\"entry_process\":\"Main\",\"entry_message_id\":0,",
    "\"process_count\":1,\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
    "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
    "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    "{\"event\":\"process_stepped\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
    "\"message_id\":0,\"message\":\"Start\",\"result\":\"Stop\",\"state_id\":1,",
    "\"state\":\"Done\",\"trace_schema\":\"mantle-runtime-observability\",",
    "\"trace_schema_version\":1}\n",
);
const VALID_SUPERVISOR_RESTART_TRACE: &str = concat!(
    "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
    "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
    "\"module\":\"supervision\",\"entry_process_id\":0,\"entry_process\":\"Main\",",
    "\"entry_message_id\":0,\"process_count\":2,",
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
fn validates_schema_fields_and_runtime_pid_correlation() {
    let summary = validate_runtime_trace_jsonl(VALID_TRACE).expect("valid trace should validate");

    assert_eq!(summary.event_count(), 3);
    assert_eq!(summary.process_count(), 1);
    assert_eq!(summary.first_event(), RuntimeTraceEventKind::ArtifactLoaded);
    assert_eq!(summary.last_event(), RuntimeTraceEventKind::MessageAccepted);
}

#[test]
fn rejects_trace_without_entry_spawn() {
    let artifact_only = VALID_TRACE
        .split_once('\n')
        .map(|(line, _)| line)
        .expect("valid trace starts with artifact_loaded");

    assert_rejects(artifact_only, "did not spawn the entry process");
}

#[test]
fn validates_payload_and_branch_contract_fields() {
    let payload_trace = VALID_TRACE.replace(
        "\"message_id\":0,\"message\":\"Start\",\"queue_depth\":1,",
        "\"message_id\":0,\"message\":\"Start\",\"payload_type_id\":2,\"payload\":\"Job{phase:Ready}\",\"queue_depth\":1,",
    );

    validate_runtime_trace_jsonl(&payload_trace).expect("payload trace should validate");
    validate_runtime_trace_jsonl(VALID_BRANCH_TRACE).expect("branch trace should validate");
}

#[test]
fn event_contract_separates_typed_ids_from_metadata() {
    for kind in RuntimeTraceEventKind::ALL {
        let contract = kind.contract();

        assert_unique_contract_fields(*kind, "required", contract.required_fields());
        assert_unique_contract_fields(*kind, "typed IDs", contract.typed_id_fields());
        assert_unique_contract_fields(
            *kind,
            "optional typed IDs",
            contract.optional_typed_id_fields(),
        );
        assert_unique_contract_fields(*kind, "metadata", contract.metadata_fields());

        assert!(contract.required_fields().contains(&"event"));
        assert!(contract.required_fields().contains(&"trace_schema"));
        assert!(contract.required_fields().contains(&"trace_schema_version"));

        for field in contract.typed_id_fields() {
            assert!(
                contract.required_fields().contains(field),
                "{kind:?} typed ID field {field:?} must be required"
            );
            assert_contract_groups_disjoint(*kind, field, "typed ID", contract.metadata_fields());
        }
        for field in contract.optional_typed_id_fields() {
            assert_contract_groups_disjoint(
                *kind,
                field,
                "optional typed ID",
                contract.metadata_fields(),
            );
        }
        for field in contract.metadata_fields() {
            assert!(
                contract.required_fields().contains(field),
                "{kind:?} metadata field {field:?} must be required"
            );
            assert_contract_groups_disjoint(*kind, field, "metadata", contract.typed_id_fields());
            assert_contract_groups_disjoint(
                *kind,
                field,
                "metadata",
                contract.optional_typed_id_fields(),
            );
        }
    }
}

#[test]
fn rejects_trace_validation_byte_limit() {
    let limits = RuntimeTraceValidationLimits::new(VALID_TRACE.len() - 1, 10, 10);
    let err = validate_runtime_trace_jsonl_with_limits(VALID_TRACE, limits)
        .expect_err("oversized trace should be rejected");

    assert!(err.to_string().contains("validation byte limit"));
}

#[test]
fn rejects_trace_validation_event_limit() {
    let limits = RuntimeTraceValidationLimits::new(VALID_TRACE.len(), 2, 10);
    let err = validate_runtime_trace_jsonl_with_limits(VALID_TRACE, limits)
        .expect_err("too many trace events should be rejected");

    assert!(err.to_string().contains("validation event limit"));
}

#[test]
fn rejects_trace_validation_runtime_process_limit() {
    let second_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE
        .replace("\"process_count\":1", "\"process_count\":2")
        .replace(
            "{\"event\":\"message_accepted\"",
            &format!("{second_spawn}{{\"event\":\"message_accepted\""),
        );
    let limits = RuntimeTraceValidationLimits::new(trace.len(), 10, 1);
    let err = validate_runtime_trace_jsonl_with_limits(&trace, limits)
        .expect_err("too many runtime processes should be rejected");

    assert!(err.to_string().contains("runtime process limit"));
}

#[test]
fn rejects_schema_mismatch() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"trace_schema\":\"mantle-runtime-observability\"",
            "\"trace_schema\":\"foreign-runtime-observability\"",
        ),
        "does not match",
    );
}

#[test]
fn rejects_schema_version_mismatch() {
    assert_rejects(
        &VALID_TRACE.replace("\"trace_schema_version\":1", "\"trace_schema_version\":2"),
        "schema version 2 does not match 1",
    );
}

#[test]
fn rejects_missing_required_field() {
    assert_rejects(
        &VALID_TRACE.replace("\"process_count\":1,", ""),
        "missing field \"process_count\"",
    );
}

#[test]
fn rejects_wrong_typed_id_type() {
    assert_rejects(
        &VALID_TRACE.replace("\"pid\":1", "\"pid\":\"1\""),
        "must be an unsigned integer",
    );
}

#[test]
fn rejects_json_invalid_unsigned_integer_literals() {
    assert_rejects(
        &VALID_TRACE.replace("\"trace_schema_version\":1", "\"trace_schema_version\":01"),
        "must be a JSON unsigned integer",
    );
    assert_rejects(
        &VALID_TRACE.replace("\"pid\":1,\"process_id\":0", "\"pid\":01,\"process_id\":0"),
        "must be a JSON unsigned integer",
    );
    assert_rejects(
        &VALID_BRANCH_TRACE.replace("\"branch_path\":[0]", "\"branch_path\":[01]"),
        "must be a JSON unsigned integer",
    );
}

#[test]
fn rejects_malformed_json_event() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"event\":\"artifact_loaded\",",
            "\"event\":\"artifact_loaded\"",
        ),
        "field separator is malformed",
    );
}

#[test]
fn rejects_invalid_json_escape() {
    assert_rejects(
        &VALID_TRACE.replace("\"module\":\"hello\"", "\"module\":\"\\x\""),
        "field value is malformed",
    );
}

#[test]
fn rejects_mismatched_nested_json_container() {
    assert_rejects(
        &VALID_TRACE.replace("\"queue_depth\":1,", "\"queue_depth\":1,\"extra\":[},"),
        "field value is malformed",
    );
}

#[test]
fn rejects_duplicated_typed_id_field() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"pid\":1,\"process_id\":0",
            "\"pid\":1,\"pid\":2,\"process_id\":0",
        ),
        "field \"pid\" is duplicated",
    );
}

#[test]
fn rejects_unknown_trace_field() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"queue_depth\":1,",
            "\"queue_depth\":1,\"dispatch_key\":\"Main\",",
        ),
        "is not allowed",
    );
}

#[test]
fn rejects_partial_payload_group() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"message_id\":0,\"message\":\"Start\",\"queue_depth\":1,",
            "\"message_id\":0,\"message\":\"Start\",\"payload_type_id\":2,\"queue_depth\":1,",
        ),
        "must include both payload_type_id and payload",
    );
}

#[test]
fn rejects_partial_payload_process_ref_group() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"message_id\":0,\"message\":\"Start\",\"queue_depth\":1,",
            "\"message_id\":0,\"message\":\"Start\",\"payload_type_id\":2,\"payload\":\"type2#1\",\"payload_process_id\":2,\"queue_depth\":1,",
        ),
        "must include both payload_process_id and payload_pid",
    );
}

#[test]
fn rejects_partial_loop_context_group() {
    assert_rejects(
        &VALID_BRANCH_TRACE.replace(
            "\"branch_path\":[0],\"condition_type_id\":1",
            "\"branch_path\":[0],\"loop_element_id\":3,\"condition_type_id\":1",
        ),
        "must include both loop_element_id and loop_index",
    );
}

#[test]
fn rejects_non_numeric_branch_path_segment() {
    assert_rejects(
        &VALID_BRANCH_TRACE.replace("\"branch_path\":[0]", "\"branch_path\":[\"0\"]"),
        "must contain only unsigned integer segments",
    );
}

#[test]
fn rejects_branch_path_segment_outside_runtime_width() {
    assert_rejects(
        &VALID_BRANCH_TRACE.replace("\"branch_path\":[0]", "\"branch_path\":[65536]"),
        "does not fit into u16",
    );
}

#[test]
fn validates_renderer_branch_path_segment_domains() {
    for segment in [
        0u16, 4095, 4096, 8191, 8192, 12287, 12288, 16383, 16384, 16385,
    ] {
        let trace = VALID_BRANCH_TRACE.replace(
            "\"branch_path\":[0]",
            &format!("\"branch_path\":[{segment}]"),
        );

        validate_runtime_trace_jsonl(&trace).unwrap_or_else(|err| {
            panic!("renderer branch path segment {segment} should validate: {err}")
        });
    }
}

#[test]
fn rejects_branch_path_segment_outside_runtime_encoding() {
    for segment in [16386u16, 20480, u16::MAX] {
        let trace = VALID_BRANCH_TRACE.replace(
            "\"branch_path\":[0]",
            &format!("\"branch_path\":[{segment}]"),
        );

        assert_rejects(&trace, "outside Mantle runtime branch-path encoding");
    }
}

#[test]
fn rejects_branch_path_outside_runtime_depth() {
    let overlong_path = (0..=RUNTIME_BRANCH_PATH_CAPACITY)
        .map(|_| "0")
        .collect::<Vec<_>>()
        .join(",");
    let trace = VALID_BRANCH_TRACE.replace(
        "\"branch_path\":[0]",
        &format!("\"branch_path\":[{overlong_path}]"),
    );

    assert_rejects(&trace, "exceeds maximum");
}

#[test]
fn rejects_runtime_enum_metadata_outside_closed_domains() {
    assert_rejects(
        &VALID_BRANCH_TRACE.replace("\"branch\":\"then\"", "\"branch\":\"sideways\""),
        "value \"sideways\" is not supported",
    );
    assert_rejects(
        &VALID_STEP_TRACE.replace("\"result\":\"Stop\"", "\"result\":\"Done\""),
        "value \"Done\" is not supported",
    );
}

#[test]
fn validates_supervisor_restart_decision_child_pid_coupling() {
    validate_runtime_trace_jsonl(VALID_SUPERVISOR_RESTART_TRACE)
        .expect("restarted supervisor trace should validate");

    let denied_trace = VALID_SUPERVISOR_RESTART_TRACE
        .replace("\"decision\":\"restarted\"", "\"decision\":\"denied\"")
        .replace("\"new_child_pid\":3", "\"new_child_pid\":null");
    validate_runtime_trace_jsonl(&denied_trace)
        .expect("denied supervisor trace should keep sampled restart time");

    let not_restarted_trace = VALID_SUPERVISOR_RESTART_TRACE
        .replace(
            "\"decision\":\"restarted\"",
            "\"decision\":\"not_restarted\"",
        )
        .replace("\"restart_time_ms\":0", "\"restart_time_ms\":null")
        .replace("\"restart_window_count\":1", "\"restart_window_count\":0")
        .replace("\"new_child_pid\":3", "\"new_child_pid\":null");

    validate_runtime_trace_jsonl(&not_restarted_trace)
        .expect("not_restarted supervisor trace should null restart evidence");
}

#[test]
fn rejects_restarted_supervisor_decision_without_new_child_pid() {
    assert_rejects(
        &VALID_SUPERVISOR_RESTART_TRACE.replace("\"new_child_pid\":3", "\"new_child_pid\":null"),
        "restarted supervisor decision requires new_child_pid",
    );
}

#[test]
fn rejects_restarted_supervisor_decision_without_restart_time() {
    assert_rejects(
        &VALID_SUPERVISOR_RESTART_TRACE
            .replace("\"restart_time_ms\":0", "\"restart_time_ms\":null"),
        "restarted supervisor decision requires restart_time_ms",
    );
}

#[test]
fn rejects_non_restart_supervisor_decision_with_new_child_pid() {
    for decision in ["denied", "not_restarted"] {
        assert_rejects(
            &VALID_SUPERVISOR_RESTART_TRACE.replace(
                "\"decision\":\"restarted\"",
                &format!("\"decision\":\"{decision}\""),
            ),
            "non-restart supervisor decision must set new_child_pid to null",
        );
    }
}

#[test]
fn rejects_denied_supervisor_decision_without_restart_time() {
    let trace = VALID_SUPERVISOR_RESTART_TRACE
        .replace("\"decision\":\"restarted\"", "\"decision\":\"denied\"")
        .replace("\"restart_time_ms\":0", "\"restart_time_ms\":null")
        .replace("\"new_child_pid\":3", "\"new_child_pid\":null");

    assert_rejects(
        &trace,
        "denied supervisor decision requires restart_time_ms",
    );
}

#[test]
fn rejects_not_restarted_supervisor_decision_with_restart_time() {
    let trace = VALID_SUPERVISOR_RESTART_TRACE
        .replace(
            "\"decision\":\"restarted\"",
            "\"decision\":\"not_restarted\"",
        )
        .replace("\"new_child_pid\":3", "\"new_child_pid\":null");

    assert_rejects(
        &trace,
        "not_restarted supervisor decision must set restart_time_ms to null",
    );
}

#[test]
fn rejects_artifact_process_id_bounds_violations() {
    assert_rejects(
        &VALID_TRACE.replace("\"process_count\":1", "\"process_count\":0"),
        "process_count must be greater than zero",
    );
    assert_rejects(
        &VALID_TRACE.replace(
            "\"process_id\":0,\"process\":\"Main\"",
            "\"process_id\":1,\"process\":\"Main\"",
        ),
        "process_id 1 is outside artifact process_count 1",
    );
    let non_entry_first_spawn = VALID_TRACE
        .replace("\"entry_process_id\":0", "\"entry_process_id\":1")
        .replace("\"process_count\":1", "\"process_count\":2");
    assert_rejects(
        &non_entry_first_spawn,
        "first spawned process_id 0 must match entry_process_id 1",
    );
}

#[test]
fn rejects_artifact_typed_id_width_violations() {
    assert_rejects(
        &VALID_TRACE.replace("\"entry_message_id\":0", "\"entry_message_id\":4294967296"),
        "does not fit into u32",
    );
    assert_rejects(
        &VALID_TRACE.replace("\"process_count\":1", "\"process_count\":257"),
        "exceeds Mantle artifact process limit",
    );
}

#[test]
fn rejects_escaped_contract_field_name() {
    assert_rejects(
        &VALID_TRACE.replace("\"pid\":1", "\"p\\u0069d\":1"),
        "field name escape is unsupported",
    );
}

#[test]
fn rejects_source_name_dispatch_attempt() {
    assert_rejects(
        &VALID_TRACE.replace("\"message_accepted\"", "\"source_dispatch\""),
        "is not supported",
    );
}

#[test]
fn rejects_event_before_artifact_loaded() {
    let trace = VALID_TRACE
        .lines()
        .skip(1)
        .chain(VALID_TRACE.lines().take(1))
        .collect::<Vec<_>>()
        .join("\n");

    assert_rejects(&trace, "first runtime trace event must be artifact_loaded");
}

#[test]
fn rejects_unknown_runtime_pid_correlation() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"event\":\"message_accepted\",\"pid\":1",
            "\"event\":\"message_accepted\",\"pid\":2",
        ),
        "was not previously spawned",
    );
}

#[test]
fn rejects_runtime_pid_process_id_retargeting() {
    let trace = VALID_TRACE
        .replace("\"process_count\":1", "\"process_count\":2")
        .replace(
            "\"event\":\"message_accepted\",\"pid\":1,\"process_id\":0",
            "\"event\":\"message_accepted\",\"pid\":1,\"process_id\":1",
        );

    assert_rejects(&trace, "pid 1 is bound to process_id 0");
}

#[test]
fn rejects_non_entry_process_spawn_without_parent_pid() {
    let trace = VALID_SUPERVISOR_RESTART_TRACE.replacen(",\"spawned_by_pid\":1", "", 1);

    assert_rejects(
        &trace,
        "non-entry process_spawned event requires spawned_by_pid",
    );
}

#[test]
fn rejects_payload_pid_process_id_retargeting() {
    let trace = concat!(
        "{\"event\":\"artifact_loaded\",\"format\":\"mantle-target-artifact\",",
        "\"schema_version\":\"6\",\"source_language\":\"language-neutral\",",
        "\"module\":\"payload-ref\",\"entry_process_id\":0,\"entry_process\":\"Main\",",
        "\"entry_message_id\":0,\"process_count\":2,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"process_spawned\",\"pid\":2,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
        "{\"event\":\"message_accepted\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"message_id\":0,\"message\":\"Start\",\"payload_type_id\":2,\"payload\":\"type2#2\",",
        "\"payload_process_id\":0,\"payload_pid\":2,\"queue_depth\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );

    assert_rejects(trace, "payload_pid 2 is bound to process_id 1");
}

#[test]
fn rejects_supervisor_child_pid_process_id_retargeting() {
    assert_rejects(
        &VALID_SUPERVISOR_RESTART_TRACE.replace(
            "\"child_pid\":2,\"child_process_id\":1",
            "\"child_pid\":2,\"child_process_id\":0",
        ),
        "child_pid 2 is bound to process_id 1",
    );
}

#[test]
fn rejects_restarted_child_pid_process_id_retargeting() {
    assert_rejects(
        &VALID_SUPERVISOR_RESTART_TRACE.replace(
            "\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1",
            "\"event\":\"process_spawned\",\"pid\":3,\"process_id\":0",
        ),
        "new_child_pid 3 is bound to process_id 0",
    );
}

#[test]
fn rejects_duplicate_runtime_pid_correlation() {
    let duplicate_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE.replace(
        "{\"event\":\"message_accepted\"",
        &format!("{duplicate_spawn}{{\"event\":\"message_accepted\""),
    );

    assert_rejects(&trace, "was reused");
}

#[test]
fn rejects_non_contiguous_runtime_pid_ordering() {
    let skipped_spawn = concat!(
        "{\"event\":\"process_spawned\",\"pid\":3,\"process_id\":1,\"process\":\"Worker\",",
        "\"state_id\":0,\"state\":\"Ready\",\"mailbox_bound\":1,\"spawned_by_pid\":1,",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE
        .replace("\"process_count\":1", "\"process_count\":2")
        .replace(
            "{\"event\":\"message_accepted\"",
            &format!("{skipped_spawn}{{\"event\":\"message_accepted\""),
        );

    assert_rejects(&trace, "must be next spawned process id 2");
}

#[test]
fn rejects_label_only_pid_retargeting_attempt() {
    assert_rejects(
        &VALID_TRACE.replace(
            "\"event\":\"message_accepted\",\"pid\":1,\"process_id\":0,\"process\":\"Main\"",
            "\"event\":\"message_accepted\",\"pid\":2,\"process_id\":0,\"process\":\"Main\"",
        ),
        "was not previously spawned",
    );
}

#[test]
fn rejects_subject_event_after_process_stop() {
    let stop = concat!(
        "{\"event\":\"process_stopped\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
        "\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE.replace(
        "{\"event\":\"message_accepted\"",
        &format!("{stop}{{\"event\":\"message_accepted\""),
    );

    assert_rejects(&trace, "after process termination");
}

#[test]
fn rejects_subject_event_after_process_failure() {
    let failure = concat!(
        "{\"event\":\"process_failed\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"state_id\":0,\"state\":\"Ready\",\"reason\":\"panic\",",
        "\"trace_schema\":\"mantle-runtime-observability\",\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE.replace(
        "{\"event\":\"message_accepted\"",
        &format!("{failure}{{\"event\":\"message_accepted\""),
    );

    assert_rejects(&trace, "after process termination");
}

#[test]
fn rejects_double_process_terminal_events() {
    let stop = concat!(
        "{\"event\":\"process_stopped\",\"pid\":1,\"process_id\":0,\"process\":\"Main\",",
        "\"reason\":\"normal\",\"trace_schema\":\"mantle-runtime-observability\",",
        "\"trace_schema_version\":1}\n",
    );
    let trace = VALID_TRACE.replace(
        "{\"event\":\"message_accepted\"",
        &format!("{stop}{stop}{{\"event\":\"message_accepted\""),
    );

    assert_rejects(&trace, "already terminated");
}

fn assert_rejects(trace: &str, expected: &str) {
    let err = validate_runtime_trace_jsonl(trace).expect_err("trace should be rejected");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn assert_unique_contract_fields(
    kind: RuntimeTraceEventKind,
    group: &str,
    fields: &[&'static str],
) {
    for (index, field) in fields.iter().enumerate() {
        assert!(
            !fields[..index].contains(field),
            "{kind:?} {group} field {field:?} is duplicated"
        );
    }
}

fn assert_contract_groups_disjoint(
    kind: RuntimeTraceEventKind,
    field: &str,
    group: &str,
    other_fields: &[&'static str],
) {
    assert!(
        !other_fields.contains(&field),
        "{kind:?} {group} field {field:?} overlaps metadata/typed ID fields"
    );
}
