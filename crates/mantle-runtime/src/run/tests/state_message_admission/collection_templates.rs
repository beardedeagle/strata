use super::super::support::*;

#[test]
fn runtime_rejects_loaded_payload_dependent_map_template_key_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let map_ty = push_map_type(&mut program, "JobMap", JOB, JOB, 1);
    program.processes[1].state_type = map_ty;
    program.processes[1].state_values = loaded_state_values(map_ty, &["Map[]"]);
    program.processes[1].message_variants[0].payload_type = Some(JOB);
    align_loaded_process_message_type(&mut program, 1);
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Map {
            ty: map_ty,
            entries: vec![mantle_artifact::ArtifactValueTemplateMapEntry {
                key: ArtifactValueTemplate::ReceivedPayload { ty: JOB },
                value: ArtifactValueTemplate::Literal {
                    ty: JOB,
                    value: artifact_value("Job{phase:Ready}"),
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
    let map_ty = push_map_type(&mut program, "JobMap", JOB, JOB, 2);
    program.processes[1].state_type = map_ty;
    program.processes[1].state_values = loaded_state_values(map_ty, &["Map[]"]);
    program.processes[1].transitions[0].next_state =
        loaded_next_state(NextState::Template(ArtifactValueTemplate::Map {
            ty: map_ty,
            entries: vec![
                mantle_artifact::ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job{phase:Ready}"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job{phase:Done}"),
                    },
                },
                mantle_artifact::ArtifactValueTemplateMapEntry {
                    key: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job{phase:Ready}"),
                    },
                    value: ArtifactValueTemplate::Literal {
                        ty: JOB,
                        value: artifact_value("Job{phase:Ready}"),
                    },
                },
            ],
        }));

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 next_state_template duplicates key Job{phase:Ready}",
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
fn runtime_rejects_loaded_zero_list_rest_prefix_before_artifact_loaded() {
    let template = ArtifactValueTemplate::ListRest {
        ty: JOB,
        list: Box::new(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("List[Ready]"),
        }),
        prefix_len: 0,
    };

    let err = LoadedValueTemplate::from_artifact(&template)
        .expect_err("zero list rest prefix should fail loaded admission");

    assert!(
        err.to_string()
            .contains("list rest projection.prefix_len must be between 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn runtime_rejects_loaded_list_prefix_index_outside_prefix_before_artifact_loaded() {
    let template = ArtifactValueTemplate::ListPrefixElement {
        ty: JOB,
        list: Box::new(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("List[Ready,Done]"),
        }),
        index: 1,
        prefix_len: 1,
    };

    let err = LoadedValueTemplate::from_artifact(&template)
        .expect_err("outside-prefix list element should fail loaded admission");

    assert!(
        err.to_string()
            .contains("list prefix projection.index 1 is outside list prefix length 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn runtime_rejects_loaded_unsorted_map_projection_keys_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let map_ty = push_map_type(&mut program, "JobMap", JOB, JOB, 2);
    program.processes[0].state_type = JOB;
    program.processes[0].state_values = loaded_state_values(JOB, &["Job{phase:Ready}"]);
    program.processes[0].transitions[0].current_state = Some(StateId::new(0));
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::MapValue {
            ty: JOB,
            map: Box::new(LoadedValueTemplate::Literal {
                ty: map_ty,
                value: artifact_value(
                    "Map[Job{phase:Done}=>Job{phase:Done},Job{phase:Ready}=>Job{phase:Ready}]",
                ),
            }),
            key: artifact_value("Job{phase:Ready}"),
            keys: vec![
                artifact_value("Job{phase:Ready}"),
                artifact_value("Job{phase:Done}"),
            ],
            projection: mantle_artifact::MapProjectionMode::Subset,
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "next_state_template expected map keys must be sorted canonically",
    );
}
