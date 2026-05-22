use super::support::*;

#[test]
fn checks_return_match_arm_state_payloads_feed_selected_arm_send_discovery() {
    let source = r#"
module process_return_match_arm_prefix_state_discovery;

record MainState;
record SinkState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum SeedRoute { Pick(Phase) }
enum WorkerState { Holding(Phase), Done }
enum WorkerMsg { Seed(SeedRoute), Tick(ProcessRef<Sink>) }
enum SinkMsg { Notice(Phase) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Seed(Pick(Ready));
        send worker Tick(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Done);
    }

    fn step(state: WorkerState, Seed(Pick(phase: Phase))) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return match phase {
            Ready => {
                return Continue(Holding(phase));
            }
            Done => {
                return Continue(Holding(phase));
            }
        };
    }

    fn step(state: WorkerState, Tick(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        match state {
            Holding(phase: Phase) => {
                return match phase {
                    Ready => {
                        send reply_to Notice(Ready);
                        return Stop(Done);
                    }
                    Done => {
                        send reply_to Notice(Done);
                        return Stop(Done);
                    }
                };
            }
            Done => {
                send reply_to Notice(Done);
                return Stop(Done);
            }
        }
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Notice(Ready)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }

    fn step(state: SinkState, Notice(Done)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source)
        .expect("return-match-produced state payloads should feed selected arm send discovery");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let state_labels = checked_state_labels(worker);
    assert!(state_labels.contains(&"Holding(Ready)"));
    assert!(state_labels.contains(&"Holding(Done)"));

    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink should be checked");
    assert_eq!(
        sink.transitions().len(),
        2,
        "state payloads produced by return-match arms should discover both Sink notices"
    );

    lower_to_artifact(&checked, source)
        .expect("return-match state discovery should lower through typed artifacts");
}
