use super::support::*;

#[test]
fn validate_accepts_declared_boundary_tables() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];
    artifact.components = vec![ArtifactComponent {
        debug_name: "WorkerComponent".to_string(),
        export_port: PortId::new(0),
        import_ports: Vec::new(),
        required_authority: ArtifactCapabilityDescriptor::ComponentExport {
            component: ComponentId::new(0),
        },
    }];
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "connect_worker".to_string(),
        descriptor: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    });
    if let ArtifactAction::Send { port, .. } = &mut artifact.processes[0].transitions[0].actions[1]
    {
        *port = Some(PortId::new(0));
    }

    artifact
        .validate()
        .expect("typed boundary tables should admit");
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("boundary artifact should round trip");
    assert_eq!(decoded.ports[0].target_process, ProcessId::new(1));
    assert!(matches!(
        decoded.processes[0].transitions[0].actions[1],
        ArtifactAction::Send {
            port: Some(port),
            ..
        } if port == PortId::new(0)
    ));
}

#[test]
fn validate_rejects_port_target_message_type_mismatch() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "MainProtocol".to_string(),
        message_type: MAIN_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("port mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("expected protocol message type id 1")
    );
}

#[test]
fn validate_rejects_send_port_target_mismatch() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "MainProtocol".to_string(),
        message_type: MAIN_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "MainPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(0),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "connect_main".to_string(),
        descriptor: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    });
    if let ArtifactAction::Send { port, .. } = &mut artifact.processes[0].transitions[0].actions[1]
    {
        *port = Some(PortId::new(0));
    }

    let err = artifact
        .validate()
        .expect_err("send through mismatched port should fail closed");

    assert!(
        err.to_string()
            .contains("sends through port id 0 targeting process id 1, expected 0")
    );
}

#[test]
fn validate_rejects_send_port_without_process_authority() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];
    if let ArtifactAction::Send { port, .. } = &mut artifact.processes[0].transitions[0].actions[1]
    {
        *port = Some(PortId::new(0));
    }

    let err = artifact
        .validate()
        .expect_err("port send without authority should fail closed");

    assert!(
        err.to_string()
            .contains("send through port id 0 requires authority port_connect")
    );
}

#[test]
fn validate_rejects_boundary_required_authority_id_mismatch() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(1),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("boundary authority id mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("protocol id 0 required authority must be protocol_boundary")
    );
}

#[test]
fn validate_rejects_port_required_authority_id_mismatch() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(1),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("port authority id mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("port id 0 required authority must be port_connect")
    );
}

#[test]
fn validate_rejects_component_required_authority_id_mismatch() {
    let mut artifact = valid_artifact();
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];
    artifact.components = vec![ArtifactComponent {
        debug_name: "WorkerComponent".to_string(),
        export_port: PortId::new(0),
        import_ports: Vec::new(),
        required_authority: ArtifactCapabilityDescriptor::ComponentExport {
            component: ComponentId::new(1),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("component authority id mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("component id 0 required authority must be component_export")
    );
}

#[test]
fn validate_accepts_checked_component_composition_metadata() {
    let artifact = composition_artifact();

    artifact
        .validate()
        .expect("checked component composition metadata should admit");
    let decoded =
        MantleArtifact::decode(&artifact.encode()).expect("composition metadata should round trip");

    assert_eq!(decoded.compositions[0].debug_name, "AppComposition");
    assert_eq!(decoded.components[0].import_ports, vec![PortId::new(1)]);
    assert_eq!(
        decoded.compositions[0].port_bindings[0].importer,
        ComponentInstanceId::new(0)
    );
    assert_eq!(
        decoded.compositions[0].port_bindings[0].exporter,
        ComponentInstanceId::new(1)
    );
}

#[test]
fn validate_rejects_unbound_component_import() {
    let mut artifact = composition_artifact();
    artifact.compositions[0].port_bindings.clear();

    let err = artifact
        .validate()
        .expect_err("unbound component import should fail closed");

    assert!(
        err.to_string().contains(
            "composition AppComposition instance main component MainComponent import port id 1 is not bound"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn validate_rejects_composition_binding_to_unimported_port() {
    let mut artifact = composition_artifact();
    artifact.compositions[0].port_bindings[0].imported_port = PortId::new(0);

    let err = artifact
        .validate()
        .expect_err("binding unimported port should fail closed");

    assert!(
        err.to_string().contains(
            "composition AppComposition instance main component MainComponent does not import port id 0"
        ),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn validate_rejects_component_import_count_above_bounds() {
    let mut artifact = composition_artifact();
    artifact.components[1].import_ports = vec![PortId::new(0); MAX_PORT_COUNT + 1];

    let err = artifact
        .validate()
        .expect_err("oversized component import list should fail closed");

    assert!(
        err.to_string().contains(&format!(
            "component.1.import_count must be no greater than {MAX_PORT_COUNT}"
        )),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn validate_rejects_composition_protocol_mismatch() {
    let mut artifact = composition_artifact();
    artifact.protocols.push(ArtifactProtocol {
        debug_name: "OtherWorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(2),
        },
    });
    artifact.ports.push(ArtifactPort {
        debug_name: "OtherWorkerPort".to_string(),
        protocol: ProtocolId::new(2),
        target_process: ProcessId::new(1),
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(2),
        },
    });
    artifact.components.push(ArtifactComponent {
        debug_name: "OtherWorkerComponent".to_string(),
        export_port: PortId::new(2),
        import_ports: Vec::new(),
        required_authority: ArtifactCapabilityDescriptor::ComponentExport {
            component: ComponentId::new(2),
        },
    });
    artifact.compositions[0].component_instances[1].component = ComponentId::new(2);
    artifact.compositions[0].port_bindings[0].exported_port = PortId::new(2);

    let err = artifact
        .validate()
        .expect_err("composition protocol mismatch should fail closed");

    assert!(
        err.to_string()
            .contains("connects port ids 1 and 2 with different protocols"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn validate_rejects_duplicate_composition_debug_name() {
    let mut artifact = composition_artifact();
    artifact.compositions.push(artifact.compositions[0].clone());

    let err = artifact
        .validate()
        .expect_err("duplicate composition names should fail closed");

    assert!(
        err.to_string()
            .contains("duplicate composition debug_name AppComposition"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn validate_rejects_composition_counts_above_bounds() {
    let component = ArtifactComponentInstance {
        debug_name: "extra".to_string(),
        component: ComponentId::new(0),
    };
    let binding = ArtifactPortBinding {
        importer: ComponentInstanceId::new(0),
        imported_port: PortId::new(1),
        exporter: ComponentInstanceId::new(1),
        exported_port: PortId::new(1),
    };
    let cases = [
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].component_instances =
                    vec![component.clone(); MAX_COMPONENT_INSTANCE_COUNT + 1];
                artifact
            },
            format!(
                "component_instance_count must be no greater than {MAX_COMPONENT_INSTANCE_COUNT}"
            ),
        ),
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].port_bindings = vec![binding; MAX_PORT_BINDING_COUNT + 1];
                artifact
            },
            format!("port_binding_count must be no greater than {MAX_PORT_BINDING_COUNT}"),
        ),
    ];

    for (artifact, expected) in cases {
        let err = artifact
            .validate()
            .expect_err("oversized composition table should fail closed");

        assert!(
            err.to_string().contains(&expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn validate_rejects_malformed_composition_references() {
    let cases = [
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].component_instances[1].component = ComponentId::new(9);
                artifact
            },
            "composition AppComposition instance worker references undefined component id 9",
        ),
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].port_bindings[0].importer = ComponentInstanceId::new(2);
                artifact
            },
            "composition AppComposition port binding id 0 references undefined importer instance id 2",
        ),
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].port_bindings[0].exporter = ComponentInstanceId::new(2);
                artifact
            },
            "composition AppComposition port binding id 0 references undefined exporter instance id 2",
        ),
    ];

    for (artifact, expected) in cases {
        let err = artifact
            .validate()
            .expect_err("malformed composition reference should fail closed");

        assert!(
            err.to_string().contains(expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

#[test]
fn validate_rejects_bad_composition_binding_edges() {
    let cases = [
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].port_bindings[0].exporter = ComponentInstanceId::new(0);
                artifact
            },
            "composition AppComposition port binding id 0 binds instance main to itself",
        ),
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0].port_bindings[0].exported_port = PortId::new(0);
                artifact
            },
            "composition AppComposition instance worker component WorkerComponent does not export port id 0",
        ),
        (
            {
                let mut artifact = composition_artifact();
                artifact.compositions[0]
                    .port_bindings
                    .push(ArtifactPortBinding {
                        importer: ComponentInstanceId::new(0),
                        imported_port: PortId::new(1),
                        exporter: ComponentInstanceId::new(1),
                        exported_port: PortId::new(1),
                    });
                artifact
            },
            "composition AppComposition binds importer instance id 0 port id 1 more than once",
        ),
    ];

    for (artifact, expected) in cases {
        let err = artifact
            .validate()
            .expect_err("bad composition binding should fail closed");

        assert!(
            err.to_string().contains(expected),
            "unexpected diagnostic for {expected}: {err}"
        );
    }
}

fn composition_artifact() -> MantleArtifact {
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
    artifact
}
