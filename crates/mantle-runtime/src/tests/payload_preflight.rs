use super::support::*;

#[test]
fn in_memory_host_delivers_payload_envelopes_and_template_state() {
    let artifact = payload_artifact();
    let expected_payload =
        RuntimePayload::from_artifact(&artifact_payload(JOB, "Job{phase:Ready}"))
            .expect("expected payload should load");
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("payload artifact should run through in-memory host");

    assert_eq!(report.emitted_outputs, ["worker assigned job"]);
    assert!(
        report
            .delivered_messages
            .iter()
            .any(|delivery| delivery.process == "Worker"
                && delivery.message == "Assign(Job{phase:Ready})")
    );
    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "Working(Job{phase:Ready})"
                && process.status == ProcessStatus::Stopped)
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageAccepted {
            process,
            message,
            payload: Some(payload),
            ..
        } if process == "Worker" && message == "Assign" && payload == &expected_payload
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageDequeued {
            process,
            message,
            payload: Some(payload),
            ..
        } if process == "Worker" && message == "Assign" && payload == &expected_payload
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process,
            message,
            payload: Some(payload),
            state,
            ..
        } if process == "Worker"
            && message == "Assign"
            && payload == &expected_payload
            && state == "Working(Job{phase:Ready})"
    )));
}

#[test]
fn runtime_preflights_template_state_before_process_outputs() {
    let mut artifact = payload_artifact();
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Working(Job{phase:Done})"]);
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("unadmitted dynamic template state should fail");

    assert!(err.to_string().contains(
        "process Worker next_state template produced value Working(Job{phase:Ready}) not admitted by state table"
    ));
    assert!(
        host.stdout().is_empty(),
        "worker output must not be emitted after invalid template state"
    );
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::ProgramOutput { process, .. } if process == "Worker"
        )),
        "worker program output event must not be recorded after invalid template state"
    );
}

#[test]
fn runtime_rejects_state_value_label_mismatch_before_trace() {
    let mut artifact = payload_artifact();
    artifact.processes[1].state_values[1] = ArtifactStateValue {
        ty: WORKER_STATE,
        value: artifact_value("Working(Job{phase:Spoofed})"),
        label: "Working(Job{phase:Ready})".to_string(),
        payload: None,
    };
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("state label mismatch should fail before runtime trace");

    assert!(err.to_string().contains(
        "state value label Working(Job{phase:Ready}) does not match ordered value label Working(Job{phase:Spoofed})"
    ));
    assert!(
        host.events().is_empty(),
        "state label mismatch must fail before ArtifactLoaded"
    );
}

#[test]
fn in_memory_host_selects_transitions_by_message_id() {
    let artifact = sequence_artifact();
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("sequence artifact should run through in-memory host");

    assert_eq!(
        report.emitted_outputs,
        ["worker handled First", "worker handled Second"]
    );
    assert!(
        report
            .processes
            .iter()
            .any(|process| process.process == "Worker"
                && process.state == "Done"
                && process.status == ProcessStatus::Stopped)
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process_id,
            process,
            message_id,
            message,
            result: RuntimeStepResult::Continue,
            state_id,
            state,
            ..
        } if *process_id == ProcessId::new(1)
            && process == "Worker"
            && *message_id == MessageId::new(0)
            && message == "First"
            && *state_id == StateId::new(1)
            && state == "SawFirst"
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::ProcessStepped {
            process_id,
            process,
            message_id,
            message,
            result: RuntimeStepResult::Stop,
            state_id,
            state,
            ..
        } if *process_id == ProcessId::new(1)
            && process == "Worker"
            && *message_id == MessageId::new(1)
            && message == "Second"
            && *state_id == StateId::new(2)
            && state == "Done"
    )));
}
