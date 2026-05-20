use super::*;

#[test]
fn runtime_for_each_if_noop_branch_traces_inside_loop_body() {
    let gate = GateHarness::new();
    const STEM: &str = "runtime_for_each_if_noop";
    const ARTIFACT: &str = "target/strata/runtime_for_each_if_noop.mta";
    let source = include_str!("../../../../../examples/runtime_for_each_if.str")
        .replace(
            "module runtime_for_each_if;",
            "module runtime_for_each_if_noop;",
        )
        .replace(
            "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            } else {\n                emit \"batch selected false\";\n                send worker Branch(item);\n            }",
            "            if ((item != False) && !(item == False)) {\n                emit \"batch selected true\";\n                send worker Branch(item);\n            }",
        );
    let source = gate.write_target_source(STEM, &source);
    let source = source
        .to_str()
        .expect("target source path should be valid UTF-8");
    gate.remove_trace(STEM);
    let run = gate.check_build_run(source, ARTIFACT);

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("batch selected true"));
    assert!(stdout.contains("worker handled true"));
    assert!(!stdout.contains("batch selected false"));
    assert!(!stdout.contains("worker handled false"));

    let artifact = gate.read_artifact(ARTIFACT);
    let bool_type = value_type_id(&artifact, "Bool");
    let batch_worker = artifact_process(&artifact, "BatchWorker");
    let transition = batch_worker
        .transitions
        .first()
        .expect("BatchWorker should have a Batch transition");
    assert!(matches!(
        transition.actions.as_slice(),
        [
            ArtifactAction::Spawn { .. },
            ArtifactAction::ForEach {
                element,
                body,
                ..
            },
        ] if element.ty == bool_type
            && matches!(
                body.as_slice(),
                [ArtifactAction::IfElse {
                    then_actions,
                    else_actions,
                    ..
                }] if matches!(
                        then_actions.as_slice(),
                        [
                            ArtifactAction::Emit { .. },
                            ArtifactAction::Send {
                                payload: Some(ArtifactValueTemplate::LoopElement {
                                    ty,
                                    element: payload_element,
                                }),
                                ..
                            },
                        ] if *ty == bool_type && *payload_element == element.id
                    ) && else_actions.is_empty()
            )
    ));
    assert!(
        !artifact
            .encode()
            .lines()
            .any(|line| line.ends_with("=item") || line.contains("debug_name=item")),
        "loop no-op branch artifact must not dispatch through the source loop binding name"
    );

    let trace = gate.read_trace(STEM);
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"then""#,
            r#""scope":"action""#,
            r#""branch_path":[1,12288]"#,
            r#""loop_element_id":0"#,
            r#""loop_index":0"#,
            r#""condition":"True""#,
        ],
    );
    assert_trace_event(
        &trace,
        &[
            r#""event":"branch_selected""#,
            r#""process":"BatchWorker""#,
            r#""branch":"else""#,
            r#""scope":"action""#,
            r#""branch_path":[1,12288]"#,
            r#""loop_element_id":0"#,
            r#""loop_index":1"#,
            r#""condition":"False""#,
        ],
    );
    assert!(
        !trace.contains(
            r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":1,"payload":"False""#
        ),
        "selected no-op loop branch must not send the false item"
    );

    let first_iteration = trace_line_index(&trace, r#""event":"loop_iteration","pid":2"#);
    let then_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"then","scope":"action""#,
    );
    let true_send = trace_line_index(
        &trace,
        r#""event":"message_accepted","pid":3,"process_id":2,"process":"Worker","message_id":0,"message":"Branch","payload_type_id":"#,
    );
    let second_iteration = trace_line_index(&trace, r#""index":1,"element_type_id""#);
    let else_noop_branch = trace_line_index(
        &trace,
        r#""event":"branch_selected","pid":2,"process_id":1,"process":"BatchWorker","message_id":0,"message":"Batch","branch":"else","scope":"action""#,
    );
    let loop_complete = trace_line_index(
        &trace,
        r#""event":"loop_completed","pid":2,"process_id":1,"process":"BatchWorker""#,
    );
    let worker_true_output = trace_line_index(
        &trace,
        r#""event":"program_output","pid":3,"process_id":2,"process":"Worker","stream":"stdout","output_id":1,"text":"worker handled true""#,
    );

    assert!(first_iteration < then_branch);
    assert!(then_branch < true_send);
    assert!(true_send < second_iteration);
    assert!(second_iteration < else_noop_branch);
    assert!(else_noop_branch < loop_complete);
    assert!(loop_complete < worker_true_output);
}
