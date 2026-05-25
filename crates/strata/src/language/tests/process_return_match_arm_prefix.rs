use super::support::*;

const ARM_PREFIX_SOURCE: &str = r#"
module process_return_match_arm_prefix;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, SawReady, Done }
enum WorkerMsg { Envelope(Route) }
record SinkState;
enum SinkMsg { Notice(Phase) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit, spawn, send] ~ [] @det {
        emit "return-match uniform prefix";
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {
            Ready => {
                emit "return-match ready arm prefix";
                send sink Notice(Ready);
                return Continue(SawReady);
            }
            Done => {
                emit "return-match done arm prefix";
                send sink Notice(Done);
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

    fn step(state: SinkState, Notice(Ready)) -> ProcResult<SinkState> ! [emit] ~ [] @det {
        emit "sink received ready notice";
        return Stop(state);
    }

    fn step(state: SinkState, Notice(Done)) -> ProcResult<SinkState> ! [emit] ~ [] @det {
        emit "sink received done notice";
        return Stop(state);
    }
}
"#;

#[test]
fn checks_step_return_match_arm_emit_prefixes_are_selected_per_transition() {
    let checked = check_source(ARM_PREFIX_SOURCE).expect("arm-local emit prefixes should check");

    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let mut transition_actions = worker
        .transitions()
        .iter()
        .map(|transition| {
            let emit_texts = transition
                .actions()
                .iter()
                .filter_map(|action| match action {
                    CheckedAction::Emit { output } => {
                        Some(checked.outputs()[output.as_u32() as usize].as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                matches!(
                    transition.actions(),
                    [
                        CheckedAction::Emit { .. },
                        CheckedAction::Spawn { .. },
                        CheckedAction::Emit { .. },
                        CheckedAction::Send { .. },
                    ]
                ),
                "unexpected return-match actions: {:?}",
                transition.actions()
            );
            (
                transition
                    .payload_guard()
                    .map(|payload| payload.label().to_string())
                    .expect("return-match transition should carry payload guard"),
                transition.step_result(),
                transition.next_state(),
                transition.effects().to_vec(),
                emit_texts,
            )
        })
        .collect::<Vec<_>>();
    transition_actions.sort_by(|left, right| left.0.cmp(&right.0));

    assert_eq!(
        transition_actions,
        [
            (
                "Assign(Done)".to_string(),
                CheckedStepResult::Stop,
                CheckedNextState::Value(checked_state_id(2)),
                vec![Effect::Emit, Effect::Spawn, Effect::Send],
                vec![
                    "return-match uniform prefix",
                    "return-match done arm prefix"
                ],
            ),
            (
                "Assign(Ready)".to_string(),
                CheckedStepResult::Continue,
                CheckedNextState::Value(checked_state_id(1)),
                vec![Effect::Emit, Effect::Spawn, Effect::Send],
                vec![
                    "return-match uniform prefix",
                    "return-match ready arm prefix"
                ],
            ),
        ]
    );
    let sink = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Sink")
        .expect("Sink should be checked");
    assert_eq!(
        sink.transitions().len(),
        2,
        "arm-local sends should discover both payload-sensitive Sink transitions"
    );

    lower_to_artifact(&checked, ARM_PREFIX_SOURCE)
        .expect("arm-local emit prefixes should lower as typed actions");
}

#[test]
fn rejects_step_return_match_arm_process_ref_binding() {
    let source = r#"
module process_return_match_arm_spawn_rejected;

record MainState;
record SinkState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, SawReady, Done }
enum WorkerMsg { Envelope(Route) }
enum SinkMsg { Tick }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [spawn] ~ [] @det {
        return match phase {
            Ready => {
                let sink: ProcessRef<Sink> = spawn Sink;
                return Continue(SawReady);
            }
            Done => {
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

    fn step(state: SinkState, Tick) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("arm-local process ref binding should fail");

    assert!(
        err.to_string()
            .contains("process Worker step return match arm cannot bind process reference sink"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_binding_that_shadows_process_ref() {
    let source = r#"
module process_return_match_arm_shadowed_process_ref_rejected;

record MainState;
record SinkState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Choice { Use(Phase) }
enum Route { Assign(Choice) }
enum WorkerState { Idle, Done }
enum WorkerMsg { Envelope(Route) }
enum SinkMsg { Ack }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Use(Ready)));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(choice: Choice))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        return match choice {
            Use(sink: Phase) => {
                send sink Ack;
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

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("arm binding should not shadow process refs");

    assert!(
        err.to_string().contains(
            "process Worker step return match payload binding sink conflicts with a process reference binding"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_terminal_runtime_if() {
    let source = ARM_PREFIX_SOURCE.replace(
        "            Ready => {\n                emit \"return-match ready arm prefix\";\n                send sink Notice(Ready);\n                return Continue(SawReady);\n            }",
        "            Ready => {\n                if (phase == Ready) {\n                    return Continue(SawReady);\n                } else {\n                    return Continue(SawReady);\n                }\n            }",
    );

    let err = check_source(&source).expect_err("arm-local terminal runtime if should fail");

    assert!(
        err.to_string().contains(
            "process Worker step return match arm cannot perform final-position runtime if"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_nested_return_match() {
    let source = ARM_PREFIX_SOURCE.replace(
        "            Ready => {\n                emit \"return-match ready arm prefix\";\n                send sink Notice(Ready);\n                return Continue(SawReady);\n            }",
        "            Ready => {\n                return match phase {\n                    Ready => {\n                        return Continue(SawReady);\n                    }\n                    Done => {\n                        return Continue(SawReady);\n                    }\n                };\n            }",
    );

    let err = check_source(&source).expect_err("nested arm-local return match should fail");

    assert!(
        err.to_string()
            .contains("process Worker step return match arm nested return match is not supported"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_process_ref_payload_binding() {
    let source = r#"
module process_return_match_arm_process_ref_payload_rejected;

record MainState;
record SinkState;
enum MainMsg { Start }
enum Route { Reply(ProcessRef<Sink>) }
enum WorkerState { Idle, Done }
enum WorkerMsg { Envelope(Route) }
enum SinkMsg { Ack }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Envelope(Reply(sink));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(route: Route)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        return match route {
            Reply(reply_to: ProcessRef<Sink>) => {
                send reply_to Ack;
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

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("return-match arm process ref binding should fail");

    assert!(
        err.to_string()
            .contains("process references must be direct message payloads"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_step_return_match_arm_missing_effect_authority() {
    let source = ARM_PREFIX_SOURCE
        .replace("        emit \"return-match uniform prefix\";\n", "")
        .replace(
            "fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit, spawn, send] ~ [] @det {",
            "fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {",
        );

    let err = check_source(&source).expect_err("arm-local emit requires declared authority");

    assert!(
        err.to_string()
            .contains("step uses effect emit but does not declare it"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_invalid_send() {
    let source = r#"
module process_return_match_arm_unselected_invalid_send;

record MainState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum Route { Assign(Phase) }
enum WorkerState { Idle, Done }
enum WorkerMsg { Envelope(Route) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! [emit, send] ~ [] @det {
        return match phase {
            Ready => {
                emit "selected ready";
                return Stop(Done);
            }
            Done => {
                send missing_ref Missing;
                return Stop(Done);
            }
        };
    }
}
"#;

    let err = check_source(source).expect_err("unselected arm send should still be validated");

    assert!(
        err.to_string()
            .contains("process Worker sends to undeclared process reference missing_ref"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_unselected_step_return_match_arm_bare_return() {
    let source = ARM_PREFIX_SOURCE
        .replace("        send worker Envelope(Assign(Done));\n", "")
        .replace(
            "            Done => {\n                emit \"return-match done arm prefix\";\n                send sink Notice(Done);\n                return Stop(Done);",
            "            Done => {\n                emit \"return-match done arm prefix\";\n                send sink Notice(Done);\n                return Done;",
        );

    let err = check_source(&source).expect_err("unselected bare arm return should fail");

    assert!(
        err.to_string().contains(
            "step return match arm must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn checks_step_return_match_arm_send_to_in_scope_direct_process_ref() {
    let source = r#"
module process_return_match_arm_send;

record MainState;
record SinkState;
enum MainMsg { Start }
enum Phase { Ready, Done }
enum WorkerState { Waiting(Phase), Done }
enum WorkerMsg { Work(ProcessRef<Sink>) }
enum SinkMsg { Ack }

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
        return Waiting(Ready);
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        match state {
            Waiting(phase: Phase) => {
                return match phase {
                    Ready => {
                        send reply_to Ack;
                        return Continue(Done);
                    }
                    Done => {
                        send reply_to Ack;
                        return Stop(Done);
                    }
                };
            }
            Done => {
                send reply_to Ack;
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

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("arm-local direct send should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    let waiting_ready = checked_state_labels(worker)
        .iter()
        .position(|label| *label == "Waiting(Ready)")
        .map(checked_state_id)
        .expect("Waiting(Ready) should be admitted");
    let transition = worker
        .transitions()
        .iter()
        .find(|transition| transition.current_state() == Some(waiting_ready))
        .expect("Waiting(Ready) transition should be checked");

    assert_eq!(transition.effects(), &[Effect::Send]);
    assert!(
        matches!(
            transition.actions(),
            [CheckedAction::Send {
                target: CheckedSendTarget::ReceivedPayload {
                    target,
                    ..
                },
                message,
                payload: None,
            }] if *target == checked_process_id(2) && *message == checked_message_id(0)
        ),
        "arm-local send should target the direct received process ref"
    );
}
