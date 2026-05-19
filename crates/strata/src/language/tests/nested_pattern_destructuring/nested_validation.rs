use super::super::support::*;
use super::shared::*;

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
