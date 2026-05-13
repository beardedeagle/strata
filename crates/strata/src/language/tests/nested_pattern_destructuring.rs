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
fn source_helpers_bind_nested_patterns_in_signature_body_and_return_match() {
    let source = r#"
module nested_helper_patterns;

record MainState {
    signature: Phase,
    body: Phase,
    ret: Phase,
    list: Phase,
}
record Job { phase: Phase }
enum Phase { Ready, Done }
enum Routed { Assign(Job) }
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

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState {
            signature: phase_signature(Assign(Job { phase: Ready })),
            body: phase_body(Assign(Job { phase: Done })),
            ret: phase_return(Assign(Job { phase: Ready })),
            list: phase_list(List<Routed,1>[Assign(Job { phase: Done })]),
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
