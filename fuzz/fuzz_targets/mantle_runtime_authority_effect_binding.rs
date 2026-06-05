#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use mantle_artifact::MantleArtifact;

static ARTIFACTS: LazyLock<[MantleArtifact; 3]> = LazyLock::new(|| {
    [
        MantleArtifact::decode(include_str!(
            "../seeds/mantle_artifact_decode/effect_outcome_spawn_denied.mta"
        ))
        .expect("dynamic-local runtime authority/effect binding fuzz fixture should decode"),
        MantleArtifact::decode(include_str!(
            "../seeds/mantle_artifact_decode/local_supervision_restart.mta"
        ))
        .expect("lexical-supervisor runtime authority/effect binding fuzz fixture should decode"),
        MantleArtifact::decode(include_str!(
            "../seeds/mantle_artifact_decode/component_composition.mta"
        ))
        .expect(
            "component authority-surface runtime authority/effect binding fixture should decode",
        ),
    ]
});

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for artifact in ARTIFACTS.iter() {
        let _ = mantle_runtime::validate_runtime_authority_effect_binding_text(text, artifact);
    }
});
