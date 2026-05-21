use super::super::support::*;

#[test]
fn runtime_rejects_loaded_next_state_if_else_above_terminal_limit_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state =
        nested_loaded_if_else_next_state(MAX_NEXT_STATE_IF_ELSE_DEPTH + 1, bool_type);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "next_state runtime if nesting exceeds maximum depth of 2",
    );
}

#[test]
fn runtime_rejects_loaded_unknown_next_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        loaded_next_state(NextState::Value(StateId::new(1)));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 current_state id 0 next_state id 1 is not a loaded state value",
    );
}

#[test]
fn runtime_rejects_loaded_unadmitted_template_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Literal {
            ty: MAIN_STATE,
            value: artifact_value("UnadmittedState"),
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 current_state id 0 next_state_template produced value UnadmittedState not admitted by loaded state table",
    );
}

#[test]
fn runtime_rejects_loaded_literal_template_outside_declared_enum_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: artifact_value("Idle"),
        });
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Literal {
            ty: WORKER_STATE,
            value: artifact_value("Bogus"),
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 next_state_template value Bogus is not a member of enum type WorkerState",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_literal_template_shape_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::Literal {
            ty: MAIN_STATE,
            value: RuntimeValue::Atom("not-valid".to_string()),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "next_state_template must be an identifier",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_enum_payload_projection_variant_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_type = WORKER_STATE;
    program.processes[0].state_values = loaded_state_values(WORKER_STATE, &["Idle"]);
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::EnumPayload {
            ty: WORKER_STATE,
            value: Box::new(LoadedValueTemplate::Literal {
                ty: WORKER_STATE,
                value: RuntimeValue::Atom("Idle".to_string()),
            }),
            variant: EnumVariantId::new(99),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "next_state_template.variant_id loaded type id 2 has no enum variant id 99",
    );
}

#[test]
fn runtime_rejects_loaded_process_ref_payload_enum_next_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[WORKER_STATE.index()] = worker_state_type_with_payloads(&[
        ("Idle", None),
        ("Handled", None),
        ("Working", None),
        ("Done", None),
        ("Routed", Some(PROCESS_REF_WORKER)),
    ]);
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[1].transitions[0].current_state = Some(StateId::new(0));
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: WORKER_STATE,
            variant: EnumVariantId::new(4),
            payload: Box::new(ArtifactValueTemplate::ReceivedPayload {
                ty: PROCESS_REF_WORKER,
            }),
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 current_state id 0 next_state_template.payload process reference template must be a direct message payload",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_template_field_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[BOX.index()] = box_record_type("item", LEAF);
    program.processes[0].state_type = BOX;
    program.processes[0].state_values = loaded_state_values(BOX, &["Box{item:Leaf}"]);
    program.processes[0].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Record {
            ty: BOX,
            fields: vec![ArtifactValueTemplateField {
                name: "item".to_string(),
                value: ArtifactValueTemplate::Literal {
                    ty: TypeId::new(99),
                    value: artifact_value("Item"),
                },
            }],
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "loaded type id 99 is not loaded",
    );
}

#[test]
fn runtime_rejects_loaded_template_depth_overflow_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[MAIN_STATE.index()] = recursive_main_state_type();
    program.processes[0].state_values = loaded_state_values(MAIN_STATE, &["Leaf"]);
    program.processes[0].transitions[0].next_state = loaded_next_state(NextState::Template(
        recursive_main_state_template_with_depth(MAX_VALUE_TEMPLATE_DEPTH + 2),
    ));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "exceeds maximum value template depth",
    );
}
