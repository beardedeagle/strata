use super::super::support::*;

#[test]
fn artifact_round_trips_for_each_control_flow() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 2);
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: JOB,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: job_list,
                value: artifact_value("List[Ready,Done]"),
            },
            max_items: 2,
            body: vec![ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::LoopElement {
                    ty: JOB,
                    element: LoopElementId::new(0),
                }),
            }],
        },
    ];

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("for_each artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.action.1.kind=for_each"));
    assert!(encoded.contains("process.0.transition.0.action.1.loop_element=0"));
    assert!(encoded.contains("process.0.transition.0.action.1.collection.kind=literal"));
    assert!(encoded.contains("process.0.transition.0.action.1.body_action.0.kind=send"));
    assert!(encoded.contains(
        "process.0.transition.0.action.1.body_action.0.payload_template.kind=loop_element"
    ));
}

#[test]
fn admission_rejects_direct_process_ref_payload_inside_for_each_loop_body() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 1);
    artifact.processes[1].message_variants[0].payload_type = Some(PROCESS_REF_WORKER);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::ProcessRef {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }),
        },
    ];
    artifact.processes[1].transitions[0].effects = vec![ArtifactEffect::Send];
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: job_list,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::Send {
            target: ArtifactSendTarget::ReceivedPayload {
                ty: PROCESS_REF_WORKER,
                target_process: ProcessId::new(1),
            },
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::ReceivedPayload {
                ty: PROCESS_REF_WORKER,
            }),
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("loop body direct process ref payload should fail admission");

    assert!(err.to_string().contains(
        "process Worker transition 0 send payload process reference template must be a direct message payload"
    ));
}

#[test]
fn admission_accepts_if_else_inside_for_each_loop_body() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 2);
    artifact.processes[1].message_variants[0].payload_type = Some(bool_type);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].effects = vec![
        ArtifactEffect::Spawn,
        ArtifactEffect::Emit,
        ArtifactEffect::Send,
    ];
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: list_type,
                value: artifact_value("List[True,False]"),
            },
            max_items: 2,
            body: vec![ArtifactAction::IfElse {
                condition: ArtifactValueTemplate::LoopElement {
                    ty: bool_type,
                    element: LoopElementId::new(0),
                },
                then_actions: vec![
                    ArtifactAction::Emit {
                        output: OutputId::new(0),
                    },
                    ArtifactAction::Send {
                        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                        message: MessageId::new(0),
                        payload: Some(ArtifactValueTemplate::LoopElement {
                            ty: bool_type,
                            element: LoopElementId::new(0),
                        }),
                    },
                ],
                else_actions: vec![ArtifactAction::Send {
                    target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                    message: MessageId::new(0),
                    payload: Some(ArtifactValueTemplate::LoopElement {
                        ty: bool_type,
                        element: LoopElementId::new(0),
                    }),
                }],
            }],
        },
    ];

    artifact
        .validate()
        .expect("if_else inside for_each loop body should admit");
}

#[test]
fn admission_rejects_loop_branch_condition_without_bool_contract() {
    let mut artifact = valid_artifact();
    let non_contract_bool = TypeId::from_index(artifact.types.len()).expect("test type id fits");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["No".to_string(), "Yes".to_string()],
    ));
    let list_type = append_list_type(&mut artifact, "BoolList", non_contract_bool, 1);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: non_contract_bool,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[Yes]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::LoopElement {
                ty: non_contract_bool,
                element: LoopElementId::new(0),
            },
            then_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("loop branch condition must require Bool contract");
    assert!(
        err.to_string()
            .contains("if condition must have type enum Bool { False, True }"),
        "{err}"
    );
}

#[test]
fn admission_rejects_loop_branch_condition_with_non_unit_bool_shape() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::Record {
                ty: bool_type,
                fields: vec![ArtifactValueTemplateField {
                    field: RecordFieldId::new(0),
                    value: ArtifactValueTemplate::LoopElement {
                        ty: bool_type,
                        element: LoopElementId::new(0),
                    },
                }],
            },
            then_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("loop branch condition must produce unit Bool atom");
    assert!(
        err.to_string()
            .contains("if condition must evaluate to unit Bool value False or True"),
        "{err}"
    );
}

#[test]
fn admission_rejects_if_else_nesting_above_limit_inside_for_each_loop_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    let condition = ArtifactValueTemplate::LoopElement {
        ty: bool_type,
        element: LoopElementId::new(0),
    };
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![ArtifactAction::IfElse {
                condition: condition.clone(),
                then_actions: vec![ArtifactAction::IfElse {
                    condition,
                    then_actions: Vec::new(),
                    else_actions: Vec::new(),
                }],
                else_actions: vec![ArtifactAction::Emit {
                    output: OutputId::new(0),
                }],
            }],
            else_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("too-deep if_else inside loop branch should fail admission");
    assert!(
        err.to_string()
            .contains("runtime if action nesting exceeds maximum depth"),
        "{err}"
    );
}

#[test]
fn admission_accepts_one_empty_if_else_branch_inside_for_each_loop_body() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].authorities = Vec::new();
    artifact.processes[0].spawn_sites = Vec::new();
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::LoopElement {
                ty: bool_type,
                element: LoopElementId::new(0),
            },
            then_actions: Vec::new(),
            else_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    artifact
        .validate()
        .expect("one empty loop if_else branch should admit");
}

#[test]
fn admission_rejects_both_empty_if_else_branches_inside_for_each_loop_body() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: ArtifactValueTemplate::LoopElement {
                ty: bool_type,
                element: LoopElementId::new(0),
            },
            then_actions: Vec::new(),
            else_actions: Vec::new(),
        }],
    }];

    let err = artifact
        .validate()
        .expect_err("both empty loop if_else branches should fail admission");
    assert!(
        err.to_string()
            .contains("runtime if action branches cannot both be empty"),
        "{err}"
    );
}

#[test]
fn decode_rejects_missing_for_each_body_action() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 1);
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: job_list,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(ArtifactValueTemplate::LoopElement {
                ty: JOB,
                element: LoopElementId::new(0),
            }),
        }],
    }];
    let encoded = artifact.encode().replace(
        "process.0.transition.0.action.0.body_action.0.kind=send\n",
        "",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("missing loop body action should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field process.0.transition.0.action.0.body_action.0.kind")
    );
}

#[test]
fn admission_rejects_inactive_for_each_loop_element_payload() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 1);
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions = vec![
        ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_WORKER_SITE,
        },
        ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: JOB,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: job_list,
                value: artifact_value("List[Ready]"),
            },
            max_items: 1,
            body: vec![ArtifactAction::Send {
                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                message: MessageId::new(0),
                payload: Some(ArtifactValueTemplate::LoopElement {
                    ty: JOB,
                    element: LoopElementId::new(1),
                }),
            }],
        },
    ];

    let err = artifact
        .validate()
        .expect_err("inactive loop element should fail admission");

    assert!(
        err.to_string()
            .contains("references inactive loop element id 1"),
        "{err}"
    );
}

#[test]
fn admission_rejects_static_for_each_non_list_collection() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Ready"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("static non-list for_each collection should fail admission");

    assert!(
        err.to_string()
            .contains("process Main transition 0 for collection type id 4 must be a list type"),
        "{err}"
    );
}

#[test]
fn admission_rejects_dynamic_for_each_non_list_collection() {
    let mut artifact = valid_artifact();
    artifact.processes[1].message_variants[0].payload_type = Some(JOB);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: JOB,
            value: artifact_value("Ready"),
        }),
    };
    artifact.processes[1].transitions[0].effects = Vec::new();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::ReceivedPayload { ty: JOB },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("dynamic non-list for_each collection should fail admission");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 for collection type id 4 must be a list type"),
        "{err}"
    );
}

#[test]
fn admission_rejects_dynamic_for_each_collection_element_type_mismatch() {
    let mut artifact = valid_artifact();
    let other_job_list = append_list_type(&mut artifact, "OtherJobList", OTHER_JOB, 1);
    artifact.processes[1].message_variants[0].payload_type = Some(other_job_list);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[0].transitions[0].actions[1] = ArtifactAction::Send {
        target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
        message: MessageId::new(0),
        payload: Some(ArtifactValueTemplate::Literal {
            ty: other_job_list,
            value: artifact_value("List[Ready]"),
        }),
    };
    artifact.processes[1].transitions[0].effects = Vec::new();
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::ReceivedPayload { ty: other_job_list },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("for_each collection element type mismatch should fail admission");

    assert!(
        err.to_string()
            .contains("process Worker transition 0 for collection element type id 5, expected 4"),
        "{err}"
    );
}

#[test]
fn admission_rejects_static_for_each_item_outside_declared_enum_variants() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 2);
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True,Maybe]"),
        },
        max_items: 2,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("static for_each item outside enum variants should fail admission");

    assert!(
        err.to_string()
            .contains("for collection.item.1 value Maybe is not a member of enum type Bool"),
        "{err}"
    );
}

#[test]
fn admission_rejects_for_each_process_ref_element_type() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 1);
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: PROCESS_REF_WORKER,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: job_list,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("process-ref for_each element should fail admission");

    assert!(
        err.to_string()
            .contains("artifact field for loop element type type id 9 must be a value type"),
        "{err}"
    );
}

#[test]
fn admission_rejects_for_each_loop_element_id_above_codec_bound() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 1);
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(MAX_VALUE_TEMPLATE_FIELDS as u32),
            ty: JOB,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: job_list,
            value: artifact_value("List[Ready]"),
        },
        max_items: 1,
        body: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("loop element ids above the codec bound should fail admission");

    assert!(
        err.to_string()
            .contains("for loop element id must be no greater than"),
        "{err}"
    );
}
