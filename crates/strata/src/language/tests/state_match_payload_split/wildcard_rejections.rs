use super::*;

#[test]
fn rejects_unreachable_state_match_payload_wildcard_when_explicit_cases_cover_discovered_payloads()
{
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let fallback_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {fallback_body}
"#
    ));

    let err = check_source(&source)
        .expect_err("fully covered state-match wildcard should be unreachable");
    assert!(
        err.to_string()
            .contains("process Worker wildcard step pattern is unreachable"),
        "expected unreachable wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_non_state_match_wildcard_after_fully_covered_state_match_payload_split() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_with_unit_message(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(Done);
    }}
"#
    ));

    let err = check_source(&source)
        .expect_err("non-state-match wildcard after state-match payload split should fail");
    assert!(
        err.to_string().contains(
            "process Worker declares a wildcard step pattern with a payload-sensitive state match step pattern for message Envelope"
        ),
        "expected mixed state-match/non-state-match wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_non_state_match_wildcard_before_fully_covered_state_match_payload_split() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_with_unit_message(&format!(
        r#"
    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(Done);
    }}

    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err = check_source(&source)
        .expect_err("non-state-match wildcard before state-match payload split should fail");
    assert!(
        err.to_string().contains(
            "process Worker declares a wildcard step pattern with a payload-sensitive state match step pattern for message Envelope"
        ),
        "expected mixed state-match/non-state-match wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_state_match_payload_wildcard_without_discovered_payload_case() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let fallback_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_without_discovered_payload_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {fallback_body}
"#
    ));

    let err = check_source(&source)
        .expect_err("state-match wildcard without a discovered payload should fail closed");
    assert!(
        err.to_string().contains("process Worker payload-sensitive state match step pattern for message Envelope has no discovered payload case for wildcard fallback"),
        "expected missing concrete payload state-match wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_block_wildcard_fallback_for_state_match_payload_split() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(Done);
    }}
"#
    ));

    let err = check_source(&source)
        .expect_err("block wildcard fallback for state-match payload split should fail");
    assert!(
        err.to_string().contains(
            "process Worker declares a wildcard step pattern with a payload-sensitive state match step pattern for message Envelope"
        ),
        "expected state-match block wildcard fallback diagnostic, got {err}"
    );
}

#[test]
fn rejects_unreachable_payload_sensitive_state_match_clause_before_dropping_body() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let other_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case_with_other(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Other))) -> ProcResult<WorkerState> ! [] ~ [] @det {other_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err =
        check_source(&source).expect_err("unreachable state-match guarded payload should fail");
    assert!(
        err.to_string().contains(
            "process Worker step pattern Envelope(Assign(Done)) has no discovered payload case"
        ),
        "expected unreachable state-match guarded payload diagnostic, got {err}"
    );
}
