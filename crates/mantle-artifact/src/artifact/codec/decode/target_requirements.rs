use crate::{
    ArtifactTargetRequirements, MAX_RUNTIME_FEATURE_REQUIREMENTS, Result, RuntimeFeature,
    fields::ArtifactFields,
};

pub(super) fn decode_target_requirements(
    fields: &mut ArtifactFields,
) -> Result<ArtifactTargetRequirements> {
    let source_language = fields.take_required_string("target_requirements.source_language")?;
    let feature_count = fields.take_bounded_usize(
        "target_requirements.feature_count",
        1,
        MAX_RUNTIME_FEATURE_REQUIREMENTS,
    )?;
    let mut features = Vec::with_capacity(feature_count);
    for index in 0..feature_count {
        features.push(RuntimeFeature::parse(
            fields.take_required(&format!("target_requirements.feature.{index}"))?,
        )?);
    }
    Ok(ArtifactTargetRequirements {
        source_language: source_language.into(),
        features,
    })
}
