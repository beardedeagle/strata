use super::super::super::*;

pub(super) fn encode_capability_descriptor(
    encoded: &mut String,
    prefix: &str,
    descriptor: ArtifactCapabilityDescriptor,
) {
    encoded.push_str(&format!("{prefix}.kind={}\n", descriptor.kind_str()));
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { target } => {
            encoded.push_str(&format!("{prefix}.target_process={}\n", target.as_u32()));
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { protocol } => {
            encoded.push_str(&format!("{prefix}.protocol={}\n", protocol.as_u32()));
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            encoded.push_str(&format!("{prefix}.port={}\n", port.as_u32()));
        }
        ArtifactCapabilityDescriptor::ComponentExport { component } => {
            encoded.push_str(&format!("{prefix}.component={}\n", component.as_u32()));
        }
    }
}
