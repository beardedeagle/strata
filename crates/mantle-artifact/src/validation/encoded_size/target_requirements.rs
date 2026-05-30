use crate::{ArtifactTargetRequirements, Result};

use super::{EncodedArtifactShape, KeyLen, add_field_bytes, add_field_usize};

pub(super) fn add_target_requirement_bytes(
    total: &mut EncodedArtifactShape,
    requirements: &ArtifactTargetRequirements,
) -> Result<()> {
    let prefix = KeyLen::new("target_requirements".len());
    add_field_bytes(
        total,
        prefix.child("source_language"),
        requirements.source_language.as_ref(),
    )?;
    add_field_usize(
        total,
        prefix.child("feature_count"),
        requirements.features.len(),
    )?;
    for (index, feature) in requirements.features.iter().enumerate() {
        add_field_bytes(
            total,
            prefix.indexed_child("feature", index),
            feature.as_str(),
        )?;
    }
    Ok(())
}
