use super::super::super::*;
use super::capabilities::decode_capability_descriptor;
use crate::fields::ArtifactFields;

pub(super) fn decode_boundaries(
    fields: &mut ArtifactFields,
) -> Result<(
    Vec<ArtifactProtocol>,
    Vec<ArtifactPort>,
    Vec<ArtifactComponent>,
)> {
    let protocol_count = fields.take_bounded_usize("protocol_count", 0, MAX_PROTOCOL_COUNT)?;
    let port_count = fields.take_bounded_usize("port_count", 0, MAX_PORT_COUNT)?;
    let component_count = fields.take_bounded_usize("component_count", 0, MAX_COMPONENT_COUNT)?;

    let mut protocols = Vec::with_capacity(protocol_count);
    for index in 0..protocol_count {
        let prefix = format!("protocol.{index}");
        protocols.push(ArtifactProtocol {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            message_type: fields.take_type_id(&format!("{prefix}.message_type_id"))?,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    let mut ports = Vec::with_capacity(port_count);
    for index in 0..port_count {
        let prefix = format!("port.{index}");
        ports.push(ArtifactPort {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            protocol: fields.take_protocol_id(&format!("{prefix}.protocol"))?,
            target_process: fields.take_process_id(&format!("{prefix}.target_process"))?,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    let mut components = Vec::with_capacity(component_count);
    for index in 0..component_count {
        let prefix = format!("component.{index}");
        components.push(ArtifactComponent {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            export_port: fields.take_port_id(&format!("{prefix}.export_port"))?,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    Ok((protocols, ports, components))
}
