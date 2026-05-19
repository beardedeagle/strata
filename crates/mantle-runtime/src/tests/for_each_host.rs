use super::support::*;

#[test]
fn in_memory_host_executes_for_each_loop_in_collection_order() {
    let artifact = for_each_artifact("List[Job{phase:Ready},Job{phase:Done}]", 2);
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("for_each artifact should run through in-memory host");

    assert_eq!(
        report.emitted_outputs,
        ["loop worker handled item", "loop worker handled item"]
    );

    let iterations = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::LoopIteration {
                element_id,
                index,
                element_type_id,
                element,
                ..
            } => Some((*element_id, *index, *element_type_id, element.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        iterations,
        [
            (LoopElementId::new(0), 0, JOB, "Job{phase:Ready}"),
            (LoopElementId::new(0), 1, JOB, "Job{phase:Done}"),
        ]
    );

    let accepted_payloads = host
        .events()
        .iter()
        .filter_map(|event| match event {
            RuntimeEvent::MessageAccepted {
                process,
                payload: Some(payload),
                ..
            } if process == "Worker" => Some(payload.label()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_payloads, ["Job{phase:Ready}", "Job{phase:Done}"]);

    let first_iteration = event_index(host.events(), |event| {
        matches!(event, RuntimeEvent::LoopIteration { index: 0, .. })
    });
    let first_send = event_index(host.events(), |event| {
        matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process,
                payload: Some(payload),
                ..
            } if process == "Worker" && payload.label() == "Job{phase:Ready}"
        )
    });
    let second_iteration = event_index(host.events(), |event| {
        matches!(event, RuntimeEvent::LoopIteration { index: 1, .. })
    });
    let second_send = event_index(host.events(), |event| {
        matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process,
                payload: Some(payload),
                ..
            } if process == "Worker" && payload.label() == "Job{phase:Done}"
        )
    });
    let completed = event_index(host.events(), |event| {
        matches!(
            event,
            RuntimeEvent::LoopCompleted {
                element_id,
                iteration_count: 2,
                ..
            } if *element_id == LoopElementId::new(0)
        )
    });

    assert!(first_iteration < first_send);
    assert!(first_send < second_iteration);
    assert!(second_iteration < second_send);
    assert!(second_send < completed);
}

#[test]
fn in_memory_host_executes_empty_for_each_loop_without_body_iterations() {
    let artifact = for_each_artifact("List[]", 0);
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("empty for_each artifact should run through in-memory host");

    assert!(report.emitted_outputs.is_empty());
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::LoopStarted {
            item_count: 0,
            max_items: 0,
            ..
        }
    )));
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopIteration { .. }))
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::LoopCompleted {
            iteration_count: 0,
            ..
        }
    )));
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process,
                payload: Some(_),
                ..
            } if process == "Worker"
        )),
        "empty loop must not execute body sends"
    );
}

#[test]
fn in_memory_host_fails_closed_when_for_each_iteration_budget_exhausts() {
    let artifact = for_each_artifact("List[Job{phase:Ready},Job{phase:Done}]", 2);
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(
        &artifact,
        &mut host,
        RunLimits {
            max_dispatches: 1,
            ..RunLimits::default()
        },
    )
    .expect_err("for_each artifact should fail closed on iteration budget exhaustion");

    assert!(
        err.to_string()
            .contains(
                "runtime loop iteration budget exceeded: loop requires 2 iteration(s), remaining budget is 1"
            ),
        "{err}"
    );
    assert_eq!(
        host.events()
            .iter()
            .filter(|event| matches!(event, RuntimeEvent::LoopIteration { .. }))
            .count(),
        0
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopStarted { .. })),
        "over-budget loop must fail before loop start"
    );
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process,
                payload: Some(_),
                ..
            } if process == "Worker"
        )),
        "over-budget loop must not execute body sends"
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopCompleted { .. })),
        "failed loop must not record completion"
    );
}

#[test]
fn in_memory_host_fails_closed_when_for_each_sends_exceed_mailbox_bound() {
    let mut artifact = for_each_artifact("List[Job{phase:Ready},Job{phase:Done}]", 2);
    artifact.processes[1].mailbox_bound = 1;

    assert_for_each_mailbox_overflow_fails_before_loop(&artifact);
}

#[test]
fn in_memory_host_fails_closed_when_selected_loop_branch_sends_exceed_mailbox_bound() {
    let mut artifact = for_each_artifact("List[Job{phase:Ready},Job{phase:Done}]", 2);
    artifact.processes[1].mailbox_bound = 1;
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0]
        .effects
        .push(ArtifactEffect::Emit);

    let ArtifactAction::ForEach { body, .. } = &mut artifact.processes[0].transitions[0].actions[1]
    else {
        panic!("test artifact should keep for_each as the second action");
    };
    let send = body
        .pop()
        .expect("test for_each body should contain a send");
    body.push(ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![send],
        else_actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
    });

    assert_for_each_mailbox_overflow_fails_before_loop(&artifact);
}

fn assert_for_each_mailbox_overflow_fails_before_loop(artifact: &MantleArtifact) {
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(artifact, &mut host, RunLimits::default())
        .expect_err("for_each artifact should fail closed before mailbox overflow");

    assert!(
        err.to_string()
            .contains("mailbox for process Worker is full; message was not accepted"),
        "{err}"
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopStarted { .. })),
        "mailbox overflow preflight must fail before loop start"
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopIteration { .. })),
        "mailbox overflow preflight must fail before any iteration"
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::BranchSelected { .. })),
        "mailbox overflow preflight must not record branch selection"
    );
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process,
                ..
            } if process == "Worker"
        )),
        "mailbox overflow preflight must not execute body sends"
    );
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::LoopCompleted { .. })),
        "failed loop must not record completion"
    );
    assert!(
        host.stdout().is_empty(),
        "failed loop must not run target process effects"
    );
}
