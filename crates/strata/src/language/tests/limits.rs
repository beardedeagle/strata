use super::support::*;

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
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
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

    assert!(
        err.to_string()
            .contains("process Main mailbox_bound must be greater than zero")
    );
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
fn rejects_checked_type_count_above_artifact_limit_during_checking() {
    let module = checked_type_count_overflow_module();

    let err =
        check_module(module).expect_err("checked type count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "checked type_count exceeds Mantle artifact limit of {MAX_TYPE_COUNT} types"
    )));
}

#[test]
fn accepts_payload_send_count_above_message_variant_limit_without_case_expansion() {
    let phases = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("P{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sends = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("        send worker Assign(Job {{ phase: P{index} }});\n"))
        .collect::<String>();
    let mailbox_bound = MAX_MESSAGE_VARIANTS_PER_PROCESS + 1;
    let source = format!(
        r#"
module concrete_payload_count;

record MainState;
record Job {{ phase: JobPhase }}
enum JobPhase {{ {phases} }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
{sends}        return Stop(state);
    }}
}}

proc Worker mailbox bounded({mailbox_bound}) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Continue(state);
    }}
}}
"#
    );
    let module = parse_source(&source).expect("payload-send-count source should parse");

    let checked = check_module(module).expect("payload sends should not expand message variants");
    let worker = &checked.processes()[1];

    assert_eq!(worker.message_cases().len(), 1);
    assert_eq!(worker.message_cases()[0].label(), "Assign");
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
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
fn rejects_process_action_budget_across_message_transitions_during_checking() {
    let first_actions = repeated_emit_statements(MAX_ACTIONS_PER_PROCESS / 2, 16);
    let second_actions = repeated_emit_statements((MAX_ACTIONS_PER_PROCESS / 2) + 1, 16);
    let source = format!(
        r#"
module action_budget;

record MainState;
enum MainMsg {{ Start, Again }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {{
{first_actions}        return Stop(state);
    }}

    fn step(state: MainState, Again) -> ProcResult<MainState> ! [emit] ~ [] @det {{
{second_actions}        return Stop(state);
    }}
}}
"#
    );
    let module = parse_source(&source).expect("aggregate action-budget source should parse");

    let err = check_module(module).expect_err("aggregate action budget should fail");

    assert!(err.to_string().contains(&format!(
        "process Main action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
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

    assert!(
        err.to_string()
            .contains("type nesting exceeds maximum depth")
    );
}

#[test]
fn rejects_excessive_value_nesting_while_parsing() {
    let value = nested_record_value_source(MAX_VALUE_NESTING + 1);
    let source = HELLO.replacen("return MainState;", &format!("return {value};"), 1);

    let err = parse_source(&source).expect_err("excessive value nesting should fail");

    let message = err.to_string();
    assert!(message.contains("value nesting exceeds maximum depth"));
    assert!(
        message.contains(" at byte "),
        "expected byte-offset context in diagnostic: {message}"
    );
}

#[test]
fn rejects_emit_output_too_large_for_artifacts() {
    let output = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let source = HELLO.replace("hello from Strata", &output);

    let err = check_source(&source).expect_err("oversized emit output should fail");

    assert!(
        err.to_string()
            .contains("output literal exceeds maximum length")
    );
}
