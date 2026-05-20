use super::*;

#[test]
fn rejects_duplicate_state_match_same_message_nested_predicate() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err =
        check_source(&source).expect_err("duplicate state-match nested predicate should fail");
    assert!(
        err.to_string().contains(
            "process Worker state match step pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"
        ),
        "expected duplicate state-match same-message diagnostic, got {err}"
    );
}

#[test]
fn rejects_guarded_and_unguarded_state_match_same_message_overlap() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(route: Routed)) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err =
        check_source(&source).expect_err("guarded and unguarded state-match overlap should fail");
    assert!(
        err.to_string().contains(
            "process Worker state match step pattern Envelope overlaps an earlier pattern for message Envelope"
        ),
        "expected guarded/unguarded state-match diagnostic, got {err}"
    );
}

#[test]
fn rejects_state_match_same_message_predicates_that_are_not_provably_disjoint() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err = check_source(&source)
        .expect_err("not-provably-disjoint state-match predicates should fail");
    assert!(
        err.to_string().contains(
            "process Worker state match step pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"
        ),
        "expected not-provably-disjoint state-match diagnostic, got {err}"
    );
}

#[test]
fn rejects_state_match_same_message_split_with_missing_discovered_payload_coverage() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case_with_other(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err =
        check_source(&source).expect_err("uncovered discovered state-match payload should fail");
    assert!(
        err.to_string().contains(
            "process Worker must declare step pattern for message Envelope payload Assign(Other)"
        ),
        "expected uncovered state-match same-message diagnostic, got {err}"
    );
}
