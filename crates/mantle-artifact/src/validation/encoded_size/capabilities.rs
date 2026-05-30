use super::{EncodedArtifactShape, KeyLen, add_field_bytes, add_field_u32};
use crate::{ArtifactCapabilityDescriptor, Result};

pub(super) fn add_capability_descriptor_bytes(
    total: &mut EncodedArtifactShape,
    prefix: KeyLen,
    descriptor: ArtifactCapabilityDescriptor,
) -> Result<()> {
    add_field_bytes(total, prefix.child("kind"), descriptor.kind_str())?;
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { target } => {
            add_field_u32(total, prefix.child("target_process"), target.as_u32())
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { protocol } => {
            add_field_u32(total, prefix.child("protocol"), protocol.as_u32())
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            add_field_u32(total, prefix.child("port"), port.as_u32())
        }
        ArtifactCapabilityDescriptor::ComponentExport { component } => {
            add_field_u32(total, prefix.child("component"), component.as_u32())
        }
    }
}
