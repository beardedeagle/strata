use super::support::*;

#[test]
fn checks_step_return_match_arm_for_prefixes_are_selected_and_typed() {
    let checked = check_source(PROCESS_RETURN_MATCH_ARM_FOR_PREFIX)
        .expect("arm-local for prefixes should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink should be checked");

    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(
        sink.transitions().len(),
        2,
        "selected arm-local loop sends should discover Sink payload cases"
    );

    for transition in worker.transitions() {
        assert_eq!(
            transition.effects(),
            &[Effect::Emit, Effect::Spawn, Effect::Send]
        );
        let [
            CheckedAction::Emit { .. },
            CheckedAction::Spawn { .. },
            CheckedAction::Emit { .. },
            CheckedAction::ForEach {
                element,
                collection,
                body,
                max_items,
            },
        ] = transition.actions()
        else {
            panic!(
                "selected return-match arm should lower uniform actions before one typed for action: {:?}",
                transition.actions()
            );
        };
        assert_eq!(*max_items, 2);
        assert!(
            matches!(
                collection,
                CheckedValueTemplate::RecordField { field, .. } if field.as_str() == "jobs"
            ),
            "return-match arm for collection should lower through the typed jobs payload template"
        );
        let [
            CheckedAction::Emit { .. },
            CheckedAction::Send { payload, .. },
        ] = body.as_slice()
        else {
            panic!("arm-local for body should lower to emit then send: {body:?}");
        };
        let Some(payload) = payload.as_ref() else {
            panic!("loop send should carry a typed payload template");
        };
        assert!(
            matches!(
                payload.as_ref(),
                CheckedValueTemplate::RecordField { field, record, .. }
                    if field.as_str() == "phase"
                        && matches!(
                            record.as_ref(),
                            CheckedValueTemplate::LoopElement { element: loop_element, .. }
                                if *loop_element == element.id()
                        )
            ),
            "loop send payload should project from the typed loop element"
        );
    }

    let artifact = lower_to_artifact(&checked, PROCESS_RETURN_MATCH_ARM_FOR_PREFIX)
        .expect("arm-local for prefixes should lower");
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact should exist");
    assert!(
        worker_artifact.transitions.iter().all(|transition| {
            matches!(
                transition.actions.as_slice(),
                [
                    ArtifactAction::Emit { .. },
                    ArtifactAction::Spawn { .. },
                    ArtifactAction::Emit { .. },
                    ArtifactAction::ForEach { body, .. },
                ] if matches!(
                    body.as_slice(),
                    [ArtifactAction::Emit { .. }, ArtifactAction::Send { .. }]
                )
            )
        }),
        "artifact should contain typed for_each actions only after source arm selection"
    );
}

#[test]
fn rejects_step_return_match_arm_multiple_for_each_prefixes() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_PREFIX.replace(
        "                return Continue(SawReady);",
        "                for Job { phase: job_phase } in jobs {\n                    emit \"return-match ready second loop item\";\n                    send sink Notice(job_phase);\n                }\n                return Continue(SawReady);",
    );

    let err = check_source(&source).expect_err("second direct arm-local for should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match arm cannot perform more than one for loop in this source slice"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_nested_for_each_body() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_PREFIX.replace(
        "                    emit \"return-match ready loop item\";\n                    send sink Notice(job_phase);",
        "                    for nested in jobs {\n                        emit \"return-match ready nested loop item\";\n                    }",
    );

    let err = check_source(&source).expect_err("nested arm-local for body should fail");

    assert!(
        err.to_string()
            .contains("nested for loops are not supported in this source slice"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_for_each_missing_send_authority() {
    let source = PROCESS_RETURN_MATCH_ARM_FOR_PREFIX.replace(
        "ProcResult<WorkerState> ! [emit, spawn, send]",
        "ProcResult<WorkerState> ! [emit, spawn]",
    );

    let err = check_source(&source).expect_err("arm-local loop send should require send authority");

    assert!(
        err.to_string()
            .contains("step uses effect send but does not declare it"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_for_each_invalid_send_payload_template() {
    let source = r#"
module process_return_match_arm_for_unselected_invalid_send_payload_template;

record MainState;
record SinkState;
record Job { phase: Phase }
record Assignment { phase: Phase, jobs: List<Job,2> }

enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Assignment) }
enum WorkerState { Idle, Done }
enum WorkerMsg { Envelope(Route) }
enum SinkMsg { Notice(Map<Phase,Phase,1>) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Assignment {
            phase: Ready,
            jobs: List<Job,2>[Job { phase: Ready }, Job { phase: Done }],
        }));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(Assignment { phase: phase, jobs: jobs }))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {
            Ready => {
                for Job { phase: job_phase } in jobs {
                    send sink Notice(Map<Phase,Phase,1>[Ready => Done]);
                }
                return Stop(Done);
            }
            Done => {
                for Job { phase: job_phase } in jobs {
                    send sink Notice(Map<Phase,Phase,1>[job_phase => Ready]);
                }
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
        .expect_err("unselected arm-local loop send payload template should be validated");

    assert!(
        err.to_string()
            .contains("map value type Map<Phase,Phase,1> keys must be static source values"),
        "unexpected error: {err}"
    );
}

#[test]
fn discovers_step_return_match_arm_for_each_sends_from_state_payload_collection() {
    let source = r#"
module process_return_match_arm_for_state_collection;

record MainState;
record SinkState;
record Job { phase: Phase }
record Assignment { phase: Phase, jobs: List<Job,2> }

enum Phase { Ready, Done }
enum MainMsg { Start }
enum WorkerMsg { Work(ProcessRef<Sink>) }
enum WorkerState { Holding(Assignment) }
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
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Assignment {
            phase: Ready,
            jobs: List<Job,2>[Job { phase: Ready }, Job { phase: Done }],
        });
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        match state {
            Holding(Assignment { phase: phase, jobs: jobs }) => {
                return match phase {
                    Ready => {
                        for Job { phase: job_phase } in jobs {
                            send reply_to Notice(job_phase);
                        }
                        return Continue(Holding(Assignment {
                            phase: Ready,
                            jobs: jobs,
                        }));
                    }
                    Done => {
                        return Stop(Holding(Assignment {
                            phase: Done,
                            jobs: jobs,
                        }));
                    }
                };
            }
        }
    }
}

proc Sink mailbox bounded(2) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Notice(Ready)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Continue(state);
    }

    fn step(state: SinkState, Notice(Done)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;

    let checked = check_source(source)
        .expect("state-payload loop collection should discover selected arm loop sends");
    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink should be checked");

    assert_eq!(
        sink.transitions().len(),
        2,
        "loop sends over current-state payload collections should seed Sink payload cases"
    );
}
