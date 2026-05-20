use super::*;

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
