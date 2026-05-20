use super::*;

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
