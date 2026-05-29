use super::capabilities::add_capability_descriptor_bytes;
use super::{KeyLen, add_field_bytes, add_field_u32, add_field_usize};
use crate::{MantleArtifact, Result};

pub(super) fn add_boundary_bytes(total: &mut usize, artifact: &MantleArtifact) -> Result<()> {
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
        add_capability_descriptor_bytes(
            total,
            prefix.child("required_authority"),
            component.required_authority,
        )?;
    }
    Ok(())
}
