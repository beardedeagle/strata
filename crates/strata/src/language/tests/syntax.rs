use super::support::*;

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

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { phase: Handled });
    }
}
"#;

    let checked = check_source(source).expect("immutable record state should check");

    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{phase:Idle}", "MainState{phase:Handled}"]
    );
    assert_eq!(checked.processes()[0].init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(&checked.processes()[0]).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
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
fn rejects_empty_braced_record_declarations() {
    let source = HELLO.replace("record MainState;", "record MainState {}");

    let err = parse_source(&source).expect_err("empty braced records should be rejected");

    assert!(
        err.to_string().contains(
            "fieldless records use `record MainState;`; braced records must declare at least one field"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_mutable_record_field_declarations() {
    let source = HELLO.replace(
        "record MainState;",
        "enum Phase { Idle }\nrecord MainState { mut phase: Phase }",
    );

    let err = parse_source(&source).expect_err("mutable record fields should be rejected");

    assert!(
        err.to_string()
            .contains("record fields are immutable; mutable field declarations are not supported")
    );
}

#[test]
fn rejects_security_declarations_instead_of_erasing_source() {
    let source = HELLO.replace(
        "record MainState;",
        "security mut policy;\nrecord MainState;",
    );

    let err = parse_source(&source).expect_err("security declarations should not be skipped");

    assert!(
        err.to_string()
            .contains("security declarations are not supported")
    );
}

#[test]
fn rejects_reserved_keywords_as_state_values() {
    for keyword in [
        "_", "as", "bounded", "else", "emit", "enum", "fn", "for", "if", "in", "let", "mailbox",
        "match", "module", "mut", "proc", "record", "return", "security", "send", "spawn", "type",
        "var",
    ] {
        let source = r#"
module reserved_keyword;

record Marker;
enum MainState { REPLACE_KEYWORD }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return REPLACE_KEYWORD;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(REPLACE_KEYWORD);
    }
}
"#
        .replace("REPLACE_KEYWORD", keyword);

        let err = parse_source(&source).expect_err("keyword should be reserved");

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

    assert!(
        err.to_string()
            .contains("record value fields use ':'; assignment syntax is not supported")
    );
}

#[test]
fn rejects_empty_braced_record_values() {
    let source = HELLO.replace("return MainState;", "return MainState {};");

    let err = parse_source(&source).expect_err("empty braced record values should be rejected");

    assert!(
        err.to_string().contains(
            "fieldless record values use `MainState`; braced record values must declare at least one field"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_incomplete_or_invalid_record_values() {
    for (source, expected) in [
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nenum Mode { Cold }\nrecord MainState { phase: Phase, mode: Mode }",
                )
                .replace("return MainState;", "return MainState { phase: Idle };"),
            "record value MainState is missing field mode",
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
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Other { value: Idle } };",
                ),
            "expected enum variant value for enum Phase",
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
fn rejects_empty_state_enum_with_enum_diagnostic() {
    let source = HELLO.replace("record MainState;", "record Marker;\nenum MainState {}");

    let err = check_source(&source).expect_err("empty state enum should fail");

    assert!(
        err.to_string()
            .contains("enum MainState must declare at least one variant")
    );
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
fn rejects_declaration_only_entry_points() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det;
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det;
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

    assert!(
        err.to_string()
            .contains("entry process Main is not declared")
    );
}

#[test]
fn rejects_bare_concrete_state_return_with_accurate_message() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Handled;");

    let err = check_source(&source).expect_err("bare state return should be rejected");

    let message = err.to_string();
    assert!(message.contains(
        "step body must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)"
    ));
    assert!(!message.contains("or a concrete state value"));
}

#[test]
fn rejects_step_return_match_over_state_parameter() {
    let source = ACTOR_PING
        .replace(
            "fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {",
            "fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {",
        )
        .replace("        emit \"worker handled Ping\";\n", "")
        .replace(
            "return Stop(Handled);",
            r#"return match state {
            Idle => {
                return Stop(Handled);
            }
            Handled => {
                return Stop(Handled);
            }
        };"#,
        );

    let err = check_source(&source).expect_err("step return match over state should be rejected");

    assert!(
        err.to_string()
            .contains("process Worker step return match scrutinee state must be a concrete enum source value binding")
    );
}

#[test]
fn rejects_general_match_expression_in_step_return_value() {
    let source = ACTOR_PING.replace(
        "return Stop(Handled);",
        r#"return Continue(match state {
            Idle => {
                return Handled;
            }
            Handled => {
                return Handled;
            }
        });"#,
    );

    let err = parse_source(&source).expect_err("general match expression should fail");

    assert!(
        err.to_string().contains(
            "match expressions are only admitted as whole function bodies or return match expressions in this source slice"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_panic_step_result_with_wrong_state_value() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Panic(MainState);");

    let err = check_source(&source).expect_err("panic must carry a WorkerState value");

    assert!(
        err.to_string()
            .contains("value MainState is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_step_proc_result_with_wrong_state_argument() {
    let source = HELLO.replace("ProcResult<MainState>", "ProcResult<MainMsg>");

    let err = check_source(&source).expect_err("wrong ProcResult argument should fail");

    assert!(
        err.to_string()
            .contains("step returns ProcResult<MainMsg>, expected ProcResult<MainState>")
    );
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
fn rejects_internal_checked_type_label_prefix_declarations() {
    for source in [
        HELLO.replace(
            "record MainState;",
            "record __strata_checked_process_ref_Main;",
        ),
        HELLO.replace(
            "enum MainMsg { Start }",
            "enum __strata_checked_process_ref_Main { Start }",
        ),
    ] {
        let err = check_source(&source).expect_err("reserved type label prefix should fail");

        assert!(
            err.to_string()
                .contains("uses reserved prefix __strata_checked_")
        );
    }
}

#[test]
fn rejects_duplicate_enum_variants() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start, Start }");

    let err = check_source(&source).expect_err("duplicate variant should be rejected");

    assert!(
        err.to_string()
            .contains("duplicate variant in enum MainMsg declaration Start")
    );
}

#[test]
fn rejects_record_enum_type_name_collision() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainState { Start }");

    let err = check_source(&source).expect_err("type name collision should be rejected");

    assert!(
        err.to_string()
            .contains("duplicate type declaration MainState used by record and enum")
    );
}

#[test]
fn rejects_invalid_annotation_identifier_start() {
    let source = HELLO.replacen("@det", "@1", 1);

    let err = parse_source(&source).expect_err("invalid annotation should fail lexing");

    assert!(err.to_string().contains("expected identifier after '@'"));
}
