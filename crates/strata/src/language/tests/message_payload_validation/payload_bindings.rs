use super::*;

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
