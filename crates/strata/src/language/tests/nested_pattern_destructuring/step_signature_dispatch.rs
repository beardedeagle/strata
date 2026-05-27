use super::super::support::*;
use super::shared::*;

#[test]
fn step_signature_dispatches_same_message_by_disjoint_fieldless_nested_predicates() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Done))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let checked = check_source(&source).expect(
        "step signature same-message predicate split should check when guards are disjoint",
    );
    let artifact = lower_to_artifact(&checked, &source)
        .expect("payload-specific step signatures should lower");
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
        "same-message step signature split should lower exact typed payload guards"
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
fn step_signature_payload_predicate_uses_wildcard_for_uncovered_discovered_payload() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, _) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let checked = check_source(&source)
        .expect("step signature wildcard should cover discovered same-message guarded misses");
    let artifact = lower_to_artifact(&checked, &source)
        .expect("payload-specific signature wildcard should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let routed_type = artifact_type_id(&artifact, "Routed");
    let mut labels = worker
        .transitions
        .iter()
        .map(|transition| {
            let guard = transition
                .payload_guard
                .as_ref()
                .expect("transition should carry a payload guard");
            assert_eq!(guard.ty, routed_type);
            guard.label()
        })
        .collect::<Vec<_>>();
    labels.sort();

    assert_eq!(labels, ["Assign(Done)", "Assign(Ready)"]);
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
        "payload-specific signature wildcard must not lower constructor names as executable fields"
    );
}

#[test]
fn step_signature_payload_wildcard_keeps_ordinary_variant_fallback_unguarded() {
    let source = r#"
module step_signature_payload_wildcard_mixed_fallback;

record MainState;
enum Phase { Ready, Done }
enum Routed { Assign(Phase) }
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed), Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, _) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source)
        .expect("signature wildcard should still cover non-payload-sensitive variants");
    let artifact = lower_to_artifact(&checked, source)
        .expect("mixed signature wildcard fallback should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let routed_type = artifact_type_id(&artifact, "Routed");

    assert_eq!(worker.transitions.len(), 2);
    assert!(
        worker.transitions.iter().any(|transition| {
            transition.message == MessageId::new(0)
                && transition.payload_guard.as_ref().is_some_and(|guard| {
                    guard.ty == routed_type && guard.label() == "Assign(Ready)"
                })
        }),
        "discovered Envelope(Assign(Ready)) should lower as an exact typed payload guard"
    );
    assert!(
        worker
            .transitions
            .iter()
            .any(|transition| transition.message == MessageId::new(1)
                && transition.payload_guard.is_none()),
        "wildcard should remain an ordinary unguarded fallback for Ping"
    );
    assert!(
        worker.transitions.iter().all(|transition| {
            transition.payload_guard.is_none()
                || transition.payload_guard.as_ref().is_some_and(|guard| {
                    guard.ty == routed_type && guard.label() == "Assign(Ready)"
                })
        }),
        "wildcard must not create an open-ended payload catch-all transition"
    );
}

#[test]
fn step_signature_same_message_split_preserves_nested_record_list_and_map_bindings() {
    let source = r#"
module same_message_step_signature_nested_bindings;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    fn step(state: WorkerState, Envelope(AssignJob(Job { phase }))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Seen(phase));
    }

    fn step(state: WorkerState, Envelope(Hold(List[Job { phase }, ..tail]))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Held(tail));
    }

    fn step(state: WorkerState, Envelope(Lookup(Map[Ready => Job { phase }, ..rest]))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Looked(rest));
    }

    fn step(state: WorkerState, Finish) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source)
        .expect("same-message step signature nested binding split should check");
    let artifact = lower_to_artifact(&checked, source)
        .expect("same-message step signature nested binding split should lower");
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
        "same-message signature split must not lower constructor labels as executable fields"
    );
}

#[test]
fn rejects_duplicate_step_signature_same_message_nested_predicate() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let err =
        check_source(&source).expect_err("duplicate step signature nested predicate should fail");
    assert!(
        err.to_string()
            .contains("process Worker step pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"),
        "expected duplicate same-message step signature diagnostic, got {err}"
    );
}

#[test]
fn rejects_guarded_and_unguarded_step_signature_same_message_overlap() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(route: Routed)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("guarded and unguarded step signature overlap should fail");
    assert!(
        err.to_string().contains(
            "process Worker step pattern Envelope overlaps an earlier pattern for message Envelope"
        ),
        "expected guarded/unguarded step signature diagnostic, got {err}"
    );
}

#[test]
fn rejects_step_signature_same_message_predicates_that_are_not_provably_disjoint() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(phase: Phase))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("not-provably-disjoint step signature predicates should fail");
    assert!(
        err.to_string()
            .contains("process Worker step pattern Envelope(Assign(Ready)) overlaps an earlier pattern for message Envelope"),
        "expected not-provably-disjoint step signature diagnostic, got {err}"
    );
}

#[test]
fn rejects_step_signature_same_message_split_with_missing_discovered_payload_coverage() {
    let source = same_message_step_split_case_with_other(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Done))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
"#,
    );

    let err =
        check_source(&source).expect_err("uncovered discovered same-message payload should fail");
    assert!(
        err.to_string().contains(
            "process Worker must declare step pattern for message Envelope payload Assign(Other)"
        ),
        "expected uncovered same-message step signature diagnostic, got {err}"
    );
}

#[test]
fn rejects_unreachable_step_signature_payload_wildcard() {
    let source = same_message_step_split_case(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Done))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, _) -> ProcResult<MainState> ! [] ~ [] @det {
        return Panic(state);
    }
"#,
    );

    let err = check_source(&source)
        .expect_err("wildcard after complete step-signature payload coverage should fail");
    assert!(
        err.to_string()
            .contains("process Worker wildcard step pattern is unreachable"),
        "expected step-signature wildcard reachability diagnostic, got {err}"
    );
}

#[test]
fn rejects_unreachable_payload_sensitive_step_signature_before_dropping_body() {
    let source = same_message_step_split_case_with_other(
        r#"
    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: MainState, Envelope(Assign(Other))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: MainState, Envelope(Assign(Done))) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "unreachable guarded payload step";
        return Panic(state);
    }
"#,
    );

    let err =
        check_source(&source).expect_err("unreachable guarded step signature should fail closed");
    assert!(
        err.to_string().contains(
            "process Worker step pattern Envelope(Assign(Done)) has no discovered payload case"
        ),
        "expected unreachable guarded step signature diagnostic, got {err}"
    );
}
