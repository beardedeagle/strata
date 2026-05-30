use super::capabilities::add_capability_descriptor_bytes;
use super::{EncodedArtifactShape, KeyLen, add_field_bytes, add_field_u32, add_field_usize};
use crate::{MantleArtifact, Result};

pub(super) fn add_boundary_bytes(
    total: &mut EncodedArtifactShape,
    artifact: &MantleArtifact,
) -> Result<()> {
    add_field_usize(
        total,
        KeyLen::new("protocol_count".len()),
        artifact.protocols.len(),
    )?;
    for (index, protocol) in artifact.protocols.iter().enumerate() {
        let prefix = KeyLen::root_indexed("protocol", index);
        add_field_bytes(total, prefix.child("debug_name"), &protocol.debug_name)?;
        add_field_u32(
            total,
            prefix.child("message_type_id"),
            protocol.message_type.as_u32(),
        )?;
        add_capability_descriptor_bytes(
            total,
            prefix.child("required_authority"),
            protocol.required_authority,
        )?;
    }

    add_field_usize(total, KeyLen::new("port_count".len()), artifact.ports.len())?;
    for (index, port) in artifact.ports.iter().enumerate() {
        let prefix = KeyLen::root_indexed("port", index);
        add_field_bytes(total, prefix.child("debug_name"), &port.debug_name)?;
        add_field_u32(total, prefix.child("protocol"), port.protocol.as_u32())?;
        add_field_u32(
            total,
            prefix.child("target_process"),
            port.target_process.as_u32(),
        )?;
        add_capability_descriptor_bytes(
            total,
            prefix.child("required_authority"),
            port.required_authority,
        )?;
    }

    add_field_usize(
        total,
        KeyLen::new("component_count".len()),
        artifact.components.len(),
    )?;
    for (index, component) in artifact.components.iter().enumerate() {
        let prefix = KeyLen::root_indexed("component", index);
        add_field_bytes(total, prefix.child("debug_name"), &component.debug_name)?;
        add_field_u32(
            total,
            prefix.child("export_port"),
            component.export_port.as_u32(),
        )?;
        add_field_usize(
            total,
            prefix.child("import_count"),
            component.import_ports.len(),
        )?;
        for (import_index, imported_port) in component.import_ports.iter().enumerate() {
            add_field_u32(
                total,
                prefix.indexed_child("import", import_index),
                imported_port.as_u32(),
            )?;
        }
        add_capability_descriptor_bytes(
            total,
            prefix.child("required_authority"),
            component.required_authority,
        )?;
    }
    add_field_usize(
        total,
        KeyLen::new("composition_count".len()),
        artifact.compositions.len(),
    )?;
    for (composition_index, composition) in artifact.compositions.iter().enumerate() {
        let prefix = KeyLen::root_indexed("composition", composition_index);
        add_field_bytes(total, prefix.child("debug_name"), &composition.debug_name)?;
        add_field_usize(
            total,
            prefix.child("component_instance_count"),
            composition.component_instances.len(),
        )?;
        for (instance_index, instance) in composition.component_instances.iter().enumerate() {
            let instance_prefix = prefix.indexed_child("instance", instance_index);
            add_field_bytes(
                total,
                instance_prefix.child("debug_name"),
                &instance.debug_name,
            )?;
            add_field_u32(
                total,
                instance_prefix.child("component"),
                instance.component.as_u32(),
            )?;
        }
        add_field_usize(
            total,
            prefix.child("port_binding_count"),
            composition.port_bindings.len(),
        )?;
        for (binding_index, binding) in composition.port_bindings.iter().enumerate() {
            let binding_prefix = prefix.indexed_child("port_binding", binding_index);
            add_field_u32(
                total,
                binding_prefix.child("importer"),
                binding.importer.as_u32(),
            )?;
            add_field_u32(
                total,
                binding_prefix.child("imported_port"),
                binding.imported_port.as_u32(),
            )?;
            add_field_u32(
                total,
                binding_prefix.child("exporter"),
                binding.exporter.as_u32(),
            )?;
            add_field_u32(
                total,
                binding_prefix.child("exported_port"),
                binding.exported_port.as_u32(),
            )?;
        }
    }
    Ok(())
}
