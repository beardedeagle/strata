use super::super::support::*;

#[test]
fn decode_rejects_missing_if_else_next_state_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state = NextState::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_state: Box::new(NextState::Value(StateId::new(1))),
        else_state: Box::new(NextState::Current),
    };
    let encoded = artifact.encode().replace(
        "process.1.transition.0.next_state_else.next_state=current\n",
        "",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("missing else branch should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field process.1.transition.0.next_state_else.next_state")
    );
}

#[test]
fn decode_rejects_if_else_next_state_above_terminal_limit() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].next_state =
        nested_if_else_next_state(MAX_NEXT_STATE_IF_ELSE_DEPTH + 1, bool_type);
    let encoded = artifact.encode();

    let err = MantleArtifact::decode(&encoded).expect_err("overly nested next_state should fail");

    assert!(
        err.to_string()
            .contains("next_state runtime if nesting exceeds maximum depth of 2"),
        "{err}"
    );
}

#[test]
fn decode_rejects_if_else_action_nesting_above_limit() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[1].transitions[0].actions = vec![nested_if_else_action(
        MAX_VALUE_TEMPLATE_DEPTH + 1,
        bool_type,
    )];
    let encoded = artifact.encode();

    let err = MantleArtifact::decode(&encoded).expect_err("overly nested action should fail");

    assert!(err.to_string().contains(&format!(
        "exceeds maximum action nesting depth of {MAX_VALUE_TEMPLATE_DEPTH}"
    )));
}

#[test]
fn decode_rejects_unknown_step_result() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.step_result=Stop",
        "process.1.transition.0.step_result=Crash",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown step result should fail");

    assert!(
        err.to_string()
            .contains("invalid step_result value \"Crash\"")
    );
}

#[test]
fn decode_rejects_unsupported_schema_before_body_fields() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version=0\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unsupported schema should fail first");

    assert!(err.to_string().contains(&format!(
        "unsupported artifact schema version 0; expected {ARTIFACT_SCHEMA_VERSION}"
    )));
}

#[test]
fn decode_reports_duplicate_fields() {
    let encoded = valid_artifact().encode().replace(
        "process.0.debug_name=Main",
        "process.0.debug_name=Main\nprocess.0.debug_name=Other",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("duplicate field should fail");

    assert!(
        err.to_string()
            .contains("duplicate artifact field \"process.0.debug_name\"")
    );
}

#[test]
fn decode_reports_unknown_fields() {
    let mut encoded = valid_artifact().encode();
    encoded.push_str("process.0.transition.0.action.0.extra=value\n");

    let err = MantleArtifact::decode(&encoded).expect_err("unknown field should fail");

    assert!(
        err.to_string()
            .contains("unknown artifact field \"process.0.transition.0.action.0.extra\"")
    );
}

#[test]
fn decode_reports_artifact_value_field_context() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value.0.value=MainState",
        "process.0.state_value.0.value=Main\u{7}State",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("invalid state value should fail");
    let message = err.to_string();

    assert!(
        message.contains(
            "process.0.state_value.0.value must be non-empty and contain no control characters"
        ),
        "unexpected error: {err}"
    );
    assert!(
        !message.contains("payload value"),
        "state value decode should not report payload context: {err}"
    );
}

#[test]
fn decode_rejects_unbounded_process_count_before_allocation() {
    let encoded = format!(
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version={ARTIFACT_SCHEMA_VERSION}\nprocess_count={}\n",
        MAX_PROCESS_COUNT + 1
    );

    let err = MantleArtifact::decode(&encoded).expect_err("process count should be bounded");

    assert!(
        err.to_string()
            .contains("process_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_nested_counts_before_allocation() {
    let encoded = valid_artifact().encode().replace(
        "process.0.state_value_count=1",
        &format!(
            "process.0.state_value_count={}",
            MAX_STATE_VALUES_PER_PROCESS + 1
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("state value count should be bounded");

    assert!(
        err.to_string()
            .contains("process.0.state_value_count must be no greater than")
    );
}

#[test]
fn decode_rejects_unbounded_transition_current_state_before_validation() {
    let encoded = valid_artifact().encode().replace(
        "process.1.transition.0.message=0",
        &format!(
            "process.1.transition.0.current_state={}\nprocess.1.transition.0.message=0",
            MAX_STATE_VALUES_PER_PROCESS
        ),
    );

    let err = MantleArtifact::decode(&encoded).expect_err("current_state id should be bounded");

    assert!(
        err.to_string()
            .contains("process.1.transition.0.current_state must be no greater than")
    );
}

#[test]
fn decode_rejects_unknown_transition_effect() {
    let encoded = valid_artifact().encode().replace(
        "process.0.transition.0.effect.1=send",
        "process.0.transition.0.effect.1=write",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown effect should fail");

    assert!(
        err.to_string()
            .contains("process.0.transition.0.effect.1: invalid effect value \"write\"")
    );
}
