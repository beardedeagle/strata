use super::support::*;

#[test]
fn runtime_rejects_loaded_process_ref_state_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_type = PROCESS_REF_WORKER;

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "state_type type id 8 must be a value type",
    );
}

#[test]
fn runtime_rejects_loaded_payload_bearing_entry_message_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].message_variants[0].payload_type = Some(START_PAYLOAD);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "entry message id 0 must not require a payload",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_message_payload_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(TypeId::new(99));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message payload_type: loaded type id 99 is not loaded",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_init_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].init_state = StateId::new(1);

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main init_state id 1 is not a loaded state value",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_state_value_shape_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_values[0].value = RuntimeValue::Atom("not-valid".to_string());

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "artifact field state value must be an identifier",
    );
}

#[test]
fn runtime_rejects_loaded_state_value_label_mismatch_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].state_values[0].label = "Spoofed".to_string();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main state value label Spoofed does not match ordered value label MainState",
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
fn runtime_rejects_loaded_process_ref_payload_enum_next_state_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    program.processes[1].transitions[0].current_state = Some(StateId::new(0));
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::EnumVariant {
            ty: WORKER_STATE,
            variant: "Routed".to_string(),
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
fn runtime_rejects_loaded_payload_dependent_map_template_key_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].message_variants[0].payload_type = Some(JOB);
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Map {
            ty: WORKER_STATE,
            entries: vec![mantle_artifact::ArtifactValueTemplateMapEntry {
                key: ArtifactValueTemplate::ReceivedPayload { ty: JOB },
                value: ArtifactValueTemplate::Literal {
                    ty: JOB,
                    value: artifact_value("Job"),
                },
            }],
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 next_state_template.entry.0.key must be a static value template",
    );
}

#[test]
fn runtime_rejects_loaded_duplicate_static_map_template_key_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Map {
            ty: WORKER_STATE,
            entries: vec![
                mantle_artifact::ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Ready"),
                    },
                },
                mantle_artifact::ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Done"),
                    },
                },
            ],
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 next_state_template duplicates key Job",
    );
}

#[test]
fn runtime_rejects_loaded_duplicate_map_projection_keys_before_artifact_loaded() {
    let template = ArtifactValueTemplate::MapValue {
        ty: JOB,
        map: Box::new(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Map[Ready=>Done]"),
        }),
        key: artifact_value("Ready"),
        keys: vec![artifact_value("Ready"), artifact_value("Ready")],
        projection: mantle_artifact::MapProjectionMode::Exact,
    };

    let err = LoadedValueTemplate::from_artifact(&template)
        .expect_err("duplicate map projection keys should fail loaded admission");

    assert!(
        err.to_string()
            .contains("map projection duplicates expected map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn runtime_rejects_loaded_duplicate_map_rest_projection_keys_before_artifact_loaded() {
    let template = ArtifactValueTemplate::MapRest {
        ty: JOB,
        map: Box::new(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Map[Ready=>Done]"),
        }),
        excluded_keys: vec![artifact_value("Ready"), artifact_value("Ready")],
    };

    let err = LoadedValueTemplate::from_artifact(&template)
        .expect_err("duplicate map rest keys should fail loaded admission");

    assert!(
        err.to_string()
            .contains("map rest projection duplicates excluded map key Ready"),
        "unexpected error: {err}"
    );
}

#[test]
fn runtime_rejects_loaded_unsorted_map_projection_keys_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::MapValue {
            ty: MAIN_STATE,
            map: Box::new(LoadedValueTemplate::Literal {
                ty: MAIN_STATE,
                value: artifact_value("Map[Done=>Done,Ready=>Ready]"),
            }),
            key: artifact_value("Ready"),
            keys: vec![artifact_value("Ready"), artifact_value("Done")],
            projection: mantle_artifact::MapProjectionMode::Subset,
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "next_state_template expected map keys must be sorted canonically",
    );
}

#[test]
fn runtime_rejects_loaded_invalid_template_field_type_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Record {
            ty: MAIN_STATE,
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
    program.processes[0].transitions[0].next_state = loaded_next_state(NextState::Template(
        record_template_with_depth(MAX_VALUE_TEMPLATE_DEPTH + 2),
    ));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "exceeds maximum value template depth",
    );
}

#[test]
fn runtime_rejects_loaded_unknown_emit_output_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(&program, "output id 0 is not loaded");
}
