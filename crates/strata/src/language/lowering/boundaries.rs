use mantle_artifact::{
    ArtifactComponent, ArtifactComponentInstance, ArtifactComposition, ArtifactPort,
    ArtifactPortBinding, ArtifactProtocol,
};

use super::capabilities::lower_capability_descriptor;
use super::{
    ArtifactTypeMap, lower_component_id, lower_component_instance_id, lower_port_id,
    lower_process_id, lower_protocol_id,
};
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
                import_ports: component
                    .import_ports()
                    .iter()
                    .map(|port| lower_port_id(*port))
                    .collect(),
                required_authority: lower_capability_descriptor(component.required_authority()),
            })
        })
        .collect()
}

pub(super) fn lower_compositions(
    checked: &CheckedProgram,
) -> mantle_artifact::Result<Vec<ArtifactComposition>> {
    checked
        .compositions()
        .iter()
        .map(|composition| {
            Ok(ArtifactComposition {
                debug_name: composition.debug_name().to_string(),
                component_instances: composition
                    .component_instances()
                    .iter()
                    .map(|instance| ArtifactComponentInstance {
                        debug_name: instance.debug_name().to_string(),
                        component: lower_component_id(instance.component()),
                    })
                    .collect(),
                port_bindings: composition
                    .port_bindings()
                    .iter()
                    .enumerate()
                    .map(|(index, binding)| {
                        debug_assert_eq!(binding.id().as_u32() as usize, index);
                        ArtifactPortBinding {
                            importer: lower_component_instance_id(binding.importer()),
                            imported_port: lower_port_id(binding.imported_port()),
                            exporter: lower_component_instance_id(binding.exporter()),
                            exported_port: lower_port_id(binding.exported_port()),
                        }
                    })
                    .collect(),
            })
        })
        .collect()
}
