use std::fmt::Write as _;

use crate::ArtifactTargetRequirements;

pub(super) fn encode_target_requirements(
    encoded: &mut String,
    requirements: &ArtifactTargetRequirements,
) {
    writeln!(
        encoded,
        "target_requirements.source_language={}",
        requirements.source_language
    )
    .expect("writing to a String cannot fail");
    writeln!(
        encoded,
        "target_requirements.feature_count={}",
        requirements.features.len()
    )
    .expect("writing to a String cannot fail");
    for (index, feature) in requirements.features.iter().enumerate() {
        writeln!(
            encoded,
            "target_requirements.feature.{index}={}",
            feature.as_str()
        )
        .expect("writing to a String cannot fail");
    }
}
