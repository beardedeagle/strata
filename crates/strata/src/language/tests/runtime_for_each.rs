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
