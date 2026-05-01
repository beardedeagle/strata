use super::*;
use mantle_artifact::{
    ArtifactAction, MessageId, OutputId, ProcessId, StateId, StepResult, MAX_FIELD_VALUE_BYTES,
};

const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

const ACTOR_PING: &str = r#"
module actor_ping;

record MainState;
enum MainMsg { Start };
enum WorkerState { Idle, Handled };
enum WorkerMsg { Ping };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        spawn Worker;
        send Worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#;

#[test]
fn parses_and_checks_hello() {
    let checked = check_source(HELLO).expect("hello should check");

    assert_eq!(checked.module.name.as_str(), "hello");
    assert_eq!(checked.entry_process, ProcessId::new(0));
    assert_eq!(checked.entry_message, MessageId::new(0));
    assert_eq!(checked.outputs, ["hello from Strata"]);
    assert_eq!(checked.processes.len(), 1);
    assert_eq!(checked.processes[0].step_result, StepResult::Stop);
    assert_eq!(
        checked.processes[0].actions,
        [ArtifactAction::Emit {
            output: OutputId::new(0)
        }]
    );
}

#[test]
fn parses_step_return_type_as_structured_type_ref() {
    let module = parse_source(HELLO).expect("hello should parse");
    let step_return_type = &module.processes[0].step.return_type;

    assert_eq!(
        step_return_type,
        &TypeRef::Applied {
            constructor: Identifier::new(PROC_RESULT_TYPE).expect("ProcResult identifier"),
            args: vec![TypeRef::Named(
                Identifier::new("MainState").expect("MainState identifier")
            )],
        }
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
enum MainState { ready };
enum MainMsg { start };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return ready;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(ready);
    }
}
"#;

    let checked = check_source(source).expect("lowercase state values should check");

    assert_eq!(checked.processes[0].state_values, ["ready"]);
    assert_eq!(checked.processes[0].init_state, StateId::new(0));
    assert_eq!(checked.processes[0].final_state, StateId::new(0));
}

#[test]
fn parses_and_checks_actor_ping() {
    let checked = check_source(ACTOR_PING).expect("actor ping should check");

    assert_eq!(checked.module.name.as_str(), "actor_ping");
    assert_eq!(checked.entry_process, ProcessId::new(0));
    assert_eq!(checked.entry_message, MessageId::new(0));
    assert_eq!(checked.outputs, ["worker handled Ping"]);
    assert_eq!(checked.processes.len(), 2);

    let main = checked
        .processes
        .iter()
        .find(|process| process.debug_name == "Main")
        .expect("Main should be checked");
    assert_eq!(
        main.actions,
        [
            ArtifactAction::Spawn {
                target: ProcessId::new(1)
            },
            ArtifactAction::Send {
                target: ProcessId::new(1),
                message: MessageId::new(0)
            }
        ]
    );

    let worker = checked
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker should be checked");
    assert_eq!(worker.init_state, StateId::new(0));
    assert_eq!(worker.final_state, StateId::new(1));
}

#[test]
fn resolves_process_references_to_ids_before_artifact_encoding() {
    let source = r#"
module actor_ping;

record MainState;
enum MainMsg { Start };
enum WorkerState { Idle, Handled };
enum WorkerMsg { Ping };

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        spawn Worker;
        send Worker Ping;
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("reordered actor ping should check");
    let main = checked
        .processes
        .get(checked.entry_process.index())
        .expect("Main entry should be present");

    assert_eq!(checked.entry_process, ProcessId::new(1));
    assert_eq!(main.debug_name, "Main");
    assert_eq!(
        main.actions,
        [
            ArtifactAction::Spawn {
                target: ProcessId::new(0)
            },
            ArtifactAction::Send {
                target: ProcessId::new(0),
                message: MessageId::new(0)
            }
        ]
    );

    let artifact = checked
        .to_artifact(source)
        .expect("checked program should lower");
    let encoded = artifact.encode();
    assert!(encoded.contains("entry_process=1"));
    assert!(encoded.contains("process.1.action.0.target_process=0"));
    assert!(!encoded.contains("target_process=Worker"));
}

#[test]
fn rejects_declaration_only_entry_points() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det;
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det;
}
"#;

    let err = check_source(source).expect_err("declaration-only source should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("init must have a body"),
        "unexpected error: {message}"
    );
}

#[test]
fn rejects_missing_main_entry_process() {
    let source = HELLO.replace("proc Main", "proc Worker");

    let err = check_source(&source).expect_err("missing Main should be rejected");

    assert!(err
        .to_string()
        .contains("entry process Main is not declared"));
}

#[test]
fn rejects_duplicate_process_members() {
    for (source, expected) in [
            (
                HELLO.replace(
                    "type State = MainState;",
                    "type State = MainState;\n    type State = MainState;",
                ),
                "process Main declares duplicate type State",
            ),
            (
                HELLO.replace(
                    "type Msg = MainMsg;",
                    "type Msg = MainMsg;\n    type Msg = MainMsg;",
                ),
                "process Main declares duplicate type Msg",
            ),
            (
                HELLO.replace(
                    "fn init() -> MainState ! [] ~ [] @det {",
                    "fn init() -> MainState ! [] ~ [] @det { return MainState; }\n\n    fn init() -> MainState ! [] ~ [] @det {",
                ),
                "process Main declares duplicate init function",
            ),
            (
                HELLO.replace(
                    "fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {",
                    "fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det { emit \"first\"; return Stop(state); }\n\n    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {",
                ),
                "process Main declares duplicate step function",
            ),
        ] {
            let err = parse_source(&source).expect_err("duplicate process member should fail");

            assert!(
                err.to_string().contains(expected),
                "expected {expected:?}, got {err}"
            );
        }
}

#[test]
fn rejects_missing_list_separators() {
    for source in [
        HELLO.replace("enum MainMsg { Start };", "enum MainMsg { Start Other };"),
        HELLO.replace("! [emit] ~ []", "! [emit send] ~ []"),
        HELLO.replace("ProcResult<MainState>", "ProcResult<MainState MainMsg>"),
    ] {
        let err = parse_source(&source).expect_err("missing separator should fail");

        assert!(err.to_string().contains("expected symbol"));
    }
}

#[test]
fn rejects_oversized_source_before_tokenizing() {
    let source = " ".repeat(MAX_SOURCE_BYTES + 1);

    let err = parse_source(&source).expect_err("oversized source should fail");

    assert!(err.to_string().contains("source exceeds maximum size"));
}

#[test]
fn rejects_excessive_token_count() {
    let source = "{}".repeat((MAX_TOKEN_COUNT / 2) + 1);

    let err = parse_source(&source).expect_err("excessive token count should fail");

    assert!(err.to_string().contains("maximum token count"));
}

#[test]
fn rejects_excessive_type_nesting() {
    let mut nested_type = "MainState".to_string();
    for _ in 0..=MAX_TYPE_NESTING {
        nested_type = format!("Box<{nested_type}>");
    }
    let source = HELLO.replace(
        "ProcResult<MainState>",
        &format!("ProcResult<{nested_type}>"),
    );

    let err = parse_source(&source).expect_err("excessive type nesting should fail");

    assert!(err
        .to_string()
        .contains("type nesting exceeds maximum depth"));
}

#[test]
fn rejects_emit_without_declared_effect() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("undeclared emit should be rejected");
    assert!(err
        .to_string()
        .contains("step uses effect emit but does not declare it"));
}

#[test]
fn rejects_spawn_without_declared_effect() {
    let source = ACTOR_PING.replace("! [spawn, send]", "! [send]");

    let err = check_source(&source).expect_err("undeclared spawn should be rejected");

    assert!(err
        .to_string()
        .contains("step uses effect spawn but does not declare it"));
}

#[test]
fn rejects_unknown_effect_name() {
    let source = HELLO.replace("! [emit]", "! [write]");

    let err = parse_source(&source).expect_err("unknown effect should fail");

    assert!(err.to_string().contains("unsupported effect write"));
}

#[test]
fn rejects_duplicate_static_spawn_target() {
    let source = ACTOR_PING.replace("spawn Worker;", "spawn Worker;\n        spawn Worker;");

    let err = check_source(&source).expect_err("duplicate spawn should be rejected");

    assert!(err.to_string().contains("duplicates spawn target Worker"));
}

#[test]
fn rejects_static_self_spawn() {
    let source = ACTOR_PING
        .replace("! [emit] ~ [] @det", "! [spawn] ~ [] @det")
        .replace(r#"emit "worker handled Ping";"#, "spawn Worker;");

    let err = check_source(&source).expect_err("self-spawn should be rejected");

    assert!(err.to_string().contains("process Worker spawns itself"));
}

#[test]
fn rejects_send_before_static_spawn() {
    let source = ACTOR_PING.replace(
        "spawn Worker;\n        send Worker Ping;",
        "send Worker Ping;\n        spawn Worker;",
    );

    let err = check_source(&source).expect_err("send before spawn should be rejected");

    assert!(err
        .to_string()
        .contains("sends to Worker before it is spawned"));
}

#[test]
fn rejects_send_without_static_spawn() {
    let source = ACTOR_PING
        .replace("! [spawn, send] ~ [] @det", "! [send] ~ [] @det")
        .replace("        spawn Worker;\n", "");

    let err = check_source(&source).expect_err("send without spawn should be rejected");

    assert!(err
        .to_string()
        .contains("sends to Worker before it is spawned"));
}

#[test]
fn rejects_send_to_stopped_process() {
    let source = ACTOR_PING
        .replace("! [emit] ~ [] @det", "! [send] ~ [] @det")
        .replace(r#"emit "worker handled Ping";"#, "send Main Start;");

    let err = check_source(&source).expect_err("send to stopped process should be rejected");

    assert!(err
        .to_string()
        .contains("sends to Main, which is not running"));
}

#[test]
fn rejects_send_to_unknown_message() {
    let source = ACTOR_PING.replace("send Worker Ping;", "send Worker Unknown;");

    let err = check_source(&source).expect_err("unknown message should be rejected");

    assert!(err
        .to_string()
        .contains("sends message Unknown not accepted by Worker"));
}

#[test]
fn rejects_continue_after_self_send() {
    let source = HELLO
        .replace("! [emit]", "! [send]")
        .replace(r#"emit "hello from Strata";"#, "send Main Start;")
        .replace("return Stop(state);", "return Continue(state);");

    let err = check_source(&source).expect_err("self-send continuation should be rejected");

    assert!(err
        .to_string()
        .contains("sends to itself, which is not supported"));
}

#[test]
fn rejects_emit_output_too_large_for_artifacts() {
    let output = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let source = HELLO.replace("hello from Strata", &output);

    let err = check_source(&source).expect_err("oversized emit output should fail");

    assert!(err
        .to_string()
        .contains("output literal exceeds maximum length"));
}

#[test]
fn rejects_bare_concrete_state_return_with_accurate_message() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Handled;");

    let err = check_source(&source).expect_err("bare state return should be rejected");

    let message = err.to_string();
    assert!(
        message.contains("step body must return Stop(<state value>) or Continue(<state value>)")
    );
    assert!(!message.contains("or a concrete state value"));
}

#[test]
fn rejects_step_proc_result_with_wrong_state_argument() {
    let source = HELLO.replace("ProcResult<MainState>", "ProcResult<MainMsg>");

    let err = check_source(&source).expect_err("wrong ProcResult argument should fail");

    assert!(err
        .to_string()
        .contains("step returns ProcResult<MainMsg>, expected ProcResult<MainState>"));
}

#[test]
fn rejects_reserved_proc_result_type_declarations() {
    for source in [
        HELLO.replace("record MainState;", "record ProcResult;"),
        HELLO.replace("enum MainMsg { Start };", "enum ProcResult { Start };"),
    ] {
        let err = check_source(&source).expect_err("reserved type name should fail");

        assert!(err.to_string().contains("type name ProcResult is reserved"));
    }
}

#[test]
fn rejects_duplicate_enum_variants() {
    let source = HELLO.replace("enum MainMsg { Start };", "enum MainMsg { Start, Start };");

    let err = check_source(&source).expect_err("duplicate variant should be rejected");

    assert!(err
        .to_string()
        .contains("duplicate variant in enum MainMsg declaration Start"));
}

#[test]
fn rejects_record_enum_type_name_collision() {
    let source = HELLO.replace("enum MainMsg { Start };", "enum MainState { Start };");

    let err = check_source(&source).expect_err("type name collision should be rejected");

    assert!(err
        .to_string()
        .contains("duplicate type declaration MainState used by record and enum"));
}

#[test]
fn rejects_invalid_annotation_identifier_start() {
    let source = HELLO.replacen("@det", "@1", 1);

    let err = parse_source(&source).expect_err("invalid annotation should fail lexing");

    assert!(err.to_string().contains("expected identifier after '@'"));
}
