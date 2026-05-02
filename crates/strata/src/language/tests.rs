use super::lexer::{Lexer, TokenKind};
use super::*;
use mantle_artifact::{
    ArtifactAction, MessageId, OutputId, ProcessId, StateId, StepResult, MAX_ACTIONS_PER_PROCESS,
    MAX_FIELD_VALUE_BYTES, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
    MAX_STATE_VALUES_PER_PROCESS,
};

const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start }

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
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

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

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("reserved state value should fail");

    assert!(err
        .to_string()
        .contains("state value state conflicts with reserved step state parameter name"));
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
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

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
enum MainMsg { Start }
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
fn rejects_process_count_above_artifact_limit_during_checking() {
    let mut source = r#"
module too_many_processes;
record MainState;
enum MainMsg { Start }
"#
    .to_string();
    for index in 0..=MAX_PROCESS_COUNT {
        let name = if index == 0 {
            "Main".to_string()
        } else {
            format!("Proc{index}")
        };
        source.push_str(&format!(
            r#"
proc {name} mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState; }}
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
        ));
    }
    let module = parse_source(&source).expect("oversized process source should parse");

    let err = check_module(module).expect_err("process count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process_count must be no greater than {MAX_PROCESS_COUNT}"
    )));
}

#[test]
fn rejects_mailbox_bound_above_artifact_limit_during_checking() {
    let source = HELLO.replace(
        "mailbox bounded(1)",
        &format!("mailbox bounded({})", MAX_MAILBOX_BOUND + 1),
    );
    let module = parse_source(&source).expect("mailbox-bound source should parse");

    let err = check_module(module).expect_err("mailbox bound above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main mailbox_bound must be no greater than {MAX_MAILBOX_BOUND}"
    )));
}

#[test]
fn rejects_zero_mailbox_bound_with_shared_count_diagnostic() {
    let source = HELLO.replace("mailbox bounded(1)", "mailbox bounded(0)");
    let module = parse_source(&source).expect("zero-mailbox-bound source should parse");

    let err = check_module(module).expect_err("zero mailbox bound should fail");

    assert!(err
        .to_string()
        .contains("process Main mailbox_bound must be greater than zero"));
}

#[test]
fn rejects_state_value_count_above_artifact_limit_during_checking() {
    let state_values = (0..=MAX_STATE_VALUES_PER_PROCESS)
        .map(|index| format!("State{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = HELLO
        .replace(
            "record MainState;",
            &format!("enum MainState {{ {state_values} }}"),
        )
        .replace(
            "enum MainMsg { Start }",
            "record Marker;\nenum MainMsg { Start }",
        )
        .replace("return MainState;", "return State0;");
    let module = parse_source(&source).expect("state-value-count source should parse");

    let err = check_module(module).expect_err("state value count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}"
    )));
}

#[test]
fn rejects_empty_state_enum_with_enum_diagnostic() {
    let source = HELLO.replace("record MainState;", "record Marker;\nenum MainState {}");

    let err = check_source(&source).expect_err("empty state enum should fail");

    assert!(err
        .to_string()
        .contains("enum MainState must declare at least one variant"));
}

#[test]
fn preserves_undeclared_state_type_diagnostics() {
    for (source, expected) in [
        (
            HELLO.replace("type State = MainState;", "type State = MissingState;"),
            "type MissingState is not declared",
        ),
        (
            HELLO.replace("type State = MainState;", "type State = Box<MainState>;"),
            "type Box<MainState> is not declared",
        ),
    ] {
        let err = check_source(&source).expect_err("undeclared state type should fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_message_count_above_artifact_limit_during_checking() {
    let messages = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("Msg{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = HELLO.replace(
        "enum MainMsg { Start }",
        &format!("enum MainMsg {{ {messages} }}"),
    );
    let module = parse_source(&source).expect("message-count source should parse");

    let err = check_module(module).expect_err("message count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main message_count must be no greater than {MAX_MESSAGE_VARIANTS_PER_PROCESS}"
    )));
}

#[test]
fn rejects_action_count_above_artifact_limit_during_checking() {
    let mut statements = String::new();
    for _ in 0..=MAX_ACTIONS_PER_PROCESS {
        statements.push_str("        emit \"hello from Strata\";\n");
    }
    let source = HELLO.replace("        emit \"hello from Strata\";\n", &statements);
    let module = parse_source(&source).expect("action-count source should parse");

    let err = check_module(module).expect_err("action count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
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
        HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start Other }"),
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
fn lexer_accepts_exact_source_token_limit_plus_eof() {
    let source = "{}".repeat(MAX_TOKEN_COUNT / 2);

    let tokens = Lexer::new(&source)
        .tokenize()
        .expect("exact source token limit should tokenize");

    assert_eq!(tokens.len(), MAX_TOKEN_COUNT + 1);
    assert!(matches!(
        tokens.last().map(|token| &token.kind),
        Some(TokenKind::Eof)
    ));
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
enum MainMsg { Start }
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
fn parses_and_checks_immutable_record_state_constructors() {
    let source = r#"
module record_state;

enum Phase { Idle, Handled }
record MainState {
    phase: Phase,
}
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Idle };
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { phase: Handled });
    }
}
"#;

    let checked = check_source(source).expect("immutable record state should check");

    assert_eq!(
        checked.processes[0].state_values,
        ["MainState{phase:Idle}", "MainState{phase:Handled}"]
    );
    assert_eq!(checked.processes[0].init_state, StateId::new(0));
    assert_eq!(checked.processes[0].final_state, StateId::new(1));
}

#[test]
fn rejects_semicolons_after_braced_type_declarations() {
    for (source, expected) in [
        (
            HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start };"),
            "braced enum declarations are terminated by '}', not ';'",
        ),
        (
            HELLO.replace(
                "record MainState;",
                "enum Phase { Idle }\nrecord MainState { phase: Phase };",
            ),
            "braced record declarations are terminated by '}', not ';'",
        ),
    ] {
        let err = parse_source(&source).expect_err("braced type semicolon should be rejected");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_mutable_record_field_declarations() {
    let source = HELLO.replace(
        "record MainState;",
        "enum Phase { Idle }\nrecord MainState { mut phase: Phase }",
    );

    let err = parse_source(&source).expect_err("mutable record fields should be rejected");

    assert!(err
        .to_string()
        .contains("record fields are immutable; mutable field declarations are not supported"));
}

#[test]
fn rejects_security_declarations_instead_of_erasing_source() {
    let source = HELLO.replace(
        "record MainState;",
        "security mut policy;\nrecord MainState;",
    );

    let err = parse_source(&source).expect_err("security declarations should not be skipped");

    assert!(err
        .to_string()
        .contains("security declarations are not supported"));
}

#[test]
fn rejects_mutability_keywords_as_state_values() {
    for keyword in ["mut", "var"] {
        let source = r#"
module reserved_mutability_keyword;

record Marker;
enum MainState { REPLACE_KEYWORD }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return REPLACE_KEYWORD;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(REPLACE_KEYWORD);
    }
}
"#
        .replace("REPLACE_KEYWORD", keyword);

        let err = parse_source(&source).expect_err("mutability keyword should be reserved");

        assert!(
            err.to_string()
                .contains(&format!("identifier {keyword:?} is reserved")),
            "unexpected error for {keyword}: {err}"
        );
    }
}

#[test]
fn rejects_assignment_syntax_in_record_values() {
    let source = HELLO
        .replace(
            "record MainState;",
            "enum Phase { Idle }\nrecord MainState { phase: Phase }",
        )
        .replace("return MainState;", "return MainState { phase = Idle };");

    let err = parse_source(&source).expect_err("record value assignment should be rejected");

    assert!(err
        .to_string()
        .contains("record value fields use ':'; assignment syntax is not supported"));
}

#[test]
fn rejects_incomplete_or_invalid_record_values() {
    for (source, expected) in [
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace("return MainState;", "return MainState {};"),
            "record value MainState is missing field phase",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Idle, extra: Idle };",
                ),
            "record value MainState declares unknown field extra",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Idle, phase: Idle };",
                ),
            "record value MainState duplicates field phase",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nenum Other { Wrong }\nrecord MainState { phase: Phase }",
                )
                .replace("return MainState;", "return MainState { phase: Wrong };"),
            "value Wrong is not a variant of enum Phase",
        ),
    ] {
        let err = check_source(&source).expect_err("invalid record value should be rejected");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
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
        HELLO.replace("enum MainMsg { Start }", "enum ProcResult { Start }"),
    ] {
        let err = check_source(&source).expect_err("reserved type name should fail");

        assert!(err.to_string().contains("type name ProcResult is reserved"));
    }
}

#[test]
fn rejects_duplicate_enum_variants() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start, Start }");

    let err = check_source(&source).expect_err("duplicate variant should be rejected");

    assert!(err
        .to_string()
        .contains("duplicate variant in enum MainMsg declaration Start"));
}

#[test]
fn rejects_record_enum_type_name_collision() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainState { Start }");

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
