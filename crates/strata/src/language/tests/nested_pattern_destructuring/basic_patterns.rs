use super::super::support::*;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_worker: Cap<Spawn<Worker>>;

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
