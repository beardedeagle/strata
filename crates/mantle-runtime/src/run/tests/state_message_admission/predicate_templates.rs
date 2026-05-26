use super::super::support::*;

#[test]
fn runtime_rejects_loaded_if_else_literal_that_is_not_bool_value_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::Literal {
            ty: bool_type,
            value: RuntimeValue::EnumVariant {
                variant: "True".to_string(),
                payload: Box::new(RuntimeValue::Atom("Payload".to_string())),
            },
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_condition enum variant True must not carry a payload",
    );
}

#[test]
fn runtime_rejects_loaded_if_else_static_projection_that_is_not_bool_value_before_artifact_loaded()
{
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[BOX.index()] =
        ArtifactType::record("Box", vec![artifact_type_field("flag", bool_type)]);
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::RecordField {
            ty: bool_type,
            record: Box::new(LoadedValueTemplate::Literal {
                ty: BOX,
                value: RuntimeValue::Record {
                    constructor: "Box".to_string(),
                    fields: vec![ArtifactRecordField {
                        name: "flag".to_string(),
                        value: RuntimeValue::EnumVariant {
                            variant: "True".to_string(),
                            payload: Box::new(RuntimeValue::Atom("Payload".to_string())),
                        },
                    }],
                },
            }),
            field: "flag".to_string(),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_condition.record.field.flag enum variant True must not carry a payload",
    );
}

#[test]
fn runtime_rejects_loaded_if_else_dynamic_non_unit_bool_shape_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[1].message_variants[0].payload_type = Some(bool_type);
    align_process_message_type(&mut artifact, 1);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::ReceivedPayload { ty: bool_type },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::List {
            ty: bool_type,
            items: vec![LoadedValueTemplate::ReceivedPayload { ty: bool_type }],
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker message id 0 next_state_condition must evaluate to unit Bool value False or True",
    );
}

#[test]
fn runtime_rejects_loaded_equality_malformed_operand_type_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::Equality {
            ty: bool_type,
            operand_ty: MAIN_STATE,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
            right: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_condition.operand_type_id must be Bool, a scalar value type, or a fieldless enum value type",
    );
}

#[test]
fn runtime_rejects_loaded_equality_non_bool_result_type_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::Equality {
            ty: MAIN_STATE,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
            right: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_condition must have type enum Bool { False, True }",
    );
}

#[test]
fn runtime_rejects_loaded_equality_operand_result_type_mismatch_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::Equality {
            ty: bool_type,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(LoadedValueTemplate::Literal {
                ty: MAIN_STATE,
                value: RuntimeValue::Atom("MainState".to_string()),
            }),
            right: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };
    let expected = format!(
        "process Main message id 0 next_state_condition.left has type id {}, expected {}",
        MAIN_STATE.as_u32(),
        bool_type.as_u32()
    );

    assert_loaded_admission_rejects_before_artifact_loaded(&program, &expected);
}

#[test]
fn runtime_rejects_loaded_boolean_predicate_non_bool_operand_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanBinary {
            ty: bool_type,
            operator: ArtifactValueBooleanOperator::And,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::BooleanBinary {
            ty: bool_type,
            operator: ArtifactValueBooleanOperator::And,
            left: Box::new(LoadedValueTemplate::Literal {
                ty: MAIN_STATE,
                value: RuntimeValue::Atom("MainState".to_string()),
            }),
            right: Box::new(LoadedValueTemplate::Literal {
                ty: bool_type,
                value: RuntimeValue::Atom("True".to_string()),
            }),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };
    let expected = format!(
        "process Main message id 0 next_state_condition.left has type id {}, expected {}",
        MAIN_STATE.as_u32(),
        bool_type.as_u32()
    );

    assert_loaded_admission_rejects_before_artifact_loaded(&program, &expected);
}

#[test]
fn runtime_rejects_loaded_boolean_not_non_bool_operand_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanNot {
            ty: bool_type,
            operand: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state = LoadedNextState::IfElse {
        condition: LoadedValueTemplate::BooleanNot {
            ty: bool_type,
            operand: Box::new(LoadedValueTemplate::Literal {
                ty: MAIN_STATE,
                value: RuntimeValue::Atom("MainState".to_string()),
            }),
        },
        then_state: Box::new(LoadedNextState::Current),
        else_state: Box::new(LoadedNextState::Current),
    };
    let expected = format!(
        "process Main message id 0 next_state_condition.operand has type id {}, expected {}",
        MAIN_STATE.as_u32(),
        bool_type.as_u32()
    );

    assert_loaded_admission_rejects_before_artifact_loaded(&program, &expected);
}
