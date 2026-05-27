use super::support::*;

#[test]
fn rejects_unselected_step_return_match_arm_invalid_next_state_template() {
    let source = r#"
module process_return_match_arm_unselected_invalid_next_state_template;

record MainState;
record Job { phase: Phase, key: Phase }

enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Job) }
enum WorkerMsg { Envelope(Route) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Job { phase: Ready, key: Done }));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = Map<Phase,Phase,1>;
    type Msg = WorkerMsg;

    fn init() -> Map<Phase,Phase,1> ! [] ~ [] @det {
        return Map<Phase,Phase,1>[Ready => Ready];
    }

    fn step(state: Map<Phase,Phase,1>, Envelope(Assign(Job { phase: phase, key: key }))) -> ProcResult<Map<Phase,Phase,1>> ! [] ~ [] @det {
        return match phase {
            Ready => {
                return Stop(Map<Phase,Phase,1>[Ready => Done]);
            }
            Done => {
                return Stop(Map<Phase,Phase,1>[key => Ready]);
            }
        };
    }
}
"#;

    let err = check_source(source)
        .expect_err("unselected return-match arm next-state template should be validated");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_invalid_send_payload_template() {
    let source = r#"
module process_return_match_arm_unselected_invalid_send_payload_template;

record MainState;
record SinkState;
record Job { phase: Phase, key: Phase }

enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Job) }
enum WorkerState { Idle, Done }
enum WorkerMsg { Envelope(Route) }
enum SinkMsg { Notice(Map<Phase,Phase,1>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Job { phase: Ready, key: Done }));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    authority spawn_sink: Cap<Spawn<Sink>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(Job { phase: phase, key: key }))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {
            Ready => {
                send sink Notice(Map<Phase,Phase,1>[Ready => Done]);
                return Stop(Done);
            }
            Done => {
                send sink Notice(Map<Phase,Phase,1>[key => Ready]);
                return Stop(Done);
            }
        };
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Notice(payload: Map<Phase,Phase,1>)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source)
        .expect_err("unselected return-match arm send payload template should be validated");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}
