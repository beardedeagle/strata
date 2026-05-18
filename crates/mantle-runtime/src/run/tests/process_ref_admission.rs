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
            payload: Some(loaded_template(ArtifactValueTemplate::ProcessRef {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(0),
                process_ref: ProcessRefId::new(0),
            })),
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
    program.types[BOX.index()] = box_record_type("reply_to", MAIN_STATE);
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
            payload: Some(loaded_template(ArtifactValueTemplate::RecordField {
                ty: PROCESS_REF_WORKER,
                record: Box::new(ArtifactValueTemplate::Literal {
                    ty: BOX,
                    value: artifact_value("Box{reply_to:ProcessRef_Worker}"),
                }),
                field: "reply_to".to_string(),
            })),
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
fn runtime_rejects_loaded_process_ref_payload_guard_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[1].transitions[0].payload_guard = Some(
        RuntimePayload::from_artifact(
            &ArtifactPayload::process_ref(PROCESS_REF_WORKER, ProcessId::new(1), 2)
                .expect("test process-ref payload should construct"),
        )
        .expect("test runtime process-ref payload should load"),
    );

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 payload guard cannot be a process reference payload",
    );
}

#[test]
fn runtime_rejects_loaded_process_ref_type_payload_guard_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[1].transitions[0].payload_guard = Some(
        RuntimePayload::from_artifact(
            &ArtifactPayload::value(PROCESS_REF_WORKER, artifact_value("Plain"))
                .expect("test plain process-ref-typed payload should construct"),
        )
        .expect("test runtime payload should load"),
    );

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 payload guard type id 8 must be a value type",
    );
}

#[test]
fn runtime_rejects_unspawned_process_ref_payload() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].message_variants =
        vec![ArtifactMessageVariant::payload("Ping", PROCESS_REF_WORKER)];
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let mut host = InMemoryRuntimeHost::default();
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());
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
                Some(runtime_payload(ArtifactPayload {
                    ty: PROCESS_REF_WORKER,
                    value: ArtifactValue::process_ref(PROCESS_REF_WORKER, 99),
                    process_ref: Some(ArtifactProcessRefPayload {
                        target_process: ProcessId::new(1),
                        pid: 99,
                    }),
                })),
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
    let mut run = RuntimeRun::new(&program, &mut host, RunLimits::default());
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
                Some(runtime_payload(ArtifactPayload {
                    ty: PROCESS_REF_WORKER,
                    value: ArtifactValue::process_ref(PROCESS_REF_WORKER, main_pid.as_u64()),
                    process_ref: Some(ArtifactProcessRefPayload {
                        target_process: ProcessId::new(0),
                        pid: main_pid.as_u64(),
                    }),
                })),
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
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let list_ty = push_list_type(&mut program, "LeafList", LEAF, 127);
    program.types[BOX.index()] = box_record_type("item", list_ty);
    let template = ArtifactValueTemplate::Record {
        ty: BOX,
        fields: vec![ArtifactValueTemplateField {
            name: "item".to_string(),
            value: ArtifactValueTemplate::ReceivedPayload { ty: list_ty },
        }],
    };
    let long_atom = RuntimeValue::Atom(format!("A{}", "a".repeat(MAX_IDENTIFIER_BYTES - 1)));
    let short_atom = RuntimeValue::Atom(format!("A{}", "a".repeat(MAX_IDENTIFIER_BYTES - 5)));
    let mut items = vec![long_atom; 126];
    items.push(short_atom);
    let received = RuntimePayload::value(list_ty, RuntimeValue::List(items))
        .expect("test payload should fit exactly");
    let label: &str = received.label();
    assert_eq!(label.len(), MAX_FIELD_VALUE_BYTES);
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
        &loaded_template(template),
        Some(&received),
        &step,
        &BTreeMap::new(),
        &[],
    )
    .expect_err("oversized record payload labels should fail closed");

    assert!(
        err.to_string()
            .contains("payload value exceeds maximum length")
    );
}

#[test]
fn runtime_payload_value_rejects_invalid_structured_value_shape() {
    let err = RuntimePayload::value(JOB, RuntimeValue::Atom("not-valid".to_string()))
        .expect_err("invalid runtime payload shape should fail");

    assert!(
        err.to_string()
            .contains("artifact field payload value must be an identifier"),
        "unexpected error: {err}"
    );
}
