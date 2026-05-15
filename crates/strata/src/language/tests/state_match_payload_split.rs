use super::support::*;

fn state_match_payload_split_case(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_payload_split_case_with_other(worker_steps: &str) -> String {
    format!(
        r#"
module state_match_payload_split_case_with_other;

record MainState;
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Other));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

{worker_steps}
}}
"#
    )
}

fn state_match_body_for(result: &str) -> String {
    format!(
        r#"{{
        match state {{
            Idle => {{
                return {result};
            }}
            SawReady => {{
                return {result};
            }}
            Done => {{
                return Stop(Done);
            }}
        }}
    }}"#
    )
}

#[test]
fn state_match_dispatches_same_message_by_disjoint_fieldless_nested_predicates() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, Envelope(Assign(Done))) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let checked = check_source(&source)
        .expect("state-match same-message predicate split should check when guards are disjoint");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(checked_state_labels(worker), ["Idle", "SawReady", "Done"]);
    assert_eq!(worker.transitions().len(), 6);
    assert!(worker.transitions().iter().all(|transition| {
        transition.message() == checked_message_id(0)
            && transition.current_state().is_some()
            && transition.payload_guard().is_some()
    }));

    let artifact = lower_to_artifact(&checked, &source)
        .expect("payload-specific state-match steps should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let routed_type = artifact_type_id(&artifact, "Routed");

    assert_eq!(artifact_state_labels(worker), ["Idle", "SawReady", "Done"]);
    assert_eq!(worker.transitions.len(), 6);
    assert!(worker.transitions.iter().all(|transition| {
        transition.message == MessageId::new(0)
            && transition.current_state.is_some()
            && transition
                .payload_guard
                .as_ref()
                .is_some_and(|guard| guard.ty == routed_type)
    }));

    let mut keys = worker
        .transitions
        .iter()
        .map(|transition| {
            let current_state = transition
                .current_state
                .expect("state-match transition should carry current state");
            let state_label = worker.state_values[current_state.index()].label.clone();
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("state-match transition should carry payload guard");
            let result_label = match transition.step_result {
                StepResult::Continue => "Continue",
                StepResult::Stop => "Stop",
                StepResult::Panic => "Panic",
            };
            (state_label, guard.label(), result_label)
        })
        .collect::<Vec<_>>();
    keys.sort();

    assert_eq!(
        keys,
        [
            ("Done".to_string(), "Assign(Done)".to_string(), "Stop"),
            ("Done".to_string(), "Assign(Ready)".to_string(), "Stop"),
            ("Idle".to_string(), "Assign(Done)".to_string(), "Stop"),
            ("Idle".to_string(), "Assign(Ready)".to_string(), "Continue"),
            ("SawReady".to_string(), "Assign(Done)".to_string(), "Stop"),
            (
                "SawReady".to_string(),
                "Assign(Ready)".to_string(),
                "Continue"
            ),
        ]
    );

    let encoded = artifact.encode();
    assert!(encoded.contains(".current_state="));
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload-specific state-match dispatch must not lower constructor labels as executable fields"
    );
}

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

#[test]
fn rejects_state_match_payload_split_wildcard_fallback() {
    let ready_body = state_match_body_for("Continue(SawReady)");
    let done_body = state_match_body_for("Stop(Done)");
    let source = state_match_payload_split_case(&format!(
        r#"
    fn step(state: WorkerState, Envelope(Assign(Ready))) -> ProcResult<WorkerState> ! [] ~ [] @det {ready_body}

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {done_body}
"#
    ));

    let err =
        check_source(&source).expect_err("state-match wildcard fallback should remain rejected");
    assert!(
        err.to_string().contains(
            "process Worker declares a wildcard step pattern with a payload-sensitive step pattern"
        ),
        "expected state-match wildcard fallback diagnostic, got {err}"
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
