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
            "component.{index}.debug_name={}\ncomponent.{index}.export_port={}\ncomponent.{index}.import_count={}\n",
            component.debug_name,
            component.export_port.as_u32(),
            component.import_ports.len()
        ));
        for (import_index, imported_port) in component.import_ports.iter().enumerate() {
            encoded.push_str(&format!(
                "component.{index}.import.{import_index}={}\n",
                imported_port.as_u32()
            ));
        }
        encode_capability_descriptor(
            encoded,
            &format!("component.{index}.required_authority"),
            component.required_authority,
        );
    }
    for (index, composition) in artifact.compositions.iter().enumerate() {
        encoded.push_str(&format!(
            "composition.{index}.debug_name={}\ncomposition.{index}.component_instance_count={}\n",
            composition.debug_name,
            composition.component_instances.len()
        ));
        for (instance_index, instance) in composition.component_instances.iter().enumerate() {
            encoded.push_str(&format!(
                "composition.{index}.instance.{instance_index}.debug_name={}\ncomposition.{index}.instance.{instance_index}.component={}\n",
                instance.debug_name,
                instance.component.as_u32()
            ));
        }
        encoded.push_str(&format!(
            "composition.{index}.port_binding_count={}\n",
            composition.port_bindings.len()
        ));
        for (binding_index, binding) in composition.port_bindings.iter().enumerate() {
            encoded.push_str(&format!(
                "composition.{index}.port_binding.{binding_index}.importer={}\ncomposition.{index}.port_binding.{binding_index}.imported_port={}\ncomposition.{index}.port_binding.{binding_index}.exporter={}\ncomposition.{index}.port_binding.{binding_index}.exported_port={}\n",
                binding.importer.as_u32(),
                binding.imported_port.as_u32(),
                binding.exporter.as_u32(),
                binding.exported_port.as_u32()
            ));
        }
    }
}
