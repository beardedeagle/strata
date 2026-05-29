use super::super::ast::Module;
use super::super::checked::{
    CheckedCapabilityDescriptor, CheckedComponent, CheckedPort, CheckedProtocol,
};
use super::super::diagnostic::Result;
use super::symbols::SemanticIndex;
use super::types::CheckedTypeInterner;
use mantle_artifact::{MAX_COMPONENT_COUNT, MAX_PORT_COUNT, MAX_PROTOCOL_COUNT};

pub(super) struct CheckedBoundaries {
    pub(super) protocols: Vec<CheckedProtocol>,
    pub(super) ports: Vec<CheckedPort>,
    pub(super) components: Vec<CheckedComponent>,
}

pub(super) fn validate_boundary_counts(module: &Module) -> Result<()> {
    super::validate_count(
        "protocol_count",
        module.protocols.len(),
        0,
        MAX_PROTOCOL_COUNT,
    )?;
    super::validate_count("port_count", module.ports.len(), 0, MAX_PORT_COUNT)?;
    super::validate_count(
        "component_count",
        module.components.len(),
        0,
        MAX_COMPONENT_COUNT,
    )
}

pub(super) fn check_boundaries(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<CheckedBoundaries> {
    let protocols = module
        .protocols
        .iter()
        .enumerate()
        .map(|(index, protocol)| {
            let protocol_id = semantic_index.protocol_id(&protocol.name)?;
            if protocol_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "protocol {} has inconsistent checked id {} at index {index}",
                    protocol.name,
                    protocol_id.as_u32()
                )));
            }
            Ok(CheckedProtocol::new(
                protocol.name.clone(),
                types.intern(&protocol.message_type)?,
                CheckedCapabilityDescriptor::ProtocolBoundary {
                    protocol: protocol_id,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let ports = module
        .ports
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let port_id = semantic_index.port_id(&port.name)?;
            if port_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "port {} has inconsistent checked id {} at index {index}",
                    port.name,
                    port_id.as_u32()
                )));
            }
            let contract = semantic_index.port_contract(port_id)?;
            Ok(CheckedPort::new(
                port.name.clone(),
                contract.protocol,
                contract.target_process,
                CheckedCapabilityDescriptor::PortConnect { port: port_id },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let components = module
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            let component_id = semantic_index.component_id(&component.name)?;
            if component_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "component {} has inconsistent checked id {} at index {index}",
                    component.name,
                    component_id.as_u32()
                )));
            }
            Ok(CheckedComponent::new(
                component.name.clone(),
                semantic_index.port_id(&component.export)?,
                CheckedCapabilityDescriptor::ComponentExport {
                    component: component_id,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CheckedBoundaries {
        protocols,
        ports,
        components,
    })
}
