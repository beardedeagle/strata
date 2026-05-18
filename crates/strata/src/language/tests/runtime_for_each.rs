use super::support::*;

#[test]
fn runtime_for_each_checks_and_lowers_to_mantle_loop_control_flow() {
    let checked = check_source(RUNTIME_FOR_EACH).expect("runtime for source should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::ForEach {
                collection: CheckedValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
                ..
            },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::Send {
                payload: Some(payload),
                ..
            }] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
        )
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_FOR_EACH).expect("runtime for should lower");
    let batch_worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let artifact_transition = batch_worker_artifact
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
            },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::Send {
                payload: Some(ArtifactValueTemplate::LoopElement { element: payload_element, .. }),
                ..
            }] if *payload_element == element.id
        )
    ));

    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=for_each"));
    assert!(encoded.contains(".kind=loop_element"));
    assert!(
        !encoded
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop element dispatch must not lower the source binding name"
    );
}

#[test]
fn runtime_for_each_if_checks_and_lowers_to_mantle_loop_branch_control_flow() {
    let checked = check_source(RUNTIME_FOR_EACH_IF).expect("runtime for-if source should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    let transition = only_transition(batch_worker);
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert!(matches!(
        transition.actions(),
        [
            CheckedAction::Spawn { .. },
            CheckedAction::ForEach {
                collection: CheckedValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
                ..
            },
        ] if matches!(
            body.as_slice(),
            [CheckedAction::IfElse {
                condition: CheckedValueTemplate::BooleanBinary {
                    operator: CheckedValueBooleanOperator::And,
                    left,
                    right,
                    ..
                },
                then_actions,
                else_actions,
            }] if matches!(left.as_ref(), CheckedValueTemplate::Equality { .. })
                && matches!(right.as_ref(), CheckedValueTemplate::BooleanNot { .. })
                && matches!(
                then_actions.as_slice(),
                [
                    CheckedAction::Emit { .. },
                    CheckedAction::Send {
                        payload: Some(payload),
                        ..
                    },
                ] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
            ) && matches!(
                else_actions.as_slice(),
                [
                    CheckedAction::Emit { .. },
                    CheckedAction::Send {
                        payload: Some(payload),
                        ..
                    },
                ] if matches!(payload.as_ref(), CheckedValueTemplate::LoopElement { .. })
            )
        )
    ));

    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert!(matches!(
        only_transition(worker).actions(),
        [CheckedAction::IfElse {
            condition: CheckedValueTemplate::Equality {
                operator: CheckedValueEqualityOperator::Equal,
                left,
                right,
                ..
            },
            then_actions,
            else_actions,
        }] if matches!(left.as_ref(), CheckedValueTemplate::ReceivedPayload { .. })
            && matches!(
                right.as_ref(),
                CheckedValueTemplate::Literal(value) if value.label() == "True"
            )
            && matches!(then_actions.as_slice(), [CheckedAction::Emit { .. }])
            && matches!(else_actions.as_slice(), [CheckedAction::Emit { .. }])
    ));

    let artifact =
        lower_to_artifact(&checked, RUNTIME_FOR_EACH_IF).expect("runtime for-if should lower");
    let batch_worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    let artifact_transition = batch_worker_artifact
        .transitions
        .first()
        .expect("BatchWorker transition should exist");
    assert!(matches!(
        artifact_transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                collection: ArtifactValueTemplate::ReceivedPayload { .. },
                max_items: 2,
                body,
            },
        ] if matches!(
            body.as_slice(),
            [ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::BooleanBinary {
                    ty,
                    operator: ArtifactValueBooleanOperator::And,
                    left,
                    right,
                },
                then_actions,
                else_actions,
            }] if *ty == element.ty
                && matches!(
                    left.as_ref(),
                    ArtifactValueTemplate::Equality {
                        ty,
                        operand_ty,
                        operator: ArtifactValueEqualityOperator::NotEqual,
                        left,
                        right,
                    } if *ty == element.ty
                        && *operand_ty == element.ty
                        && matches!(
                            left.as_ref(),
                            ArtifactValueTemplate::LoopElement {
                                element: condition_element,
                                ..
                            } if *condition_element == element.id
                        )
                        && matches!(
                            right.as_ref(),
                            ArtifactValueTemplate::Literal { ty, value } if *ty == element.ty && value == &artifact_value("False")
                        )
                )
                && matches!(
                    right.as_ref(),
                    ArtifactValueTemplate::BooleanNot { ty, operand }
                        if *ty == element.ty && matches!(operand.as_ref(), ArtifactValueTemplate::Equality { .. })
                )
                && matches!(
                    then_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::Send {
                            payload: Some(ArtifactValueTemplate::LoopElement {
                                element: payload_element,
                                ..
                            }),
                            ..
                        },
                    ] if *payload_element == element.id
                )
                && matches!(
                    else_actions.as_slice(),
                    [
                        ArtifactAction::Emit { .. },
                        ArtifactAction::Send {
                            payload: Some(ArtifactValueTemplate::LoopElement {
                                element: payload_element,
                                ..
                            }),
                            ..
                        },
                    ] if *payload_element == element.id
                )
        )
    ));
    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=if_else"));
    assert!(encoded.contains(".kind=boolean_binary"));
    assert!(encoded.contains(".kind=boolean_not"));
    assert!(encoded.contains(".kind=equality"));
    assert!(encoded.contains(".kind=loop_element"));
    assert!(
        !encoded
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop branch dispatch must not lower the source binding name"
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
fn runtime_for_each_if_rejects_nested_statement_branch() {
    let source = RUNTIME_FOR_EACH_IF.replace(
        "emit \"batch selected true\";",
        "if (item) { emit \"nested true\"; } else { emit \"nested false\"; }",
    );
    let error = check_source(&source).expect_err("nested statement if must be rejected");
    assert!(
        error
            .to_string()
            .contains("nested statement-level if branches are not supported"),
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
