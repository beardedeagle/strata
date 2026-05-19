use super::super::support::*;
use super::shared::*;

#[test]
fn match_msg_dispatches_same_message_by_disjoint_fieldless_nested_predicates() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Done)) => {
                return Stop(state);
            }
        }
}
"#,
    );

    let checked = check_source(&source)
        .expect("match msg same-message predicate split should check when guards are disjoint");
    let artifact =
        lower_to_artifact(&checked, &source).expect("payload-specific match msg should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");

    assert_eq!(worker.transitions.len(), 2);
    assert!(
        worker
            .transitions
            .iter()
            .all(|transition| transition.message == MessageId::new(0)
                && transition.payload_guard.is_some()),
        "same-message match msg split should lower exact typed payload guards"
    );
    let mut payload_guard_labels = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .label()
        })
        .collect::<Vec<_>>();
    payload_guard_labels.sort();
    assert_eq!(payload_guard_labels, ["Assign(Done)", "Assign(Ready)"]);
    let step_result_for = |label: &str| {
        worker
            .transitions
            .iter()
            .find(|transition| {
                transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.label() == label)
            })
            .expect("payload guard transition should exist")
            .step_result
    };
    assert_eq!(step_result_for("Assign(Ready)"), StepResult::Continue);
    assert_eq!(step_result_for("Assign(Done)"), StepResult::Stop);

    let encoded = artifact.encode();
    assert!(encoded.contains(".payload_guard_type_id="));
    assert!(encoded.contains(".payload_guard_value=Assign(Ready)"));
    assert!(encoded.contains(".payload_guard_value=Assign(Done)"));
    assert!(
        !encoded.contains("field_name=Assign"),
        "payload guards must lower as typed values, not source-field selectors"
    );
}

#[test]
fn match_msg_same_message_payload_split_uses_wildcard_for_uncovered_discovered_payload() {
    let source = same_message_step_split_case_with_other(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            _ => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let checked = check_source(&source)
        .expect("match msg wildcard should cover discovered same-message guarded misses");
    let artifact =
        lower_to_artifact(&checked, &source).expect("payload-specific wildcard match should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let mut labels = worker
        .transitions
        .iter()
        .map(|transition| {
            transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard")
                .label()
        })
        .collect::<Vec<_>>();
    labels.sort();

    assert_eq!(labels, ["Assign(Other)", "Assign(Ready)"]);
    let step_result_for = |label: &str| {
        worker
            .transitions
            .iter()
            .find(|transition| {
                transition
                    .payload_guard
                    .as_ref()
                    .is_some_and(|guard| guard.label() == label)
            })
            .expect("payload guard transition should exist")
            .step_result
    };
    assert_eq!(step_result_for("Assign(Ready)"), StepResult::Continue);
    assert_eq!(step_result_for("Assign(Other)"), StepResult::Stop);
}

#[test]
fn rejects_unreachable_wildcard_after_payload_sensitive_match_msg_covers_discovered_payloads() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Done)) => {
                return Stop(state);
            }
            _ => {
                return Panic(state);
            }
        }
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("wildcard after complete payload-sensitive coverage should fail");
    assert!(
        err.to_string()
            .contains("process Worker wildcard step pattern is unreachable"),
        "expected payload-sensitive wildcard reachability diagnostic, got {err}"
    );
}

#[test]
fn rejects_unreachable_payload_sensitive_match_msg_arm_before_dropping_body() {
    let source = same_message_step_split_case_with_other(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Other)) => {
                return Stop(state);
            }
            Envelope(Assign(Done)) => {
                emit "unreachable guarded payload arm";
                return Panic(state);
            }
        }
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("unreachable guarded match msg payload arm should fail closed");
    assert!(
        err.to_string().contains(
            "process Worker match msg pattern Envelope(Assign(Done)) has no discovered payload case"
        ),
        "expected unreachable guarded payload arm diagnostic, got {err}"
    );
}

#[test]
fn match_msg_same_message_split_preserves_nested_record_list_and_map_bindings() {
    let source = r#"
module same_message_match_msg_nested_bindings;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done, Other }
enum Routed {
    AssignJob(Job),
    Hold(List<Job,2>),
    Lookup(Map<Phase,Job,2>),
}
enum WorkerState {
    Idle,
    Seen(Phase),
    Held(List<Job,1>),
    Looked(Map<Phase,Job,1>),
}
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed), Finish }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(AssignJob(Job { phase: Ready }));
        send worker Envelope(Hold(List<Job,2>[Job { phase: Done }, Job { phase: Other }]));
        send worker Envelope(Lookup(Map<Phase,Job,2>[
            Ready => Job { phase: Done },
            Other => Job { phase: Ready },
        ]));
        send worker Finish;
        return Stop(state);
    }
}

proc Worker mailbox bounded(4) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match msg {
            Envelope(AssignJob(Job { phase })) => {
                return Continue(Seen(phase));
            }
            Envelope(Hold(List[Job { phase }, ..tail])) => {
                return Continue(Held(tail));
            }
            Envelope(Lookup(Map[Ready => Job { phase }, ..rest])) => {
                return Continue(Looked(rest));
            }
            Finish => {
                return Stop(state);
            }
        }
    }
}
"#;

    let checked =
        check_source(source).expect("same-message match msg nested binding split should check");
    let artifact = lower_to_artifact(&checked, source)
        .expect("same-message nested binding split should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");

    assert_eq!(worker.transitions.len(), 4);
    assert_eq!(
        worker
            .transitions
            .iter()
            .filter(|transition| transition.message == MessageId::new(0)
                && transition.payload_guard.is_some())
            .count(),
        3
    );
    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=record_field"));
    assert!(encoded.contains(".kind=list_rest"));
    assert!(encoded.contains(".kind=map_rest"));
    assert!(
        !encoded.contains("field_name=AssignJob"),
        "same-message split must not lower constructor labels as executable fields"
    );
}

#[test]
fn rejects_duplicate_match_msg_same_message_nested_predicate() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Ready)) => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err = check_source(&source).expect_err("duplicate match msg nested predicate should fail");
    assert!(
        err.to_string()
            .contains("process Worker match msg pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"),
        "expected duplicate same-message match diagnostic, got {err}"
    );
}

#[test]
fn rejects_guarded_and_unguarded_match_msg_same_message_overlap() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err =
        check_source(&source).expect_err("guarded and unguarded match msg overlap should fail");
    assert!(
        err.to_string()
            .contains("process Worker match msg pattern Envelope overlaps an earlier pattern for message Envelope"),
        "expected guarded/unguarded same-message match diagnostic, got {err}"
    );
}

#[test]
fn rejects_match_msg_same_message_predicates_that_are_not_provably_disjoint() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(phase: Phase)) => {
                return Continue(state);
            }
            Envelope(Assign(Ready)) => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err =
        check_source(&source).expect_err("not-provably-disjoint match msg predicates should fail");
    assert!(
        err.to_string()
            .contains("process Worker match msg pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"),
        "expected not-provably-disjoint same-message match diagnostic, got {err}"
    );
}

#[test]
fn rejects_match_msg_same_message_split_with_missing_discovered_payload_coverage() {
    let source = same_message_step_split_case_with_other(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Done)) => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err =
        check_source(&source).expect_err("uncovered discovered same-message payload should fail");
    assert!(
        err.to_string().contains(
            "process Worker must declare step pattern for message Envelope payload Assign(Other)"
        ),
        "expected uncovered same-message payload diagnostic, got {err}"
    );
}

#[test]
fn rejects_match_msg_payload_split_without_discovered_payload_case() {
    let source = same_message_step_split_without_discovered_payload_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            Envelope(Assign(Done)) => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err =
        check_source(&source).expect_err("payload split without a discovered payload should fail");
    assert!(
        err.to_string()
            .contains("process Worker must declare step pattern for message Envelope"),
        "expected missing concrete payload coverage diagnostic, got {err}"
    );
}

#[test]
fn rejects_step_signature_payload_split_wildcard_without_discovered_payload_case() {
    let source = same_message_step_split_without_discovered_payload_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, _) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let err = check_source(&source).expect_err(
        "step-signature payload split wildcard without a discovered payload should fail closed",
    );
    assert!(
        err.to_string().contains("process Worker payload-sensitive step pattern for message Envelope has no discovered payload case for wildcard fallback"),
        "expected missing concrete payload step wildcard diagnostic, got {err}"
    );
}

#[test]
fn rejects_match_msg_payload_split_wildcard_without_discovered_payload_case() {
    let source = same_message_step_split_without_discovered_payload_case(
        r#"
    fn step(state: MainState, msg: WorkerMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        match msg {
            Envelope(Assign(Ready)) => {
                return Continue(state);
            }
            _ => {
                return Stop(state);
            }
        }
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("payload split wildcard without a discovered payload should fail closed");
    assert!(
        err.to_string().contains("process Worker payload-sensitive match msg pattern for message Envelope has no discovered payload case for wildcard fallback"),
        "expected missing concrete payload wildcard diagnostic, got {err}"
    );
}
