use mantle_artifact::{ArtifactComponent, ArtifactPort, ArtifactProtocol};

use super::capabilities::lower_capability_descriptor;
use super::{ArtifactTypeMap, lower_port_id, lower_process_id, lower_protocol_id};
use crate::language::checked::CheckedProgram;

pub(super) fn lower_protocols(
    checked: &CheckedProgram,
    types: &ArtifactTypeMap,
) -> mantle_artifact::Result<Vec<ArtifactProtocol>> {
    checked
        .protocols()
        .iter()
        .map(|protocol| {
            Ok(ArtifactProtocol {
                debug_name: protocol.debug_name().to_string(),
                message_type: types.artifact_id(protocol.message_type())?,
                required_authority: lower_capability_descriptor(protocol.required_authority()),
            })
        })
        .collect()
}

pub(super) fn lower_ports(checked: &CheckedProgram) -> mantle_artifact::Result<Vec<ArtifactPort>> {
    checked
        .ports()
        .iter()
        .map(|port| {
            Ok(ArtifactPort {
                debug_name: port.debug_name().to_string(),
                protocol: lower_protocol_id(port.protocol()),
                target_process: lower_process_id(port.target_process()),
                required_authority: lower_capability_descriptor(port.required_authority()),
            })
        })
        .collect()
}

pub(super) fn lower_components(
    checked: &CheckedProgram,
) -> mantle_artifact::Result<Vec<ArtifactComponent>> {
    checked
        .components()
        .iter()
        .map(|component| {
            Ok(ArtifactComponent {
                debug_name: component.debug_name().to_string(),
                export_port: lower_port_id(component.export_port()),
                required_authority: lower_capability_descriptor(component.required_authority()),
            })
        })
        .collect()
}
