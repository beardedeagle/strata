use super::super::support::*;

#[test]
fn artifact_round_trips_and_validates_magic() {
    let artifact = valid_artifact();
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains(&format!("schema_version={ARTIFACT_SCHEMA_VERSION}")));
    assert!(encoded.contains("entry_process=0"));
    assert!(encoded.contains("type.2.label=WorkerState"));
    assert!(encoded.contains("process.1.state_value.1.type_id=2"));
    assert!(encoded.contains("process.1.state_value.1.value=Handled"));
    assert!(encoded.contains("process.1.state_value.1.label=Handled"));
    assert!(encoded.contains("process.0.transition.0.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_value=1"));
    assert!(encoded.contains("process.0.process_ref.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.effect_count=2"));
    assert!(encoded.contains("process.0.transition.0.effect.0=spawn"));
    assert!(encoded.contains("process.0.transition.0.effect.1=send"));
    assert!(encoded.contains("process.0.transition.0.action.0.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.action.0.process_ref=0"));
    assert!(encoded.contains("process.0.transition.0.action.1.target_process_ref=0"));

    let err = MantleArtifact::decode("not-mta\n").expect_err("bad magic should fail");
    assert!(err.to_string().contains("invalid Mantle artifact magic"));
}

#[test]
fn encode_value_type_without_shape_does_not_panic() {
    let mut artifact = valid_artifact();
    artifact.types[MAIN_STATE.index()] = ArtifactType {
        label: "MainState".to_string(),
        kind: ArtifactTypeKind::Value,
        shape: None,
    };

    let encoded = artifact.encode();
    let err = MantleArtifact::decode(&encoded)
        .expect_err("missing value type shape should decode fail closed");

    assert!(
        err.to_string()
            .contains("missing artifact field type.0.shape")
    );
}

#[test]
fn artifact_round_trips_enum_variant_metadata_above_template_field_limit() {
    let mut artifact = valid_artifact();
    let variants = (0..=MAX_VALUE_TEMPLATE_FIELDS)
        .map(|index| format!("V{index}"))
        .collect::<Vec<_>>();
    artifact.types[BOX.index()] = ArtifactType::enum_value("LargeEnum", variants);

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded)
        .expect("enum variant metadata above template-field limit should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains(&format!(
        "type.{}.enum_variant_count={}",
        BOX.index(),
        MAX_VALUE_TEMPLATE_FIELDS + 1
    )));
}

#[test]
fn artifact_round_trips_panic_step_result() {
    let mut artifact = valid_artifact();
    artifact.processes[1].transitions[0].step_result = StepResult::Panic;

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("panic artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.step_result=Panic"));
}

#[test]
fn artifact_round_trips_map_rest_template_with_excluded_keys() {
    let mut artifact = valid_artifact();
    let job_map = append_map_type(&mut artifact, "JobMap", JOB, JOB, 2);
    artifact.processes[0].state_type = job_map;
    artifact.processes[0].state_values = vec![state_value(job_map, "Map[Done=>Ready]")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::MapRest {
            ty: job_map,
            map: Box::new(ArtifactValueTemplate::Literal {
                ty: job_map,
                value: artifact_value("Map[Done=>Ready,Ready=>Done]"),
            }),
            excluded_keys: vec![artifact_value("Ready")],
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("map rest artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.next_state_template.kind=map_rest"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.excluded_key.0=Ready"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.expected_key.0"));
}

#[test]
fn artifact_round_trips_list_rest_template_with_prefix_len() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 2);
    artifact.processes[0].state_type = job_list;
    artifact.processes[0].state_values = vec![state_value(job_list, "List[Done]")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ListRest {
            ty: job_list,
            list: Box::new(ArtifactValueTemplate::Literal {
                ty: job_list,
                value: artifact_value("List[Ready,Done]"),
            }),
            prefix_len: 1,
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("list rest artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.next_state_template.kind=list_rest"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.prefix_len=1"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.expected_key.0"));
    assert!(!encoded.contains("process.0.transition.0.next_state_template.excluded_key.0"));
}

#[test]
fn artifact_round_trips_list_prefix_template_with_prefix_len() {
    let mut artifact = valid_artifact();
    let job_list = append_list_type(&mut artifact, "JobList", JOB, 2);
    artifact.processes[0].state_type = JOB;
    artifact.processes[0].state_values = vec![state_value(JOB, "Ready")];
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ListPrefixElement {
            ty: JOB,
            list: Box::new(ArtifactValueTemplate::Literal {
                ty: job_list,
                value: artifact_value("List[Ready,Done]"),
            }),
            index: 0,
            prefix_len: 1,
        });

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("list prefix artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(
        encoded.contains("process.0.transition.0.next_state_template.kind=list_prefix_element")
    );
    assert!(encoded.contains("process.0.transition.0.next_state_template.index=0"));
    assert!(encoded.contains("process.0.transition.0.next_state_template.prefix_len=1"));
}

#[test]
fn artifact_round_trips_if_else_control_flow() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let condition = ArtifactValueTemplate::Literal {
        ty: bool_type,
        value: artifact_value("True"),
    };
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: condition.clone(),
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };
    artifact.processes[1].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition,
        then_actions: vec![ArtifactAction::Emit {
            output: OutputId::new(0),
        }],
        else_actions: Vec::new(),
    }];

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("if_else artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.next_state=if_else"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.kind=literal"));
    assert!(encoded.contains("process.1.transition.0.next_state_then.next_state=value"));
    assert!(encoded.contains("process.1.transition.0.next_state_else.next_state=current"));
    assert!(encoded.contains("process.1.transition.0.action.0.kind=if_else"));
    assert!(encoded.contains("process.1.transition.0.action.0.then_action_count=1"));
    assert!(encoded.contains("process.1.transition.0.action.0.else_action_count=0"));
}

#[test]
fn artifact_round_trips_equality_condition_template() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let condition = ArtifactValueTemplate::Equality {
        ty: bool_type,
        operand_ty: bool_type,
        operator: ArtifactValueEqualityOperator::NotEqual,
        left: Box::new(ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        }),
        right: Box::new(ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("False"),
        }),
    };
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition,
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("equality artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.next_state_condition.kind=equality"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.operator=ne"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.left.kind=literal"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.right.kind=literal"));
}

#[test]
fn artifact_round_trips_boolean_predicate_condition_template() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let equality = ArtifactValueTemplate::Equality {
        ty: bool_type,
        operand_ty: bool_type,
        operator: ArtifactValueEqualityOperator::Equal,
        left: Box::new(ArtifactValueTemplate::ReceivedPayload { ty: bool_type }),
        right: Box::new(ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        }),
    };
    artifact.processes[1].message_variants[0].payload_type = Some(bool_type);
    align_process_message_type(&mut artifact, 1);
    match &mut artifact.processes[0].transitions[0].actions[1] {
        ArtifactAction::Send { payload, .. } => {
            *payload = Some(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            });
        }
        action => panic!("expected test fixture send action, got {action:?}"),
    }
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanBinary {
            ty: bool_type,
            operator: ArtifactValueBooleanOperator::And,
            left: Box::new(equality.clone()),
            right: Box::new(ArtifactValueTemplate::BooleanNot {
                ty: bool_type,
                operand: Box::new(equality),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let encoded = artifact.encode();
    let decoded =
        MantleArtifact::decode(&encoded).expect("boolean predicate artifact should decode");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.1.transition.0.next_state_condition.kind=boolean_binary"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.operator=and"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.left.kind=equality"));
    assert!(encoded.contains("process.1.transition.0.next_state_condition.right.kind=boolean_not"));
}
