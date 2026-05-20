use super::support::*;

#[test]
fn runtime_loop_element_record_projection_checks_and_lowers() {
    let checked = check_source(RUNTIME_LOOP_ELEMENT_PROJECTION)
        .expect("runtime loop element projection should check");
    let batch_worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "BatchWorker")
        .expect("BatchWorker should be checked");
    assert!(
        batch_worker.process_refs().is_empty(),
        "BatchWorker must route through the received process ref without local authority"
    );
    let transition = batch_worker
        .transitions()
        .iter()
        .find(|transition| transition.message() == checked_message_id(1))
        .expect("Route transition should exist");
    assert!(matches!(
        transition.actions(),
        [CheckedAction::IfElse {
            condition:
                CheckedValueTemplate::Equality {
                    left: enabled,
                    right,
                    ..
                },
            then_actions,
            else_actions,
        }] if matches!(
            enabled.as_ref(),
            CheckedValueTemplate::RecordField {
                record,
                field,
                ..
            } if field.as_str() == "enabled"
                && matches!(
                    record.as_ref(),
                    CheckedValueTemplate::CurrentStatePayload { .. }
                )
        ) && matches!(
            right.as_ref(),
            CheckedValueTemplate::Literal(value) if value.label() == "True"
        ) && matches!(
            then_actions.as_slice(),
            [CheckedAction::ForEach {
                collection:
                    CheckedValueTemplate::RecordField {
                        record,
                        field,
                        ..
                    },
                max_items: 2,
                body,
                ..
            }] if field.as_str() == "jobs"
                && matches!(
                    record.as_ref(),
                    CheckedValueTemplate::CurrentStatePayload { .. }
                )
                && matches!(
                    body.as_slice(),
                    [CheckedAction::IfElse {
                        condition:
                            CheckedValueTemplate::Equality {
                                left: phase,
                                right,
                                ..
                            },
                        then_actions,
                        else_actions,
                    }] if matches!(
                        phase.as_ref(),
                        CheckedValueTemplate::RecordField {
                            record,
                            field,
                            ..
                        } if field.as_str() == "phase"
                            && matches!(
                                record.as_ref(),
                                CheckedValueTemplate::LoopElement { .. }
                            )
                    ) && matches!(
                        right.as_ref(),
                        CheckedValueTemplate::Literal(value) if value.label() == "Ready"
                    ) && matches!(
                        then_actions.as_slice(),
                        [CheckedAction::Send {
                            target: CheckedSendTarget::ReceivedPayload { .. },
                            payload: Some(payload),
                            ..
                        }] if matches!(
                            payload.as_ref(),
                            CheckedValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            } if field.as_str() == "phase"
                                && matches!(
                                    record.as_ref(),
                                    CheckedValueTemplate::LoopElement { .. }
                                )
                        )
                    ) && else_actions.is_empty()
                )
        ) && else_actions.is_empty()
    ));

    let artifact = lower_to_artifact(&checked, RUNTIME_LOOP_ELEMENT_PROJECTION)
        .expect("runtime loop element projection should lower");
    let worker_process_index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == "Worker")
        .expect("Worker process should exist");
    let worker_process_id =
        ProcessId::from_index(worker_process_index).expect("artifact process index should fit");
    let worker_ref_type = artifact_process_ref_type_id(&artifact, worker_process_id);
    let batch_worker = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "BatchWorker")
        .expect("BatchWorker artifact should exist");
    assert!(
        batch_worker.process_refs.is_empty(),
        "BatchWorker artifact must not gain local process-ref authority"
    );
    let route = batch_worker
        .transitions
        .iter()
        .find(|transition| transition.message == MessageId::new(1))
        .expect("Route artifact transition should exist");
    assert!(matches!(
        route.actions.as_slice(),
        [ArtifactAction::IfElse {
            then_actions,
            else_actions,
            ..
        }] if matches!(
            then_actions.as_slice(),
            [ArtifactAction::ForEach {
                element,
                body,
                ..
            }] if matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    condition:
                        ArtifactValueTemplate::Equality {
                            left,
                            right,
                            ..
                        },
                    then_actions,
                    else_actions,
                }] if matches!(
                    left.as_ref(),
                    ArtifactValueTemplate::RecordField {
                        record,
                        field,
                        ..
                    } if field == "phase"
                        && matches!(
                            record.as_ref(),
                            ArtifactValueTemplate::LoopElement {
                                element: condition_element,
                                ..
                            } if *condition_element == element.id
                        )
                ) && matches!(
                    right.as_ref(),
                    ArtifactValueTemplate::Literal { value, .. } if value == &artifact_value("Ready")
                ) && matches!(
                    then_actions.as_slice(),
                    [ArtifactAction::Send {
                        target:
                            ArtifactSendTarget::ReceivedPayload {
                                ty,
                                target_process,
                            },
                        payload:
                            Some(ArtifactValueTemplate::RecordField {
                                record,
                                field,
                                ..
                            }),
                        ..
                    }] if *ty == worker_ref_type
                        && *target_process == worker_process_id
                        && field == "phase"
                        && matches!(
                            record.as_ref(),
                            ArtifactValueTemplate::LoopElement {
                                element: payload_element,
                                ..
                            } if *payload_element == element.id
                        )
                ) && else_actions.is_empty()
            )
        ) && else_actions.is_empty()
    ));

    let encoded = artifact.encode();
    assert!(
        !encoded.lines().any(|line| {
            line.contains("debug_name=job")
                || line.contains("debug_name=routed_phase")
                || line.ends_with("=job")
                || line.ends_with("=routed_phase")
        }),
        "loop element projections must not lower source bindings as executable names"
    );
}

#[test]
fn runtime_loop_element_record_projection_rejects_unknown_field() {
    let source = RUNTIME_LOOP_ELEMENT_PROJECTION
        .replace("for Job { phase: routed_phase }", "for Job { lane }");
    let error = check_source(&source).expect_err("unknown loop element field should be rejected");
    assert!(
        error
            .to_string()
            .contains("for loop record pattern Job has no field lane"),
        "{error}"
    );
}

#[test]
fn runtime_loop_element_record_projection_rejects_wrong_record_pattern() {
    let source = RUNTIME_LOOP_ELEMENT_PROJECTION.replace(
        "for Job { phase: routed_phase }",
        "for WorkerState { phase }",
    );
    let error = check_source(&source).expect_err("wrong loop element record should be rejected");
    assert!(
        error
            .to_string()
            .contains("for loop record pattern WorkerState cannot match record Job"),
        "{error}"
    );
}

#[test]
fn runtime_loop_element_record_projection_rejects_process_ref_field_data() {
    let source = RUNTIME_LOOP_ELEMENT_PROJECTION
        .replace(
            "record Job {\n    phase: Phase,\n}",
            "record Job {\n    reply_to: ProcessRef<Worker>,\n    phase: Phase,\n}",
        )
        .replace("for Job { phase: routed_phase }", "for Job { reply_to }");
    let error = check_source(&source).expect_err("process-ref field data should remain rejected");
    assert!(
        error.to_string().contains(
            "record Job field reply_to type ProcessRef<Worker> contains a process reference"
        ),
        "{error}"
    );
}

#[test]
fn runtime_loop_element_record_projection_rejects_reassignment() {
    let source = RUNTIME_LOOP_ELEMENT_PROJECTION.replace(
        "send worker AssignPhase(routed_phase);",
        "routed_phase = Ready;",
    );
    let error =
        check_source(&source).expect_err("projected loop field reassignment should be rejected");
    assert!(
        error
            .to_string()
            .contains("assignment statements are not supported"),
        "{error}"
    );
}

const RUNTIME_LOOP_ELEMENT_PROJECTION: &str = r#"
module runtime_loop_element_projection;

record MainState;

record Job {
    phase: Phase,
}

record BatchRequest {
    enabled: Bool,
    jobs: List<Job,2>,
}

enum Bool {
    False,
    True,
}

enum Phase {
    Ready,
    Done,
}

enum MainMsg {
    Start,
}

enum BatchState {
    Configured(BatchRequest),
}

enum BatchMsg {
    Configure(BatchRequest),
    Route(ProcessRef<Worker>),
}

record WorkerState;

enum WorkerMsg {
    AssignPhase(Phase),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let batch: ProcessRef<BatchWorker> = spawn BatchWorker;
        let worker: ProcessRef<Worker> = spawn Worker;
        send batch Configure(BatchRequest { enabled: True, jobs: List<Job,2>[Job { phase: Ready }, Job { phase: Done }] });
        send batch Route(worker);
        return Stop(state);
    }
}

proc BatchWorker mailbox bounded(2) {
    type State = BatchState;
    type Msg = BatchMsg;

    fn init() -> BatchState ! [] ~ [] @det {
        return Configured(BatchRequest { enabled: False, jobs: List<Job,2>[Job { phase: Done }, Job { phase: Done }] });
    }

    fn step(state: BatchState, Configure(request: BatchRequest)) -> ProcResult<BatchState> ! [] ~ [] @det {
        return Continue(Configured(request));
    }

    fn step(state: BatchState, Route(worker: ProcessRef<Worker>)) -> ProcResult<BatchState> ! [send] ~ [] @det {
        match state {
            Configured(BatchRequest { enabled, jobs }) => {
                if (enabled == True) {
                    for Job { phase: routed_phase } in jobs {
                        if (routed_phase == Ready) {
                            send worker AssignPhase(routed_phase);
                        } else {
                        }
                    }
                } else {
                }
                return Continue(state);
            }
        }
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, AssignPhase(phase: Phase)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;
