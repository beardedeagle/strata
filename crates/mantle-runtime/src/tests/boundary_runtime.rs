use super::support::*;
use crate::event::RuntimeAuthorityResult;

#[test]
fn runtime_traces_declared_boundary_send() {
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

    let mut host = InMemoryRuntimeHost::default();
    run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("declared boundary send should run");

    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::BoundarySendChecked {
            port_id,
            port,
            protocol_id,
            protocol,
            target_process_id,
            target_process,
            boundary_result: RuntimeAuthorityResult::Accepted,
            ..
        } if *port_id == PortId::new(0)
            && port == "WorkerPort"
            && *protocol_id == ProtocolId::new(0)
            && protocol == "WorkerProtocol"
            && *target_process_id == ProcessId::new(1)
            && target_process == "Worker"
    )));
}

#[test]
fn runtime_rejects_artifact_table_id_mismatch_before_execution() {
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
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("boundary mismatch should fail during admission");

    assert!(
        err.to_string()
            .contains("sends through port id 0 targeting process id 1, expected 0")
    );
    assert!(host.events().is_empty());
}

#[test]
fn runtime_rejects_boundary_send_without_process_authority() {
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
    let mut host = InMemoryRuntimeHost::default();

    let err = run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect_err("boundary authority omission should fail during admission");

    assert!(
        err.to_string()
            .contains("send through port id 0 requires authority port_connect")
    );
    assert!(host.events().is_empty());
}
