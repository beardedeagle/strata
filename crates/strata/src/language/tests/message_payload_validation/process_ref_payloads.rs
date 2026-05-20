use super::*;

#[test]
fn rejects_process_ref_payload_with_wrong_target_type() {
    let source = r#"
module wrong_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
record OtherState;
enum MainMsg { Start }
enum WorkerMsg { Work(ProcessRef<Other>) }
enum SinkMsg { Done }
enum OtherMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Other>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Other mailbox bounded(1) {
    type State = OtherState;
    type Msg = OtherMsg;

    fn init() -> OtherState ! [] ~ [] @det {
        return OtherState;
    }

    fn step(state: OtherState, Done) -> ProcResult<OtherState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("wrong process ref payload should fail");

    assert!(
        err.to_string()
            .contains("process reference payload sink targets process id 2, expected 3")
    );
}

#[test]
fn rejects_process_ref_payload_with_wrong_type_arity() {
    let source = ACTOR_REPLY
        .replace("Work(ProcessRef<Sink>)", "Work(ProcessRef<Sink,Other>)")
        .replace(
            "Work(reply_to: ProcessRef<Sink>)",
            "Work(reply_to: ProcessRef<Sink,Other>)",
        );

    let err = check_source(&source).expect_err("wrong process ref payload arity should fail");

    assert!(
        err.to_string().contains(
            "enum WorkerMsg variant Work payload type ProcessRef<Sink,Other> must declare exactly one target process"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_bare_process_ref_payload_with_arity_diagnostic() {
    let source = ACTOR_REPLY
        .replace("Work(ProcessRef<Sink>)", "Work(ProcessRef)")
        .replace(
            "Work(reply_to: ProcessRef<Sink>)",
            "Work(reply_to: ProcessRef)",
        );

    let err = check_source(&source).expect_err("bare process ref payload should fail");

    assert!(
        err.to_string().contains(
            "enum WorkerMsg variant Work payload type ProcessRef must declare exactly one target process"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_record_field_containing_process_ref_type() {
    let source = r#"
module record_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
record Route { reply_to: ProcessRef<Sink> }
enum MainMsg { Start }
enum WorkerMsg { Work(Route) }
enum SinkMsg { Done }

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

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(route: Route)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("record process ref field should fail");

    assert!(
        err.to_string().contains(
            "record Route field reply_to type ProcessRef<Sink> contains a process reference; process references must be direct message payloads"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_record_field_containing_bare_process_ref_type() {
    let source = r#"
module record_bare_process_ref_payload;

record MainState;
record WorkerState;
record Route { reply_to: ProcessRef }
enum MainMsg { Start }
enum WorkerMsg { Work(Route) }

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

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(route: Route)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("bare record process ref field should fail");

    assert!(
        err.to_string().contains(
            "record Route field reply_to type ProcessRef contains a process reference; process references must be direct message payloads"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_collection_message_payload_containing_process_ref_type() {
    let source = r#"
module collection_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
enum MainMsg { Start }
enum WorkerMsg { Work(List<ProcessRef<Sink>,1>) }
enum SinkMsg { Done }

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

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(refs: List<ProcessRef<Sink>,1>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("collection process ref payload should fail");

    assert!(
        err.to_string().contains(
            "enum WorkerMsg variant Work payload type List<ProcessRef<Sink>,1> contains a process reference; process references must be direct message payloads"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_key_message_payload_containing_process_ref_type() {
    let source = r#"
module map_key_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
enum MainMsg { Start }
enum WorkerMsg { Work(Map<ProcessRef<Sink>,MainState,1>) }
enum SinkMsg { Done }

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

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(refs: Map<ProcessRef<Sink>,MainState,1>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("map key process ref payload should fail");

    assert!(
        err.to_string().contains(
            "enum WorkerMsg variant Work payload type Map<ProcessRef<Sink>,MainState,1> contains a process reference; process references must be direct message payloads"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_map_value_message_payload_containing_process_ref_type() {
    let source = r#"
module map_value_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
enum MainMsg { Start }
enum WorkerMsg { Work(Map<MainState,ProcessRef<Sink>,1>) }
enum SinkMsg { Done }

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

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(refs: Map<MainState,ProcessRef<Sink>,1>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("map value process ref payload should fail");

    assert!(
        err.to_string().contains(
            "enum WorkerMsg variant Work payload type Map<MainState,ProcessRef<Sink>,1> contains a process reference; process references must be direct message payloads"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_malformed_collection_process_ref_payload_with_collection_diagnostic() {
    let source = ACTOR_REPLY
        .replace("Work(ProcessRef<Sink>)", "Work(List<ProcessRef<Sink>>)")
        .replace(
            "Work(reply_to: ProcessRef<Sink>)",
            "Work(reply_to: List<ProcessRef<Sink>>)",
        );

    let err = check_source(&source).expect_err("malformed collection payload should fail");

    assert!(
        err.to_string().contains(
            "list type List<ProcessRef<Sink>> must declare exactly one element type and one numeric capacity"
        ),
        "unexpected error: {err}"
    );
}
