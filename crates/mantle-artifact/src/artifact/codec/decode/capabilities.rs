use super::super::super::*;
use crate::fields::ArtifactFields;

pub(super) fn decode_capability_descriptor(
    fields: &mut ArtifactFields,
    prefix: &str,
) -> Result<ArtifactCapabilityDescriptor> {
    let kind = fields.take_required(&format!("{prefix}.kind"))?;
    match kind {
        "spawn" => Ok(ArtifactCapabilityDescriptor::Spawn {
            target: fields.take_process_id(&format!("{prefix}.target_process"))?,
        }),
        "protocol_boundary" => Ok(ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: fields.take_protocol_id(&format!("{prefix}.protocol"))?,
        }),
        "port_connect" => Ok(ArtifactCapabilityDescriptor::PortConnect {
            port: fields.take_port_id(&format!("{prefix}.port"))?,
        }),
        "component_export" => Ok(ArtifactCapabilityDescriptor::ComponentExport {
            component: fields.take_component_id(&format!("{prefix}.component"))?,
        }),
        _ => Err(Error::new(format!("invalid {prefix}.kind value {kind:?}"))),
    }
}
