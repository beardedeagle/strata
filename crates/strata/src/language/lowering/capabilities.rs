use mantle_artifact::ArtifactCapabilityDescriptor;

use super::{lower_component_id, lower_port_id, lower_process_id, lower_protocol_id};
use crate::language::checked::CheckedCapabilityDescriptor;

pub(in crate::language::lowering) fn lower_capability_descriptor(
    descriptor: CheckedCapabilityDescriptor,
) -> ArtifactCapabilityDescriptor {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { target } => ArtifactCapabilityDescriptor::Spawn {
            target: lower_process_id(target),
        },
        CheckedCapabilityDescriptor::ProtocolBoundary { protocol } => {
            ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: lower_protocol_id(protocol),
            }
        }
        CheckedCapabilityDescriptor::PortConnect { port } => {
            ArtifactCapabilityDescriptor::PortConnect {
                port: lower_port_id(port),
            }
        }
        CheckedCapabilityDescriptor::ComponentExport { component } => {
            ArtifactCapabilityDescriptor::ComponentExport {
                component: lower_component_id(component),
            }
        }
    }
}
