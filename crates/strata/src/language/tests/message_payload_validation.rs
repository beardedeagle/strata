use super::support::*;

#[test]
fn rejects_send_missing_required_message_payload() {
    let source = payload_source_with(
        "send worker Assign;",
        "fn step(state: WorkerState, Assign(job: Job))",
    );

    let err = check_source(&source).expect_err("missing message payload should fail");

    assert!(
        err.to_string()
            .contains("message Assign requires a payload")
    );
}

#[test]
fn rejects_payload_for_unit_message_variant() {
    let source = ACTOR_PING.replace("send worker Ping;", "send worker Ping(MainState);");

    let err = check_source(&source).expect_err("payload on unit message should fail");

    assert!(
        err.to_string()
            .contains("message Ping does not accept a payload")
    );
}

#[test]
fn rejects_wildcard_payload_binding() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, _(job: Job))",
    );

    let err = parse_source(&source).expect_err("wildcard payload binding should fail");

    assert!(
        err.to_string()
            .contains("wildcard patterns cannot bind payloads")
    );
}

#[test]
fn rejects_forward_payload_binding_with_wrong_send_type() {
    let source = r#"
module forward_payload_wrong_type;

record MainState;
record Job { phase: JobPhase }
record OtherJob { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Assign(OtherJob) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Assign(job);
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Assign(job: OtherJob)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("forwarded payload type mismatch should fail");

    assert!(
        err.to_string()
            .contains("value binding job has type Job, expected OtherJob")
    );
}

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

#[test]
fn rejects_non_process_ref_payload_as_send_target() {
    let source = r#"
module non_ref_send_target;

record MainState;
record Job;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Work(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Work(Job);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(job: Job)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        send job Work(Job);
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("non-ref send target should fail");

    assert!(
        err.to_string()
            .contains("process Worker send target job is not a process reference payload")
    );
}

#[test]
fn rejects_step_payload_binding_with_wrong_type() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, Assign(job: MainState))",
    );

    let err = check_source(&source).expect_err("wrong payload binding type should fail");

    assert!(
        err.to_string()
            .contains("step pattern payload job has type MainState, expected Job")
    );
}

#[test]
fn rejects_payload_binding_named_like_value_constructor() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, Assign(Job: Job))",
    );

    let err = check_source(&source).expect_err("constructor-like payload binding should fail");

    assert!(
        err.to_string()
            .contains("payload binding Job conflicts with a declared type or value constructor")
    );
}

#[test]
fn rejects_process_ref_named_like_payload_binding() {
    let source = r#"
module payload_process_ref_conflict;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let job: ProcessRef<Sink> = spawn Sink;
        send job Ping;
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Ping) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("local binding shadowing should fail");

    assert!(
        err.to_string()
            .contains("process reference job conflicts with payload binding")
    );
}

#[test]
fn rejects_invalid_step_signature_before_payload_case_discovery() {
    let source = r#"
module invalid_step_discovery;

record MainState;
record Job { phase: Phase }
enum Phase { Ready }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Forward(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Forward(Job { phase: Ready });
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Forward(job: Job)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("invalid step signature should fail first");

    assert!(err.to_string().contains(
        "step second parameter must be a message constructor pattern or wildcard pattern"
    ));
}

#[test]
fn rejects_generic_message_payload_type_with_precise_diagnostic() {
    let source = r#"
module generic_payload_type;

record MainState;
record Job;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(ProcResult<Job>) }

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
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: ProcResult<Job>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("generic payload type should fail");

    assert!(err.to_string().contains(
        "payload type ProcResult<Job> must be a named record, enum, list, map, or process reference type"
    ));
}

#[test]
fn rejects_payload_entry_message() {
    let source = r#"
module entry_payload;

record MainState;
record Job;
enum MainMsg { Start(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start(job: Job)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("payload entry message should fail");

    assert!(
        err.to_string()
            .contains("entry message Start must not require a payload")
    );
}
