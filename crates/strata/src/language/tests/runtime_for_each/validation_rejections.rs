use super::*;

#[test]
fn guarded_runtime_for_each_accepts_loop_prefix_inside_final_runtime_if_branch() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "        if (enabled == True) {\n            for item in items {\n                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }\n            }\n        } else {\n        }\n        return Continue(state);",
        "        if (enabled == True) {\n            for item in items {\n                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }\n            }\n            return Continue(state);\n        } else {\n            return Continue(state);\n        }",
    );
    check_source(&source).expect("final-position runtime if branch loop prefix should be accepted");
}

#[test]
fn guarded_runtime_for_each_rejects_nested_loop_inside_final_runtime_if_branch_loop() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "        if (enabled == True) {\n            for item in items {\n                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }\n            }\n        } else {\n        }\n        return Continue(state);",
        "        if (enabled == True) {\n            for item in items {\n                for other in items {\n                    send worker Branch(other);\n                }\n            }\n            return Continue(state);\n        } else {\n            return Continue(state);\n        }",
    );
    let error = check_source(&source)
        .expect_err("nested loop in final-position runtime if branch loop must remain rejected");
    assert!(
        error
            .to_string()
            .contains("nested for loops are not supported"),
        "{error}"
    );
}

#[test]
fn guarded_runtime_for_each_rejects_process_ref_inside_final_runtime_if_branch() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "        if (enabled == True) {\n            for item in items {\n                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }\n            }\n        } else {\n        }\n        return Continue(state);",
        "        if (enabled == True) {\n            let other: ProcessRef<Worker> = spawn Worker;\n            return Continue(state);\n        } else {\n            return Continue(state);\n        }",
    );
    let error = check_source(&source)
        .expect_err("final-position runtime if branch process ref must remain rejected");
    assert!(
        error
            .to_string()
            .contains("final-position runtime if branch cannot bind process reference other"),
        "{error}"
    );
}

#[test]
fn guarded_runtime_for_each_rejects_nested_loop_inside_branch_loop() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }",
        "                for other in items {\n                    send worker Branch(other);\n                }",
    );
    let error = check_source(&source).expect_err("nested branch-contained loop must be rejected");
    assert!(
        error
            .to_string()
            .contains("nested for loops are not supported"),
        "{error}"
    );
}

#[test]
fn guarded_runtime_for_each_rejects_spawn_inside_branch_loop() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }",
        "                let other: ProcessRef<Worker> = spawn Worker;",
    );
    let error = check_source(&source).expect_err("branch-contained loop spawn must be rejected");
    assert!(
        error
            .to_string()
            .contains("for loop body cannot bind process reference"),
        "{error}"
    );
}

#[test]
fn guarded_runtime_for_each_rejects_return_inside_branch_loop() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "                if (item == True) {\n                    emit \"guarded loop selected true\";\n                    send worker Branch(item);\n                } else {\n                }",
        "                return Stop(state);",
    );
    let error = parse_source(&source).expect_err("branch-contained loop return must be rejected");
    assert!(
        error
            .to_string()
            .contains("for loop bodies are statement-only"),
        "{error}"
    );
}

#[test]
fn guarded_runtime_for_each_accepts_one_direct_nested_branch() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "            for item in items {",
        "            if (enabled == True) {\n                emit \"nested branch\";\n            }\n            for item in items {",
    );
    check_source(&source).expect("one direct nested statement branch should check");
}

#[test]
fn guarded_runtime_for_each_rejects_branch_loop_effect_without_declared_authority() {
    let source = RUNTIME_GUARDED_FOR_EACH.replace(
        "fn step(state: BatchState, Batch(BatchRequest { enabled, items })) -> ProcResult<BatchState> ! [spawn, emit, send] ~ [] @det",
        "fn step(state: BatchState, Batch(BatchRequest { enabled, items })) -> ProcResult<BatchState> ! [spawn] ~ [] @det",
    );
    let error = check_source(&source)
        .expect_err("branch-contained loop effects must require declared authority");
    assert!(
        error
            .to_string()
            .contains("step uses effect emit but does not declare it"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_non_list_collection() {
    let source = RUNTIME_FOR_EACH
        .replace("Batch(List<Bool,2>)", "Batch(Bool)")
        .replace("Batch(List<Bool,2>[True, False])", "Batch(True)")
        .replace("Batch(items: List<Bool,2>)", "Batch(items: Bool)");
    let error = check_source(&source).expect_err("for collection must be a list");
    assert!(
        error.to_string().contains("must have type List<T,N>"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_static_collection_source_folding() {
    let source =
        RUNTIME_FOR_EACH.replace("for item in items", "for item in List<Bool,2>[True, False]");
    let error = check_source(&source).expect_err("for collection must be a binding");
    assert!(
        error
            .to_string()
            .contains("for loop collection must be an identifier binding"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_non_bool_loop_condition() {
    let source =
        RUNTIME_FOR_EACH_IF.replace("if ((item != False) && !(item == False))", "if (state)");
    let error = check_source(&source).expect_err("loop branch condition must be Bool");
    assert!(
        error
            .to_string()
            .contains("if condition must have type Bool"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_missing_bool_contract() {
    let source = RUNTIME_FOR_EACH_IF
        .replace(
            "enum Bool {\n    False,\n    True,\n}",
            "enum Bool {\n    No,\n    Yes,\n}",
        )
        .replace("List<Bool,2>[True, False]", "List<Bool,2>[Yes, No]")
        .replace("item != False", "item != No")
        .replace("item == False", "item == No")
        .replace("flag == True", "flag == Yes");
    let error =
        check_source(&source).expect_err("loop branch condition must require Bool contract");
    assert!(
        error
            .to_string()
            .contains("if condition requires enum Bool { False, True }"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_return_inside_statement_branch() {
    let source =
        RUNTIME_FOR_EACH_IF.replace("emit \"batch selected true\";", "return Stop(state);");
    let error = check_source(&source).expect_err("statement if branch return must be rejected");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches must not return"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_spawn_inside_loop_branch() {
    let source = RUNTIME_FOR_EACH_IF.replace(
        "emit \"batch selected true\";",
        "let other: ProcessRef<Worker> = spawn Worker;",
    );
    let error = check_source(&source).expect_err("loop branch spawn must be rejected");
    assert!(
        error
            .to_string()
            .contains("statement-level if branches cannot bind process references"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_statement_branch_nesting_above_limit() {
    let source = RUNTIME_FOR_EACH_IF.replace(
        "emit \"batch selected true\";",
        "if (item) { if (item) { emit \"nested true\"; } else { emit \"nested false\"; } } else { emit \"outer false\"; }",
    );
    let error = check_source(&source).expect_err("too-deep statement if must be rejected");
    assert!(
        error
            .to_string()
            .contains("statement-level if action nesting exceeds maximum depth"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_if_rejects_branch_effect_without_declared_authority() {
    let source = RUNTIME_FOR_EACH_IF.replace(
        "fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn, emit, send] ~ [] @det",
        "fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn, send] ~ [] @det",
    );
    let error = check_source(&source).expect_err("branch emit authority must be declared");
    assert!(
        error
            .to_string()
            .contains("step uses effect emit but does not declare it"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_process_ref_binding_inside_loop_body() {
    let source = RUNTIME_FOR_EACH.replace(
        "        for item in items {\n            send worker Branch(item);\n        }",
        "        for item in items {\n            let other: ProcessRef<Worker> = spawn Worker;\n            send worker Branch(item);\n        }",
    );
    let error = check_source(&source).expect_err("loop body must not bind process refs");
    assert!(
        error
            .to_string()
            .contains("for loop body cannot bind process reference"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_direct_process_ref_payload_inside_loop_body() {
    let source = r#"
module loop_ref_payload;

record MainState;
record HubState;
record SinkState;
enum Bool { False, True }
enum MainMsg { Start }
enum WorkerState { Holding(List<Bool,2>) }
enum WorkerMsg { Work(ProcessRef<Sink>) }
enum HubMsg { Route(ProcessRef<Sink>) }
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
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(List<Bool,2>[True, False]);
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        match state {
            Holding(items: List<Bool,2>) => {
                let hub: ProcessRef<Hub> = spawn Hub;
                for item in items {
                    send hub Route(reply_to);
                }
                return Stop(Holding(items));
            }
        }
    }
}

proc Hub mailbox bounded(2) {
    type State = HubState;
    type Msg = HubMsg;

    fn init() -> HubState ! [] ~ [] @det {
        return HubState;
    }

    fn step(state: HubState, Route(reply_to: ProcessRef<Sink>)) -> ProcResult<HubState> ! [send] ~ [] @det {
        send reply_to Done;
        return Continue(state);
    }
}

proc Sink mailbox bounded(2) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;
    let error = check_source(source).expect_err("loop body must not forward direct process refs");
    assert!(
        error
            .to_string()
            .contains("process reference payload templates must be direct message payloads"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_loop_element_reassignment() {
    let source = RUNTIME_FOR_EACH.replace(
        "        for item in items {\n            send worker Branch(item);\n        }",
        "        for item in items {\n            item = True;\n        }",
    );
    let error = check_source(&source).expect_err("loop element assignment must be rejected");
    assert!(
        error
            .to_string()
            .contains("assignment statements are not supported"),
        "{error}"
    );
}

#[test]
fn runtime_for_each_rejects_loop_body_effect_without_declared_authority() {
    let source = RUNTIME_FOR_EACH.replace(
        "fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn, send] ~ [] @det",
        "fn step(state: BatchState, Batch(items: List<Bool,2>)) -> ProcResult<BatchState> ! [spawn] ~ [] @det",
    );
    let error = check_source(&source).expect_err("loop body send authority must be declared");
    assert!(
        error
            .to_string()
            .contains("step uses effect send but does not declare it"),
        "{error}"
    );
}
