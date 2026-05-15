use super::support::*;

fn nested_worker_pattern_source(pattern: &str) -> String {
    format!(
        r#"
module nested_pattern_rejection;

record MainState;
record Job {{ phase: Phase }}
enum Phase {{ Ready, Done, Other }}
enum Routed {{
    Assign(Job),
    Hold(List<Job,1>),
    Lookup(Map<Phase,Job,1>),
}}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, {pattern}) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn assert_nested_worker_pattern_rejected(pattern: &str, expected: &str) {
    let source = nested_worker_pattern_source(pattern);
    let err = check_source(&source).expect_err("nested worker pattern should fail");
    assert!(
        err.to_string().contains(expected),
        "expected diagnostic containing {expected:?}, got {err}"
    );
}

#[test]
fn step_patterns_bind_nested_constructor_record_list_and_map_values() {
    let source = r#"
module nested_runtime_patterns;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done, Other }
enum Routed {
    Assign(Job),
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
enum WorkerMsg {
    AssignEnvelope(Routed),
    HoldEnvelope(Routed),
    LookupEnvelope(Routed),
    Finish,
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker AssignEnvelope(Assign(Job { phase: Ready }));
        send worker HoldEnvelope(Hold(List<Job,2>[Job { phase: Done }, Job { phase: Other }]));
        send worker LookupEnvelope(Lookup(Map<Phase,Job,2>[
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

    fn step(state: WorkerState, AssignEnvelope(Assign(Job { phase }))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Seen(phase));
    }

    fn step(state: WorkerState, HoldEnvelope(Hold(List[Job { phase }, ..tail]))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Held(tail));
    }

    fn step(state: WorkerState, LookupEnvelope(Lookup(Map[Ready => Job { phase }, ..rest]))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Looked(rest));
    }

    fn step(state: WorkerState, Finish) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("nested step payload patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("nested patterns should lower");
    let encoded = artifact.encode();

    assert!(
        encoded.contains(".kind=enum_payload"),
        "lowering should emit typed enum payload projection templates"
    );
    assert!(
        !encoded.contains("field_name=Assign"),
        "constructor names must not be lowered as record-field executable references"
    );
}

#[test]
fn step_patterns_accept_fieldless_nested_enum_constructor_payloads() {
    let source = r#"
module fieldless_nested_constructor_payload;

record MainState;
enum JobKind { Ready, Done }
enum Routed { Assign(JobKind) }
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked =
        check_source(source).expect("fieldless nested enum constructor pattern should check");
    lower_to_artifact(&checked, source)
        .expect("fieldless nested enum constructor pattern should lower");
}

#[test]
fn step_patterns_reject_fieldless_nested_enum_constructor_payload_mismatch() {
    let source = r#"
module fieldless_nested_constructor_payload_mismatch;

record MainState;
enum JobKind { Ready, Done }
enum Routed { Assign(JobKind) }
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Done));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Assign(Ready))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err =
        check_source(source).expect_err("nonmatching fieldless nested enum payload should fail");

    assert!(
        err.to_string().contains(
            "process Worker must declare step pattern for message Envelope payload Assign(Done)"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn match_msg_binds_nested_constructor_record_list_and_map_values() {
    let source = r#"
module nested_match_msg_patterns;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done, Other }
enum Routed {
    Assign(Job),
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
enum WorkerMsg {
    AssignEnvelope(Routed),
    HoldEnvelope(Routed),
    LookupEnvelope(Routed),
    Finish,
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker AssignEnvelope(Assign(Job { phase: Ready }));
        send worker HoldEnvelope(Hold(List<Job,2>[Job { phase: Done }, Job { phase: Other }]));
        send worker LookupEnvelope(Lookup(Map<Phase,Job,2>[
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
            AssignEnvelope(Assign(Job { phase })) => {
                return Continue(Seen(phase));
            }
            HoldEnvelope(Hold(List[Job { phase }, ..tail])) => {
                return Continue(Held(tail));
            }
            LookupEnvelope(Lookup(Map[Ready => Job { phase }, ..rest])) => {
                return Continue(Looked(rest));
            }
            Finish => {
                return Stop(state);
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("nested match-msg payload patterns should check");
    let artifact =
        lower_to_artifact(&checked, source).expect("nested match-msg patterns should lower");
    let worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker process should lower");
    let encoded = artifact.encode();

    assert_eq!(worker.transitions.len(), 4);
    assert!(
        encoded.contains(".kind=enum_payload"),
        "match-msg lowering should emit typed enum payload projection templates"
    );
    assert!(
        encoded.contains(".variant_id="),
        "match-msg lowering should emit typed enum variant ids"
    );
    assert!(
        !encoded.contains("field_name=Assign"),
        "constructor names must not be lowered as record-field executable references"
    );
}

#[test]
fn state_match_binds_nested_constructor_record_payload() {
    let source = r#"
module nested_state_match_pattern;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Assign(Job) }
enum WorkerState {
    Holding(Routed),
    Done(Phase),
}
enum MainMsg { Start }
enum WorkerMsg { Advance }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Advance;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Assign(Job { phase: Ready }));
    }

    fn step(state: WorkerState, Advance) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(Assign(Job { phase })) => {
                return Stop(Done(phase));
            }
            Done(phase: Phase) => {
                return Stop(Done(phase));
            }
        }
    }
}
"#;

    check_source(source).expect("nested state payload pattern should check");
}

#[test]
fn state_match_accepts_fieldless_nested_enum_constructor_payload() {
    let source = r#"
module fieldless_nested_state_match_pattern;

record MainState;
enum Phase { Ready, Done }
enum Routed { Assign(Phase) }
enum WorkerState { Holding(Routed) }
enum MainMsg { Start }
enum WorkerMsg { Advance }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Advance;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Assign(Ready));
    }

    fn step(state: WorkerState, Advance) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(Assign(Ready)) => {
                return Stop(state);
            }
        }
    }
}
"#;

    check_source(source).expect("fieldless nested state payload pattern should check");
}

#[test]
fn state_match_rejects_fieldless_nested_enum_constructor_payload_mismatch() {
    let source = r#"
module fieldless_nested_state_match_pattern_mismatch;

record MainState;
enum Phase { Ready, Done }
enum Routed { Assign(Phase) }
enum WorkerState { Holding(Routed) }
enum MainMsg { Start }
enum WorkerMsg { Advance }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Advance;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Assign(Done));
    }

    fn step(state: WorkerState, Advance) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(Assign(Ready)) => {
                return Stop(state);
            }
        }
    }
}
"#;

    let err = check_source(source)
        .expect_err("nonmatching fieldless nested enum state payload should fail");

    assert!(
        err.to_string().contains(
            "process Worker state match pattern Holding does not match discovered payload Assign(Done)"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn source_helpers_bind_nested_patterns_in_signature_body_and_return_match() {
    let source = r#"
module nested_helper_patterns;

record MainState {
    signature: Phase,
    body: Phase,
    ret: Phase,
    list: Phase,
    fieldless_signature: Phase,
    fieldless_body: Phase,
    fieldless_ret: Phase,
}
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Assign(Job) }
enum RoutedKind { Mark(Phase) }
enum MainMsg { Start }

fn phase_signature(Assign(Job { phase })) -> Phase ! [] ~ [] @det {
    return phase;
}

fn phase_body(route: Routed) -> Phase ! [] ~ [] @det {
    match route {
        Assign(Job { phase }) => {
            return phase;
        }
    }
}

fn phase_return(route: Routed) -> Phase ! [] ~ [] @det {
    return match route {
        Assign(Job { phase }) => {
            return phase;
        }
    };
}

fn phase_list(List<Routed,1>[Assign(Job { phase })]) -> Phase ! [] ~ [] @det {
    return phase;
}

fn fieldless_signature(Mark(Ready)) -> Phase ! [] ~ [] @det {
    return Ready;
}

fn fieldless_body(route: RoutedKind) -> Phase ! [] ~ [] @det {
    match route {
        Mark(Ready) => {
            return Ready;
        }
    }
}

fn fieldless_return(route: RoutedKind) -> Phase ! [] ~ [] @det {
    return match route {
        Mark(Ready) => {
            return Ready;
        }
    };
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: phase_signature(Assign(Job { phase: Ready })),
            body: phase_body(Assign(Job { phase: Done })),
            ret: phase_return(Assign(Job { phase: Ready })),
            list: phase_list(List<Routed,1>[Assign(Job { phase: Done })]),
            fieldless_signature: fieldless_signature(Mark(Ready)),
            fieldless_body: fieldless_body(Mark(Ready)),
            fieldless_ret: fieldless_return(Mark(Ready)),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    check_source(source).expect("nested source helper patterns should check");
}

#[test]
fn source_helpers_dispatch_same_constructor_by_disjoint_fieldless_nested_predicates() {
    let source = r#"
module payload_sensitive_helper_dispatch;

record MainState {
    body_ready: Phase,
    body_done: Phase,
    body_fallback: Phase,
    return_ready: Phase,
    return_done: Phase,
    return_fallback: Phase,
    bound_assign: Phase,
    bound_hold: Phase,
}
record Job { phase: Phase }
enum Phase { Ready, Done, Other }
enum Routed {
    Assign(Phase),
    AssignJob(Job),
    Hold(List<Job,2>),
}
enum Packet { Envelope(Routed) }
enum MainMsg { Start }

fn route_body(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    }
}

fn route_return(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    };
}

fn route_body_with_fallback(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        _ => {
            return Other;
        }
    }
}

fn route_return_with_fallback(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        _ => {
            return Other;
        }
    };
}

fn route_bound(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(AssignJob(Job { phase })) => {
            return phase;
        }
        Envelope(Hold(List[Job { phase }, ..tail])) => {
            return phase;
        }
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            body_ready: route_body(Envelope(Assign(Ready))),
            body_done: route_body(Envelope(Assign(Done))),
            body_fallback: route_body_with_fallback(Envelope(Assign(Done))),
            return_ready: route_return(Envelope(Assign(Ready))),
            return_done: route_return(Envelope(Assign(Done))),
            return_fallback: route_return_with_fallback(Envelope(Assign(Done))),
            bound_assign: route_bound(Envelope(AssignJob(Job { phase: Ready }))),
            bound_hold: route_bound(Envelope(Hold(List<Job,2>[
                Job { phase: Done },
                Job { phase: Other },
            ]))),
        };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("payload-sensitive helper dispatch should check");
    lower_to_artifact(&checked, source).expect("payload-sensitive helper dispatch should lower");
}

fn payload_sensitive_helper_case(route: &str, init_value: &str) -> String {
    format!(
        r#"
module payload_sensitive_helper_case;

record MainState {{ phase: Phase }}
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum Packet {{ Envelope(Routed) }}
enum MainMsg {{ Start }}

{route}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return {init_value};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

#[test]
fn rejects_duplicate_payload_sensitive_helper_predicate() {
    let source = payload_sensitive_helper_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Ready)) => {
            return Ready;
        }
    }
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("duplicate nested predicate should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope(Assign(Ready)) overlaps an earlier pattern"),
        "expected duplicate nested predicate diagnostic, got {err}"
    );
}

#[test]
fn rejects_guarded_and_unguarded_helper_constructor_overlap() {
    let source = payload_sensitive_helper_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    return match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope => {
            return Done;
        }
    };
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("guarded and unguarded overlap should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope overlaps an earlier pattern"),
        "expected guarded/unguarded overlap diagnostic, got {err}"
    );
}

#[test]
fn rejects_payload_sensitive_helper_predicates_that_are_not_provably_disjoint() {
    let source = payload_sensitive_helper_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(phase: Phase)) => {
            return phase;
        }
        Envelope(Assign(Ready)) => {
            return Ready;
        }
    }
}
"#,
        "MainState { phase: Ready }",
    );

    let err = check_source(&source).expect_err("unproven predicate disjointness should fail");
    assert!(
        err.to_string()
            .contains("pattern Envelope(Assign(Ready)) overlaps an earlier pattern"),
        "expected unproven overlap diagnostic, got {err}"
    );
}

#[test]
fn rejects_uncovered_payload_sensitive_helper_predicate_at_expansion_time() {
    let source = payload_sensitive_helper_case(
        r#"
fn route(packet: Packet) -> Phase ! [] ~ [] @det {
    match packet {
        Envelope(Assign(Ready)) => {
            return Ready;
        }
        Envelope(Assign(Done)) => {
            return Done;
        }
    }
}
"#,
        "MainState { phase: route(Envelope(Assign(Other))) }",
    );

    let err = check_source(&source).expect_err("uncovered nested predicate should fail");
    assert!(
        err.to_string()
            .contains("function route match has no matching pattern for Envelope(Assign(Other))"),
        "expected uncovered nested predicate diagnostic, got {err}"
    );
}

fn same_message_step_split_case(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
}

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

fn same_message_step_split_case_with_other(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_case_with_other;

record MainState;
enum Phase {{ Ready, Done, Other }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
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
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
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

fn same_message_step_split_without_discovered_payload_case(worker_step: &str) -> String {
    format!(
        r#"
module same_message_step_split_without_payload_case;

record MainState;
enum Phase {{ Ready, Done }}
enum Routed {{ Assign(Phase) }}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Envelope(Routed) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

{worker_step}
}}
"#
    )
}

fn fieldless_helper_mismatch_source(selected_call: &str) -> String {
    format!(
        r#"
module fieldless_helper_mismatch;

record MainState {{ selected: Phase }}
enum Phase {{ Ready, Done }}
enum RoutedKind {{ Mark(Phase) }}
enum MainMsg {{ Start }}

fn fieldless_signature(Mark(Ready)) -> Phase ! [] ~ [] @det {{
    return Ready;
}}

fn fieldless_body(route: RoutedKind) -> Phase ! [] ~ [] @det {{
    match route {{
        Mark(Ready) => {{
            return Ready;
        }}
    }}
}}

fn fieldless_return(route: RoutedKind) -> Phase ! [] ~ [] @det {{
    return match route {{
        Mark(Ready) => {{
            return Ready;
        }}
    }};
}}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState {{ selected: {selected_call} }};
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

#[test]
fn source_helpers_reject_fieldless_nested_enum_constructor_mismatches() {
    for (selected_call, expected) in [
        (
            "fieldless_signature(Mark(Done))",
            "function fieldless_signature signature nested payload pattern does not match concrete Done",
        ),
        (
            "fieldless_body(Mark(Done))",
            "function fieldless_body match nested payload pattern does not match concrete Done",
        ),
        (
            "fieldless_return(Mark(Done))",
            "function fieldless_return return match nested payload pattern does not match concrete Done",
        ),
    ] {
        let source = fieldless_helper_mismatch_source(selected_call);
        let err = check_source(&source)
            .expect_err("fieldless nested enum constructor helper mismatch should fail checking");

        assert!(
            err.to_string().contains(expected),
            "expected diagnostic containing {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_duplicate_nested_binding_names() {
    let source = r#"
module duplicate_nested_pattern_binding;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Hold(List<Job,2>) }
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Hold(List<Job,2>[Job { phase: Ready }, Job { phase: Done }]));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Hold(List[Job { phase }, Job { phase }]))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("duplicate nested binding should fail");
    assert!(
        err.to_string().contains("phase is declared more than once"),
        "expected duplicate binding diagnostic, got {err}"
    );
}

#[test]
fn rejects_nested_binding_name_conflicts_with_existing_binding() {
    assert_nested_worker_pattern_rejected(
        "Envelope(Assign(Job { phase: state }))",
        "payload binding state conflicts with a reserved state parameter name",
    );
}

#[test]
fn rejects_nested_record_field_that_does_not_exist() {
    assert_nested_worker_pattern_rejected(
        "Envelope(Assign(Job { missing }))",
        "record payload pattern Job has no field missing",
    );
}

#[test]
fn rejects_nested_collection_pattern_that_exceeds_capacity() {
    assert_nested_worker_pattern_rejected(
        "Envelope(Hold(List[Job { phase }, Job { phase }]))",
        "list payload pattern length 2 exceeds capacity 1 for List<Job,1>",
    );
}

#[test]
fn rejects_nested_list_rest_without_prefix() {
    assert_nested_worker_pattern_rejected(
        "Envelope(Hold(List[..tail]))",
        "list rest payload pattern must declare at least one prefix element",
    );
}

#[test]
fn rejects_nested_map_rest_without_static_key() {
    assert_nested_worker_pattern_rejected(
        "Envelope(Lookup(Map[..rest]))",
        "map rest payload pattern must declare at least one key",
    );
}

#[test]
fn rejects_malformed_nested_pattern_syntax_precisely() {
    let source = nested_worker_pattern_source("Envelope(Assign(Job { phase = phase }))");
    let err = parse_source(&source).expect_err("malformed nested pattern should fail");
    assert!(
        err.to_string()
            .contains("record pattern fields use ':'; assignment syntax is not supported"),
        "expected malformed nested pattern diagnostic, got {err}"
    );
}

#[test]
fn rejects_nested_pattern_payload_type_mismatch() {
    let source = r#"
module nested_pattern_payload_type_mismatch;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Assign(Job) }
enum MainMsg { Start }
enum WorkerMsg { Envelope(Routed) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Job { phase: Ready }));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Assign(List[phase]))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("nested payload type mismatch should fail");
    assert!(
        err.to_string()
            .contains("nested list pattern cannot match value type Job"),
        "expected nested type mismatch diagnostic, got {err}"
    );
}

#[test]
fn rejects_nested_process_reference_payload_binding() {
    let source = r#"
module nested_process_reference_pattern;

record MainState;
enum MainMsg { Start }
enum SinkMsg { Done }
enum Routed { Reply(ProcessRef<Sink>) }
enum WorkerMsg { Envelope(Routed) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = MainState;
    type Msg = SinkMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Done) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = MainState;
    type Msg = WorkerMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Envelope(Reply(reply_to: ProcessRef<Sink>))) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("nested process reference binding should fail");
    assert!(
        err.to_string().contains(
            "nested constructor payload reply_to cannot bind process reference payload type ProcessRef<Sink>; process references must be direct message payload bindings"
        ),
        "expected nested process reference diagnostic, got {err}"
    );
}
