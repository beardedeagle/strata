use super::support::*;
use crate::RuntimeEvent;
use mantle_artifact::{
    ArtifactBranch, ArtifactPrimitiveType, ArtifactScalarArithmeticOperator,
    ArtifactScalarOrderingOperator, ArtifactScalarType, ArtifactValueEqualityOperator,
};

#[test]
fn runtime_evaluates_scalar_arithmetic_next_state_template() {
    let mut artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ScalarArithmetic {
            ty: u32_type,
            operator: ArtifactScalarArithmeticOperator::Add,
            left: Box::new(scalar_literal_template(u32_type, "1_u32")),
            right: Box::new(scalar_literal_template(u32_type, "2_u32")),
        });
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("scalar arithmetic runtime should finish");

    assert_eq!(report.processes[0].state, "3_u32");
}

#[test]
fn runtime_evaluates_typed_value_if_template() {
    let mut artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::IfElse {
            ty: u32_type,
            condition: Box::new(ArtifactValueTemplate::ScalarOrdering {
                ty: bool_type,
                operand_ty: u32_type,
                operator: ArtifactScalarOrderingOperator::GreaterEqual,
                left: Box::new(scalar_literal_template(u32_type, "10_u32")),
                right: Box::new(scalar_literal_template(u32_type, "10_u32")),
            }),
            then_value: Box::new(scalar_literal_template(u32_type, "3_u32")),
            else_value: Box::new(scalar_literal_template(u32_type, "0_u32")),
        });
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("typed value-if runtime should finish");

    assert_eq!(report.processes[0].state, "3_u32");
}

#[test]
fn runtime_uses_scalar_ordering_for_branch_selection() {
    let mut artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::ScalarOrdering {
            ty: bool_type,
            operand_ty: u32_type,
            operator: ArtifactScalarOrderingOperator::GreaterEqual,
            left: Box::new(scalar_literal_template(u32_type, "10_u32")),
            right: Box::new(scalar_literal_template(u32_type, "10_u32")),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Value(StateId::new(0))),
    };
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("scalar ordering runtime should finish");

    assert_eq!(report.processes[0].state, "3_u32");
    assert!(host.events().iter().any(|event| {
        matches!(
            event,
            RuntimeEvent::BranchSelected {
                branch: ArtifactBranch::Then,
                ..
            }
        )
    }));
}

#[test]
fn runtime_uses_scalar_equality_over_arithmetic_for_branch_selection() {
    let mut artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: u32_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::ScalarArithmetic {
                ty: u32_type,
                operator: ArtifactScalarArithmeticOperator::Add,
                left: Box::new(scalar_literal_template(u32_type, "1_u32")),
                right: Box::new(scalar_literal_template(u32_type, "2_u32")),
            }),
            right: Box::new(ArtifactValueTemplate::ScalarArithmetic {
                ty: u32_type,
                operator: ArtifactScalarArithmeticOperator::Multiply,
                left: Box::new(scalar_literal_template(u32_type, "1_u32")),
                right: Box::new(scalar_literal_template(u32_type, "3_u32")),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Value(StateId::new(0))),
    };
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("scalar equality runtime should finish");

    assert_eq!(report.processes[0].state, "3_u32");
    assert!(host.events().iter().any(|event| {
        matches!(
            event,
            RuntimeEvent::BranchSelected {
                branch: ArtifactBranch::Then,
                ..
            }
        )
    }));
}

#[test]
fn runtime_uses_primitive_equality_for_branch_selection() {
    let mut artifact = artifact_with_scalar_main_state();
    let bool_type = append_bool_type(&mut artifact);
    let text_value_type = append_primitive_type(&mut artifact, ArtifactPrimitiveType::String);
    let octet_value_type = append_primitive_type(&mut artifact, ArtifactPrimitiveType::Bytes);
    artifact.processes[0].state_type = octet_value_type;
    artifact.processes[0].state_values =
        state_values(octet_value_type, &["Bytes(00)", "Bytes(010262696e)"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanBinary {
            ty: bool_type,
            operator: ArtifactValueBooleanOperator::And,
            left: Box::new(ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: text_value_type,
                operator: ArtifactValueEqualityOperator::Equal,
                left: Box::new(ArtifactValueTemplate::Literal {
                    ty: text_value_type,
                    value: artifact_value("String(7265616479)"),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: text_value_type,
                    value: artifact_value("String(7265616479)"),
                }),
            }),
            right: Box::new(ArtifactValueTemplate::Equality {
                ty: bool_type,
                operand_ty: octet_value_type,
                operator: ArtifactValueEqualityOperator::NotEqual,
                left: Box::new(ArtifactValueTemplate::Literal {
                    ty: octet_value_type,
                    value: artifact_value("Bytes(00)"),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: octet_value_type,
                    value: artifact_value("Bytes(010262696e)"),
                }),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Value(StateId::new(0))),
    };
    let mut host = InMemoryRuntimeHost::default();

    let report = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("primitive equality runtime should finish");

    assert_eq!(report.processes[0].state, "Bytes(010262696e)");
    assert!(host.events().iter().any(|event| {
        matches!(
            event,
            RuntimeEvent::BranchSelected {
                branch: ArtifactBranch::Then,
                ..
            }
        )
    }));
}

#[test]
fn runtime_fails_closed_on_scalar_divide_or_modulo_by_zero() {
    for (operator, expected) in [
        (
            ArtifactScalarArithmeticOperator::Divide,
            "scalar division by zero",
        ),
        (
            ArtifactScalarArithmeticOperator::Modulo,
            "scalar modulo by zero",
        ),
    ] {
        let mut artifact = artifact_with_scalar_main_state();
        let u32_type = artifact.processes[0].state_type;
        artifact.processes[0].transitions[0].next_state =
            NextState::Template(ArtifactValueTemplate::ScalarArithmetic {
                ty: u32_type,
                operator,
                left: Box::new(scalar_literal_template(u32_type, "1_u32")),
                right: Box::new(scalar_literal_template(u32_type, "0_u32")),
            });
        let mut host = InMemoryRuntimeHost::default();

        let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
            .expect_err("zero divisor should fail closed");

        assert!(err.to_string().contains(expected), "{err}");
        assert!(
            host.stdout().is_empty(),
            "runtime scalar failure must not emit host output"
        );
    }
}

#[test]
fn runtime_rejects_loaded_invalid_scalar_arithmetic_result_type_before_artifact_loaded() {
    let mut artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].state_type = bool_type;
    artifact.processes[0].state_values = state_values(bool_type, &["True"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].next_state = NextState::Value(StateId::new(0));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::ScalarArithmetic {
            ty: bool_type,
            operator: ArtifactScalarArithmeticOperator::Add,
            left: Box::new(LoadedValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
            right: Box::new(LoadedValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("2_u32"),
            }),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_template.type must be a scalar value type",
    );
}

#[test]
fn runtime_rejects_loaded_payload_enum_variants_that_collide_with_primitive_value_labels() {
    let mut artifact = artifact_with_scalar_main_state();
    let string_type = append_primitive_type(&mut artifact, ArtifactPrimitiveType::String);
    let payload_type =
        TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact.types.push(ArtifactType::enum_value_with_payloads(
        "Payload",
        vec![ArtifactEnumVariant {
            label: "Text".to_string(),
            payload_type: Some(string_type),
        }],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.types[payload_type.index()] = ArtifactType::enum_value_with_payloads(
        "Payload",
        vec![ArtifactEnumVariant {
            label: "Bytes".to_string(),
            payload_type: Some(string_type),
        }],
    );

    let expected = format!(
        "loaded type.{} payload-bearing enum variant Bytes collides with reserved primitive value label",
        payload_type.index()
    );
    assert_loaded_admission_rejects_before_artifact_loaded(&program, &expected);
}

#[test]
fn runtime_rejects_loaded_malformed_value_if_before_artifact_loaded() {
    let artifact = artifact_with_scalar_main_state();
    let u32_type = artifact.processes[0].state_type;
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].next_state =
        LoadedNextState::Template(LoadedValueTemplate::IfElse {
            ty: u32_type,
            condition: Box::new(LoadedValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
            then_value: Box::new(LoadedValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("3_u32"),
            }),
            else_value: Box::new(LoadedValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("0_u32"),
            }),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main message id 0 next_state_template.condition.type must have type enum Bool { False, True }",
    );
}

fn artifact_with_scalar_main_state() -> MantleArtifact {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[0].state_type = u32_type;
    artifact.processes[0].state_values = state_values(u32_type, &["0_u32", "3_u32"]);
    artifact.processes[0].init_state = StateId::new(0);
    artifact.processes[0].transitions[0].next_state = NextState::Value(StateId::new(0));
    artifact
}

fn append_scalar_type(artifact: &mut MantleArtifact, scalar: ArtifactScalarType) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::scalar(scalar.source_name(), scalar));
    ty
}

fn append_bool_type(artifact: &mut MantleArtifact) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    ty
}

fn append_primitive_type(
    artifact: &mut MantleArtifact,
    primitive: ArtifactPrimitiveType,
) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::primitive(primitive.source_name(), primitive));
    ty
}

fn scalar_literal_template(ty: TypeId, value: &str) -> ArtifactValueTemplate {
    ArtifactValueTemplate::Literal {
        ty,
        value: artifact_value(value),
    }
}
