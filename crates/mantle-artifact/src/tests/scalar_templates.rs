use super::support::*;

#[test]
fn admission_accepts_scalar_state_values_and_templates() {
    let mut artifact = valid_artifact();
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[1].state_type = u32_type;
    artifact.processes[1].state_values = state_values(u32_type, &["0_u32", "3_u32"]);
    artifact.processes[1].init_state = StateId::new(0);
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ScalarArithmetic {
            ty: u32_type,
            operator: ArtifactScalarArithmeticOperator::Add,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("2_u32"),
            }),
        });

    artifact
        .validate()
        .expect("scalar artifact should validate");
    let encoded = artifact.encode();
    assert!(encoded.contains(".shape=scalar"));
    assert!(encoded.contains(".kind=scalar_arithmetic"));

    let decoded = MantleArtifact::decode(&encoded).expect("scalar artifact should decode");
    decoded
        .validate()
        .expect("decoded scalar artifact should validate");
}

#[test]
fn admission_accepts_scalar_equality_template() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: u32_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::ScalarArithmetic {
                ty: u32_type,
                operator: ArtifactScalarArithmeticOperator::Add,
                left: Box::new(ArtifactValueTemplate::Literal {
                    ty: u32_type,
                    value: artifact_value("1_u32"),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: u32_type,
                    value: artifact_value("2_u32"),
                }),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("3_u32"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };

    artifact
        .validate()
        .expect("scalar equality artifact should validate");
}

#[test]
fn admission_accepts_typed_value_if_template() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::IfElse {
            ty: WORKER_STATE,
            condition: Box::new(ArtifactValueTemplate::ScalarOrdering {
                ty: bool_type,
                operand_ty: u32_type,
                operator: ArtifactScalarOrderingOperator::GreaterEqual,
                left: Box::new(ArtifactValueTemplate::Literal {
                    ty: u32_type,
                    value: artifact_value("10_u32"),
                }),
                right: Box::new(ArtifactValueTemplate::Literal {
                    ty: u32_type,
                    value: artifact_value("10_u32"),
                }),
            }),
            then_value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
            else_value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Idle"),
            }),
        });

    artifact
        .validate()
        .expect("typed value-if artifact should validate");
    let encoded = artifact.encode();
    assert!(encoded.contains(".kind=if_else"));

    let decoded = MantleArtifact::decode(&encoded).expect("value-if artifact should decode");
    decoded
        .validate()
        .expect("decoded value-if artifact should validate");
}

#[test]
fn admission_rejects_wrong_scalar_value_shape() {
    let mut artifact = valid_artifact();
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[1].state_type = u32_type;
    artifact.processes[1].state_values = state_values(u32_type, &["1_u64"]);

    let err = artifact
        .validate()
        .expect_err("wrong scalar value type should fail admission");

    assert!(
        err.to_string()
            .contains("scalar value 1_u64 has type U64, expected U32"),
        "{err}"
    );
}

#[test]
fn admission_rejects_malformed_value_if_template() {
    let mut non_bool_condition = valid_artifact();
    let u32_type = append_scalar_type(&mut non_bool_condition, ArtifactScalarType::U32);
    non_bool_condition.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::IfElse {
            ty: WORKER_STATE,
            condition: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
            then_value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
            else_value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Idle"),
            }),
        });
    let err = non_bool_condition
        .validate()
        .expect_err("non-Bool value-if condition should fail admission");
    assert!(
        err.to_string()
            .contains("condition.type_id must have type enum Bool { False, True }"),
        "{err}"
    );

    let mut branch_type_mismatch = valid_artifact();
    let bool_type = append_bool_type(&mut branch_type_mismatch);
    let u32_type = append_scalar_type(&mut branch_type_mismatch, ArtifactScalarType::U32);
    branch_type_mismatch.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::IfElse {
            ty: WORKER_STATE,
            condition: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
            then_value: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
            else_value: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
        });
    let err = branch_type_mismatch
        .validate()
        .expect_err("value-if branch type mismatch should fail admission");
    assert!(err.to_string().contains("else has type id"), "{err}");
}

#[test]
fn admission_rejects_invalid_scalar_operator_type_pairing() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    artifact.processes[1].state_type = bool_type;
    artifact.processes[1].state_values = state_values(bool_type, &["True"]);
    artifact.processes[1].transitions[0].next_state =
        NextState::Template(ArtifactValueTemplate::ScalarArithmetic {
            ty: bool_type,
            operator: ArtifactScalarArithmeticOperator::Add,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("1_u32"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("2_u32"),
            }),
        });

    let err = artifact
        .validate()
        .expect_err("scalar arithmetic result type must be scalar");

    assert!(
        err.to_string()
            .contains("type_id must be a scalar value type"),
        "{err}"
    );
}

#[test]
fn admission_rejects_scalar_ordering_operand_type_mismatch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let u32_type = append_scalar_type(&mut artifact, ArtifactScalarType::U32);
    let u64_type = append_scalar_type(&mut artifact, ArtifactScalarType::U64);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::ScalarOrdering {
            ty: bool_type,
            operand_ty: u32_type,
            operator: ArtifactScalarOrderingOperator::GreaterEqual,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: u32_type,
                value: artifact_value("10_u32"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: u64_type,
                value: artifact_value("10_u64"),
            }),
        },
        then_state: Box::new(NextState::Current),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("scalar ordering operands must match operand type");

    assert!(
        err.to_string().contains(&format!(
            "right has type id {}, expected {}",
            u64_type.as_u32(),
            u32_type.as_u32()
        )),
        "{err}"
    );
}

#[test]
fn decode_rejects_malformed_scalar_type_and_value() {
    let mut artifact = valid_artifact();
    let u8_type = append_scalar_type(&mut artifact, ArtifactScalarType::U8);
    artifact.processes[1].state_type = u8_type;
    artifact.processes[1].state_values = state_values(u8_type, &["1_u8"]);
    let encoded = artifact.encode();

    let malformed_type = encoded.replace(
        &format!("type.{}.scalar_type=u8", u8_type.as_u32()),
        &format!("type.{}.scalar_type=u128", u8_type.as_u32()),
    );
    let err = MantleArtifact::decode(&malformed_type).expect_err("unknown scalar type should fail");
    assert!(
        err.to_string().contains("invalid scalar type \"u128\""),
        "{err}"
    );

    let malformed_value = encoded.replace(
        "process.1.state_value.0.value=1_u8",
        "process.1.state_value.0.value=300_u8",
    );
    let err =
        MantleArtifact::decode(&malformed_value).expect_err("out-of-range scalar should fail");
    assert!(err.to_string().contains("outside U8 range"), "{err}");
}

#[test]
fn decode_preserves_atom_values_with_scalar_suffix_text() {
    let mut artifact = valid_artifact();
    artifact.types[WORKER_STATE.index()] =
        ArtifactType::enum_value("WorkerState", vec!["Ready_u8".to_string()]);
    artifact.processes[1].state_values = state_values(WORKER_STATE, &["Ready_u8"]);
    artifact.processes[1].init_state = StateId::new(0);
    artifact.processes[1].transitions[0].next_state = NextState::Value(StateId::new(0));

    artifact
        .validate()
        .expect("atom label ending with scalar suffix text should validate");
    let decoded = MantleArtifact::decode(&artifact.encode())
        .expect("atom label ending with scalar suffix text should decode");

    decoded
        .validate()
        .expect("decoded scalar-suffix atom label should validate");
    assert_eq!(decoded.processes[1].state_values[0].label, "Ready_u8");
}

fn append_scalar_type(artifact: &mut MantleArtifact, scalar: ArtifactScalarType) -> TypeId {
    let ty = TypeId::from_index(artifact.types.len()).expect("test type index should fit");
    artifact
        .types
        .push(ArtifactType::scalar(scalar.source_name(), scalar));
    ty
}
