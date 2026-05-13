use super::support::*;

#[test]
fn checks_payload_state_enum_values() {
    let checked = check_source(STATE_PAYLOAD_ENUM).expect("payload state enum should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "Working(Job{phase:Ready})"]
    );
    assert_eq!(worker.init_state(), checked_state_id(0));
    match only_transition(worker).next_state() {
        CheckedNextState::Template(CheckedValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        }) => {
            assert_eq!(&ty, worker.state_type());
            assert_eq!(variant.as_u32(), 1);
            assert_eq!(
                *payload,
                CheckedValueTemplate::ReceivedPayload {
                    ty: worker.message_cases()[0]
                        .payload_type()
                        .expect("Assign should carry Job")
                        .clone(),
                }
            );
        }
        next_state => panic!("expected payload enum next-state template, got {next_state:?}"),
    }
}

#[test]
fn checks_concrete_payload_state_enum_init_value() {
    let source = STATE_PAYLOAD_ENUM
        .replace("fn init() -> WorkerState ! [] ~ [] @det {\n        return Idle;\n    }", "fn init() -> WorkerState ! [] ~ [] @det {\n        return Working(Job { phase: Ready });\n    }")
        .replace("return Stop(Working(job));", "return Stop(Idle);");

    let checked = check_source(&source).expect("concrete payload state enum init should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker process should be checked");

    assert_eq!(
        checked_state_labels(worker),
        ["Idle", "Working(Job{phase:Ready})"]
    );
    assert_eq!(worker.init_state(), checked_state_id(1));
    assert_eq!(
        only_transition(worker).next_state(),
        CheckedNextState::Value(checked_state_id(0))
    );
}

#[test]
fn rejects_assignment_style_state_update_syntax() {
    let source = STATE_PAYLOAD_MATCH.replace(
        "emit \"worker completed job\";",
        "job = Job { phase: Done };",
    );

    let err = parse_source(&source).expect_err("assignment syntax should fail to parse");

    assert!(err.to_string().contains("expected") || err.to_string().contains("unexpected token"));
}

#[test]
fn rejects_payload_state_constructor_without_payload_value() {
    let source = STATE_PAYLOAD_ENUM.replace("return Stop(Working(job));", "return Stop(Working);");

    let err =
        check_source(&source).expect_err("payload state constructor without payload should fail");

    assert!(err.to_string().contains(
        "enum variant Working requires a payload and cannot be used as a fieldless value"
    ));
}

#[test]
fn rejects_fieldless_state_constructor_with_payload_value() {
    let source =
        STATE_PAYLOAD_ENUM.replace("return Stop(Working(job));", "return Stop(Idle(job));");

    let err =
        check_source(&source).expect_err("fieldless state constructor with payload should fail");

    assert!(
        err.to_string()
            .contains("enum variant Idle does not accept a payload")
    );
}

#[test]
fn rejects_payload_state_constructor_with_wrong_payload_type() {
    let source =
        STATE_PAYLOAD_ENUM.replace("return Stop(Working(job));", "return Stop(Working(Ready));");

    let err = check_source(&source).expect_err("wrong state payload type should fail");

    assert!(
        err.to_string()
            .contains("record state type Job must be constructed with Job { ... }")
    );
}

#[test]
fn rejects_unknown_payload_state_constructor() {
    let source =
        STATE_PAYLOAD_ENUM.replace("return Stop(Working(job));", "return Stop(Missing(job));");

    let err = check_source(&source).expect_err("unknown state constructor should fail");

    assert!(
        err.to_string()
            .contains("value Missing is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_process_ref_payload_state_constructor() {
    let source = r#"
module state_process_ref_payload;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Routed(ProcessRef<Sink>) }
enum WorkerMsg { Start }
record SinkState;
enum SinkMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Start;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Start) -> ProcResult<WorkerState> ! [spawn] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        return Stop(Routed(sink));
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Ping) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("process refs in state payloads should fail");

    assert!(
        err.to_string()
            .contains("process reference payloads are not valid state values"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_received_process_ref_payload_state_constructor() {
    let source = ACTOR_REPLY
        .replace(
            "record WorkerState;",
            "enum WorkerState { Idle, Routed(ProcessRef<Sink>) }",
        )
        .replace("return WorkerState;", "return Idle;")
        .replace(
            "send reply_to Done;\n        return Stop(state);",
            "send reply_to Done;\n        return Stop(Routed(reply_to));",
        );

    let err =
        check_source(&source).expect_err("received process refs in state payloads should fail");

    assert!(
        err.to_string()
            .contains("process reference payloads are not valid state values"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_nested_process_ref_message_payload_with_direct_payload_diagnostic() {
    let source = r#"
module nested_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
enum Envelope { Forward(ProcessRef<Sink>) }
enum MainMsg { Start }
enum WorkerMsg { Route(Envelope) }
enum SinkMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Route(Forward(sink));
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Route(env: Envelope)) -> ProcResult<WorkerState> ! [] ~ [] @det {
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

    let err = check_source(source).expect_err("nested process refs in payloads should fail");

    assert!(
        err.to_string()
            .contains("process references must be direct message payloads"),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_assignment_style_payload_state_construction() {
    let source = STATE_PAYLOAD_ENUM.replace(
        "return Stop(Working(job));",
        "return Stop(Working(Job { phase = Ready }));",
    );

    let err =
        parse_source(&source).expect_err("assignment in payload state construction should fail");

    assert!(
        err.to_string()
            .contains("record value fields use ':'; assignment syntax is not supported")
    );
}

#[test]
fn public_ast_constructors_validate_values() {
    let identifier = Identifier::new("MainState").expect("valid identifier should construct");
    assert_eq!(identifier.as_str(), "MainState");
    let identifier_from_try =
        Identifier::try_from("Worker").expect("TryFrom should construct identifiers");
    assert_eq!(identifier_from_try.as_str(), "Worker");
    assert!(Identifier::new("1Invalid").is_err());
    assert!(Identifier::new("invalid-name").is_err());
    assert!(Identifier::new("_").is_err());
    assert!(Identifier::new("as").is_err());
    assert!(Identifier::new("let").is_err());
    assert!(Identifier::new("mut").is_err());
    assert!(Identifier::new("var").is_err());

    let output = OutputLiteral::new("hello from Strata").expect("valid output should construct");
    assert_eq!(output.as_str(), "hello from Strata");
    let output_from_try =
        OutputLiteral::try_from("worker handled Ping").expect("TryFrom should construct output");
    assert_eq!(output_from_try.as_str(), "worker handled Ping");
    assert!(OutputLiteral::new("").is_err());
    assert!(OutputLiteral::new("bad\noutput").is_err());
}

#[test]
fn resolves_lowercase_state_values_without_casing_semantics() {
    let source = r#"
module lowercase_state;

record Marker;
enum MainState { ready }
enum MainMsg { start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return ready;
    }

    fn step(state: MainState, start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(ready);
    }
}
"#;

    let checked = check_source(source).expect("lowercase state values should check");

    assert_eq!(checked_state_labels(&checked.processes()[0]), ["ready"]);
    assert_eq!(checked.processes()[0].init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(&checked.processes()[0]).next_state(),
        CheckedNextState::Value(checked_state_id(0))
    );
}

#[test]
fn rejects_state_value_named_like_step_state_parameter() {
    let source = r#"
module reserved_state_value;

record Marker;
enum MainState { state }
enum MainMsg { start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return state;
    }

    fn step(state: MainState, start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("reserved state value should fail");

    assert!(
        err.to_string()
            .contains("state value state conflicts with reserved step state parameter name")
    );
}
