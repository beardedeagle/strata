use super::support::*;

#[test]
fn runtime_rejects_loaded_spawn_target_mismatch_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Spawn]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Spawn {
            target: ProcessId::new(0),
            process_ref: ProcessRefId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 spawn process reference id 0 targets process id 0, expected 1",
    );
}

#[test]
fn runtime_rejects_loaded_send_before_spawn_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Send]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: None,
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 sends through unbound process reference id 0",
    );
}

#[test]
fn runtime_rejects_loaded_send_missing_payload_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(JOB);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Spawn,
            ArtifactEffect::Send,
        ]);
    program.processes[0].transitions[0].actions = vec![
        LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        },
        LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: None,
        },
    ];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 sends process id 1 message id 0 without required payload",
    );
}

#[test]
fn runtime_rejects_loaded_process_ref_payload_target_mismatch_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Spawn,
            ArtifactEffect::Send,
        ]);
    program.processes[0].transitions[0].actions = vec![
        LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        },
        LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::ProcessRef {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(0),
                process_ref: ProcessRefId::new(0),
            }),
        },
    ];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process reference payload type type id 8 targets process id 1, expected 0",
    );
}

#[test]
fn runtime_rejects_loaded_projected_process_ref_payload_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Spawn,
            ArtifactEffect::Send,
        ]);
    program.processes[0].transitions[0].actions = vec![
        LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        },
        LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::RecordField {
                ty: PROCESS_REF_WORKER,
                record: Box::new(ArtifactValueTemplate::Literal {
                    ty: BOX,
                    value: "Box{reply_to:ProcessRef_Worker}".to_string(),
                }),
                field: "reply_to".to_string(),
            }),
        },
    ];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 send payload process reference template must be a direct message payload",
    );
}

#[test]
fn runtime_rejects_loaded_received_process_ref_send_without_payload_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Send]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Send {
            target: LoadedSendTarget::ReceivedPayload {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(1),
            },
            message: MessageId::new(0),
            payload: None,
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 send target requires a payload-bearing message",
    );
}

#[test]
fn runtime_rejects_unspawned_process_ref_payload() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(
        &program,
        &mut host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    );
    let main_pid = run
        .spawn_process(ProcessId::new(0), None)
        .expect("entry process should spawn");
    let worker_pid = run
        .spawn_process(ProcessId::new(1), Some(main_pid))
        .expect("worker process should spawn");

    let err = run
        .send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                MessageId::new(0),
                Some(ArtifactPayload {
                    ty: PROCESS_REF_WORKER,
                    value: "type8#99".to_string(),
                    process_ref: Some(ArtifactProcessRefPayload {
                        target_process: ProcessId::new(1),
                        pid: 99,
                    }),
                }),
            ),
            Some(main_pid),
        )
        .expect_err("unspawned process ref payload should fail closed");

    assert!(
        err.to_string()
            .contains("runtime process 99 is not spawned")
    );
}

#[test]
fn runtime_rejects_process_ref_payload_target_type_mismatch() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(
        &program,
        &mut host,
        DEFAULT_MAX_RUNTIME_PROCESSES,
        DEFAULT_MAX_TRACE_BYTES,
        DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
    );
    let main_pid = run
        .spawn_process(ProcessId::new(0), None)
        .expect("entry process should spawn");
    let worker_pid = run
        .spawn_process(ProcessId::new(1), Some(main_pid))
        .expect("worker process should spawn");

    let err = run
        .send_message(
            worker_pid,
            RuntimeMessageEnvelope::new(
                MessageId::new(0),
                Some(ArtifactPayload {
                    ty: PROCESS_REF_WORKER,
                    value: "type8#1".to_string(),
                    process_ref: Some(ArtifactProcessRefPayload {
                        target_process: ProcessId::new(0),
                        pid: main_pid.as_u64(),
                    }),
                }),
            ),
            Some(main_pid),
        )
        .expect_err("process ref target type mismatch should fail closed");

    assert!(err.to_string().contains(
        "payload process reference metadata targets process id 0, expected 1 for type id 8"
    ));
}

#[test]
fn runtime_rejects_oversized_record_payload_template_value() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let template = ArtifactValueTemplate::Record {
        ty: BOX,
        fields: vec![ArtifactValueTemplateField {
            name: "item".to_string(),
            value: ArtifactValueTemplate::ReceivedPayload { ty: JOB },
        }],
    };
    let received = ArtifactPayload {
        ty: JOB,
        value: "a".repeat(MAX_FIELD_VALUE_BYTES),
        process_ref: None,
    };
    let step = ActiveStep {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(0),
        process_name: "Main".to_string(),
        message: MessageId::new(0),
        message_label: "Start".to_string(),
        payload: Some(received.clone()),
        current_state: StateId::new(0),
    };

    let err = evaluate_runtime_template(
        &program,
        &template,
        Some(&received),
        &step,
        &BTreeMap::new(),
    )
    .expect_err("oversized record payload labels should fail closed");

    assert!(
        err.to_string()
            .contains("payload value exceeds maximum length")
    );
}
