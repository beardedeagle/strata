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
fn decode_rejects_unknown_spawn_authority_kind() {
    let encoded = valid_artifact().encode().replace(
        "process.0.authority.0.kind=spawn",
        "process.0.authority.0.kind=ambient",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown authority kind should fail");

    assert!(
        err.to_string()
            .contains("invalid process.0.authority.0.kind value \"ambient\""),
        "{err}"
    );
}

#[test]
fn decode_rejects_missing_boundary_table_counts() {
    let encoded = valid_artifact().encode().replace("protocol_count=0\n", "");

    let err = MantleArtifact::decode(&encoded).expect_err("missing protocol count should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field protocol_count"),
        "{err}"
    );
}

#[test]
fn decode_rejects_missing_composition_instance_component_field() {
    let encoded = valid_composition_artifact()
        .encode()
        .replace("composition.0.instance.0.component=0\n", "");

    let err =
        MantleArtifact::decode(&encoded).expect_err("missing component instance field should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field composition.0.instance.0.component"),
        "{err}"
    );
}

#[test]
fn decode_rejects_missing_composition_port_binding_field() {
    let encoded = valid_composition_artifact()
        .encode()
        .replace("composition.0.port_binding.0.importer=0\n", "");

    let err = MantleArtifact::decode(&encoded).expect_err("missing port binding field should fail");

    assert!(
        err.to_string()
            .contains("missing artifact field composition.0.port_binding.0.importer"),
        "{err}"
    );
}

#[test]
fn decode_rejects_unbounded_composition_counts_before_allocation() {
    let cases = [
        (
            valid_composition_artifact().encode().replace(
                "composition.0.component_instance_count=2",
                &format!(
                    "composition.0.component_instance_count={}",
                    MAX_COMPONENT_INSTANCE_COUNT + 1
                ),
            ),
            "composition.0.component_instance_count must be no greater than",
        ),
        (
            valid_composition_artifact().encode().replace(
                "composition.0.port_binding_count=1",
                &format!(
                    "composition.0.port_binding_count={}",
                    MAX_PORT_BINDING_COUNT + 1
                ),
            ),
            "composition.0.port_binding_count must be no greater than",
        ),
    ];

    for (encoded, expected) in cases {
        let err = MantleArtifact::decode(&encoded)
            .expect_err("composition count should be bounded before allocation");

        assert!(
            err.to_string().contains(expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn decode_and_validate_reject_encoded_field_count_above_limit() {
    let artifact = artifact_above_encoded_field_count();
    let encoded = artifact.encode();

    assert!(encoded.len() < MAX_ARTIFACT_BYTES);
    assert!(encoded.lines().skip(1).count() > MAX_ARTIFACT_FIELDS);

    let validate_err = artifact
        .validate()
        .expect_err("programmatic artifact above decode field limit should fail");
    assert!(
        validate_err.to_string().contains(&format!(
            "artifact declares too many fields; maximum supported count is {MAX_ARTIFACT_FIELDS}"
        )),
        "unexpected validation diagnostic: {validate_err}"
    );

    let decode_err = MantleArtifact::decode(&encoded)
        .expect_err("encoded artifact above decode field limit should fail");
    assert!(
        decode_err.to_string().contains(&format!(
            "artifact declares too many fields; maximum supported count is {MAX_ARTIFACT_FIELDS}"
        )),
        "unexpected decode diagnostic: {decode_err}"
    );
}

#[test]
fn decode_rejects_unknown_spawn_site_kind() {
    let encoded = valid_artifact().encode().replace(
        "process.0.spawn_site.0.kind=dynamic_local",
        "process.0.spawn_site.0.kind=remote",
    );

    let err = MantleArtifact::decode(&encoded).expect_err("unknown spawn site kind should fail");

    assert!(
        err.to_string()
            .contains("invalid spawn_kind value \"remote\""),
        "{err}"
    );
}

#[test]
fn decode_rejects_unknown_runtime_feature_requirement() {
    let encoded = valid_artifact().encode().replace(
        "target_requirements.feature.0=bounded_mailbox",
        "target_requirements.feature.0=remote_execution_v99",
    );

    let err =
        MantleArtifact::decode(&encoded).expect_err("unknown runtime feature should fail closed");

    assert!(
        err.to_string()
            .contains("invalid runtime feature \"remote_execution_v99\""),
        "{err}"
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
        "MTA0\nformat={ARTIFACT_FORMAT}\nschema_version={ARTIFACT_SCHEMA_VERSION}\nsource_language={TEST_SOURCE_LANGUAGE}\ntarget_requirements.source_language={TEST_SOURCE_LANGUAGE}\ntarget_requirements.feature_count=1\ntarget_requirements.feature.0=bounded_mailbox\nprocess_count={}\n",
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

fn valid_composition_artifact() -> MantleArtifact {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![
        ArtifactProtocol {
            debug_name: "MainProtocol".to_string(),
            message_type: MAIN_MSG,
            required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: ProtocolId::new(0),
            },
        },
        ArtifactProtocol {
            debug_name: "WorkerProtocol".to_string(),
            message_type: WORKER_MSG,
            required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: ProtocolId::new(1),
            },
        },
    ];
    artifact.ports = vec![
        ArtifactPort {
            debug_name: "MainPort".to_string(),
            protocol: ProtocolId::new(0),
            target_process: ProcessId::new(0),
            required_authority: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(0),
            },
        },
        ArtifactPort {
            debug_name: "WorkerPort".to_string(),
            protocol: ProtocolId::new(1),
            target_process: ProcessId::new(1),
            required_authority: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(1),
            },
        },
    ];
    artifact.components = vec![
        ArtifactComponent {
            debug_name: "MainComponent".to_string(),
            export_port: PortId::new(0),
            import_ports: vec![PortId::new(1)],
            required_authority: ArtifactCapabilityDescriptor::ComponentExport {
                component: ComponentId::new(0),
            },
        },
        ArtifactComponent {
            debug_name: "WorkerComponent".to_string(),
            export_port: PortId::new(1),
            import_ports: Vec::new(),
            required_authority: ArtifactCapabilityDescriptor::ComponentExport {
                component: ComponentId::new(1),
            },
        },
    ];
    artifact.compositions = vec![ArtifactComposition {
        debug_name: "AppComposition".to_string(),
        component_instances: vec![
            ArtifactComponentInstance {
                debug_name: "main".to_string(),
                component: ComponentId::new(0),
            },
            ArtifactComponentInstance {
                debug_name: "worker".to_string(),
                component: ComponentId::new(1),
            },
        ],
        port_bindings: vec![ArtifactPortBinding {
            importer: ComponentInstanceId::new(0),
            imported_port: PortId::new(1),
            exporter: ComponentInstanceId::new(1),
            exported_port: PortId::new(1),
        }],
    }];
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "connect_worker".to_string(),
        descriptor: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(1),
        },
    });
    if let ArtifactAction::Send { port, .. } = &mut artifact.processes[0].transitions[0].actions[1]
    {
        *port = Some(PortId::new(1));
    }

    artifact
}

fn artifact_above_encoded_field_count() -> MantleArtifact {
    let mut artifact = valid_artifact();
    while artifact.types.len() < MAX_TYPE_COUNT {
        artifact.types.push(ArtifactType::value(format!(
            "ExtraType{}",
            artifact.types.len()
        )));
    }
    artifact.outputs = (0..MAX_OUTPUT_LITERALS)
        .map(|index| format!("output_{index}"))
        .collect();
    artifact
}
