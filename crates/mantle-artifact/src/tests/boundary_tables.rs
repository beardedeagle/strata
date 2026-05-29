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
