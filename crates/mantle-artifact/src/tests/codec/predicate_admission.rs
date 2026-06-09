use super::super::support::*;

#[test]
fn admission_rejects_malformed_equality_operand_type() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: MAIN_STATE,
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
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("malformed equality operand type should fail admission");

    assert!(
        err.to_string().contains(
            "operand_type_id must be Bool, String, Bytes, a scalar value type, or a fieldless enum value type"
        ),
        "{err}"
    );
}

#[test]
fn admission_rejects_equality_non_bool_result_type() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: WORKER_STATE,
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
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("equality result type must be the Bool contract");

    assert!(
        err.to_string()
            .contains("next_state_condition must have type enum Bool { False, True }"),
        "{err}"
    );
}

#[test]
fn admission_rejects_equality_left_operand_result_type_mismatch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: bool_type,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Idle"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("equality operand templates must match operand_type_id");

    assert!(
        err.to_string().contains(&format!(
            "left has type id {}, expected {}",
            WORKER_STATE.as_u32(),
            bool_type.as_u32()
        )),
        "{err}"
    );
}

#[test]
fn admission_rejects_boolean_predicate_non_bool_operand() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanBinary {
            ty: bool_type,
            operator: ArtifactValueBooleanOperator::And,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: bool_type,
                value: artifact_value("True"),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("boolean predicate operands must be Bool");

    assert!(
        err.to_string().contains(&format!(
            "next_state_condition.left has type id {}, expected {}",
            WORKER_STATE.as_u32(),
            bool_type.as_u32()
        )),
        "{err}"
    );
}

#[test]
fn admission_rejects_boolean_not_non_bool_operand() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::BooleanNot {
            ty: bool_type,
            operand: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("boolean ! predicate operand must be Bool");

    assert!(
        err.to_string().contains(&format!(
            "next_state_condition.operand has type id {}, expected {}",
            WORKER_STATE.as_u32(),
            bool_type.as_u32()
        )),
        "{err}"
    );
}

#[test]
fn admission_rejects_payload_bearing_enum_equality_operand_type() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.types[WORKER_STATE.index()] = ArtifactType::enum_value_with_payloads(
        "WorkerState",
        vec![
            ArtifactEnumVariant {
                label: "Idle".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Handled".to_string(),
                payload_type: None,
            },
            ArtifactEnumVariant {
                label: "Working".to_string(),
                payload_type: Some(JOB),
            },
        ],
    );
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Equality {
            ty: bool_type,
            operand_ty: WORKER_STATE,
            operator: ArtifactValueEqualityOperator::Equal,
            left: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
            right: Box::new(ArtifactValueTemplate::Literal {
                ty: WORKER_STATE,
                value: artifact_value("Handled"),
            }),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };

    let err = artifact
        .validate()
        .expect_err("payload-bearing enum equality operand type should fail admission");

    assert!(
        err.to_string().contains(
            "operand_type_id must be Bool, String, Bytes, a scalar value type, or a fieldless enum value type"
        ),
        "{err}"
    );
}
