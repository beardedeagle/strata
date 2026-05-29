use super::super::super::*;
use super::capabilities::encode_capability_descriptor;

pub(super) fn encode_boundaries(encoded: &mut String, artifact: &MantleArtifact) {
    for (index, protocol) in artifact.protocols.iter().enumerate() {
        encoded.push_str(&format!(
            "protocol.{index}.debug_name={}\nprotocol.{index}.message_type_id={}\n",
            protocol.debug_name,
            protocol.message_type.as_u32()
        ));
        encode_capability_descriptor(
            encoded,
            &format!("protocol.{index}.required_authority"),
            protocol.required_authority,
        );
    }
    for (index, port) in artifact.ports.iter().enumerate() {
        encoded.push_str(&format!(
            "port.{index}.debug_name={}\nport.{index}.protocol={}\nport.{index}.target_process={}\n",
            port.debug_name,
            port.protocol.as_u32(),
            port.target_process.as_u32()
        ));
        encode_capability_descriptor(
            encoded,
            &format!("port.{index}.required_authority"),
            port.required_authority,
        );
    }
    for (index, component) in artifact.components.iter().enumerate() {
        encoded.push_str(&format!(
            "component.{index}.debug_name={}\ncomponent.{index}.export_port={}\n",
            component.debug_name,
            component.export_port.as_u32()
        ));
        encode_capability_descriptor(
            encoded,
            &format!("component.{index}.required_authority"),
            component.required_authority,
        );
    }
}
