use super::support::*;

#[test]
fn parses_checks_and_lowers_state_payload_match() {
    let checked = check_source(STATE_PAYLOAD_MATCH).expect("state payload match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        [
            "Idle",
            "Working(Job{phase:Ready})",
            "Done(Job{phase:Ready})"
        ]
    );
    assert_eq!(worker.transitions().len(), 4);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(worker.transitions()[0].current_state(), None);
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].current_state(),
        Some(checked_state_id(0))
    );
    assert_eq!(worker.transitions()[2].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[2].current_state(),
        Some(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[3].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[3].current_state(),
        Some(checked_state_id(2))
    );

    match worker.transitions()[2].next_state() {
        CheckedNextState::Template(CheckedValueTemplate::EnumVariant {
            variant, payload, ..
        }) => {
            assert_eq!(variant.as_str(), "Done");
            assert_eq!(
                *payload,
                CheckedValueTemplate::CurrentStatePayload {
                    ty: worker.state_values()[1]
                        .payload()
                        .expect("Working should carry Job")
                        .ty()
                        .clone(),
                }
            );
        }
        next_state => panic!("expected current-state payload template, got {next_state:?}"),
    }

    let artifact =
        lower_to_artifact(&checked, STATE_PAYLOAD_MATCH).expect("state match should lower");
    let worker_artifact = &artifact.processes[1];
    assert_eq!(
        artifact_state_labels(worker_artifact),
        [
            "Idle",
            "Working(Job{phase:Ready})",
            "Done(Job{phase:Ready})"
        ]
    );
    let job_ty = artifact_type_id(&artifact, "Job");
    assert_eq!(
        worker_artifact.state_values[1].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_ty,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(
        worker_artifact.state_values[2].payload.as_ref(),
        Some(&mantle_artifact::ArtifactPayload {
            ty: job_ty,
            value: artifact_value("Job{phase:Ready}"),
            process_ref: None,
        })
    );
    assert_eq!(
        worker_artifact.transitions[2].current_state,
        Some(mantle_artifact::StateId::new(1))
    );
    match &worker_artifact.transitions[2].next_state {
        mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
            variant,
            payload,
            ..
        }) => {
            assert_eq!(variant, "Done");
            assert_eq!(
                **payload,
                ArtifactValueTemplate::CurrentStatePayload { ty: job_ty }
            );
        }
        next_state => {
            panic!("expected artifact current-state payload template, got {next_state:?}")
        }
    }
}

#[test]
fn state_match_destructures_subset_map_payloads() {
    let source = r#"
module subset_map_state_payload_match;

record MainState;
enum Phase {
    Ready,
    Done,
}
enum MainMsg {
    Start,
}
enum WorkerState {
    Idle,
    Working(Map<Phase,Phase,2>),
    Done(Phase),
}
enum WorkerMsg {
    Begin,
    Complete,
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Begin;
        send worker Complete;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Begin) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Working(Map<Phase,Phase,2>[Ready => Done, Done => Ready]));
    }

    fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Idle => {
                return Stop(Idle);
            }
            Working(Map[Ready => phase, ..]) => {
                return Stop(Done(phase));
            }
            Done(phase: Phase) => {
                return Stop(Done(phase));
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("subset map state match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert_eq!(
        checked_state_labels(worker),
        [
            "Idle",
            "Working(Map[Done=>Ready,Ready=>Done])",
            "Done(Done)"
        ]
    );

    let artifact =
        lower_to_artifact(&checked, source).expect("subset map state match should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker should lower");
    let complete_working = worker_artifact
        .transitions
        .iter()
        .find(|transition| transition.current_state == Some(mantle_artifact::StateId::new(1)))
        .expect("Working state transition should lower");
    let mantle_artifact::NextState::Template(ArtifactValueTemplate::EnumVariant {
        variant,
        payload,
        ..
    }) = &complete_working.next_state
    else {
        panic!("Working state transition should lower to a Done template");
    };
    assert_eq!(variant, "Done");
    let ArtifactValueTemplate::MapValue {
        key,
        keys,
        projection,
        ..
    } = payload.as_ref()
    else {
        panic!("Done payload should project from current map state payload");
    };
    assert_eq!(key, &artifact_value("Ready"));
    assert_eq!(keys.as_slice(), [artifact_value("Ready")]);
    assert_eq!(*projection, mantle_artifact::MapProjectionMode::Subset);
}

#[test]
fn state_match_sees_concrete_payload_states_from_other_steps() {
    let source = r#"
module concrete_state_payload_match;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle, Working(Job), Done(Job) }
enum WorkerMsg { Begin, Complete }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Begin;
        send worker Complete;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Begin) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Working(Job { phase: Ready }));
    }

    fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Idle => {
                return Stop(Idle);
            }
            Working(job: Job) => {
                return Stop(Done(job));
            }
            Done(job: Job) => {
                return Stop(Done(job));
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("concrete payload state match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        [
            "Idle",
            "Working(Job{phase:Ready})",
            "Done(Job{phase:Ready})"
        ]
    );
    assert_eq!(worker.transitions().len(), 4);
    assert_eq!(
        worker.transitions()[2].current_state(),
        Some(checked_state_id(1))
    );
}

#[test]
fn state_match_wildcard_covers_payload_states_from_message_cases() {
    let source = r#"
module wildcard_state_payload_match;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready }
enum MainMsg { Start }
enum WorkerState { Idle, Working(Job) }
enum WorkerMsg { Assign(Job), Complete }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        send worker Complete;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Working(job));
    }

    fn step(state: WorkerState, Complete) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Idle => {
                return Stop(Idle);
            }
            _ => {
                return Stop(state);
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("wildcard state payload match should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "Working(Job{phase:Ready})"]
    );
    assert_eq!(worker.transitions().len(), 3);
    assert_eq!(
        worker.transitions()[2].current_state(),
        Some(checked_state_id(1))
    );
    assert_eq!(
        worker.transitions()[2].next_state(),
        CheckedNextState::Current
    );
}

#[test]
fn state_match_payload_binding_can_feed_send_payload() {
    let source = r#"
module state_payload_send;

record MainState;
record SinkState;
record Job { phase: JobPhase }
enum JobPhase { Ready }
enum MainMsg { Start }
enum SinkMsg { Ack, Done(Job) }
enum WorkerState { Idle, Working(Job), Done(Job) }
enum WorkerMsg { Assign(Job), Complete(ProcessRef<Sink>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Assign(Job { phase: Ready });
        send worker Complete(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(Working(job));
    }

    fn step(state: WorkerState, Complete(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        match state {
            Idle => {
                send reply_to Ack;
                return Stop(Idle);
            }
            Working(job: Job) => {
                send reply_to Done(job);
                return Stop(Done(job));
            }
            Done(job: Job) => {
                send reply_to Done(job);
                return Stop(Done(job));
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

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: SinkState, Done(job: Job)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("state payload send should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");
    let working_transition = worker
        .transitions()
        .iter()
        .find(|transition| transition.current_state() == Some(checked_state_id(1)))
        .expect("Working transition should be expanded");

    let CheckedAction::Send { payload, .. } = &working_transition.actions()[0] else {
        panic!("expected send action");
    };
    assert!(matches!(
        payload.as_ref(),
        Some(CheckedValueTemplate::CurrentStatePayload { .. })
    ));
    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink process should be checked");
    assert_eq!(sink.transitions().len(), 2);
}
