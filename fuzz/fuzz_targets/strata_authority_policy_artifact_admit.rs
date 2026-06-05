#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;

static AUTHORITY_EFFECT_ARTIFACTS: LazyLock<[&'static str; 3]> = LazyLock::new(|| {
    [
        include_str!(
            "../seeds/strata_authority_effect_artifact_admit/valid_admitted.authority-effect.json"
        ),
        include_str!(
            "../seeds/strata_authority_effect_artifact_admit/valid_lexical_supervisor_child.authority-effect.json"
        ),
        include_str!(
            "../seeds/strata_authority_effect_artifact_admit/valid_component_authority_surfaces.authority-effect.json"
        ),
    ]
});

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for authority_effect in AUTHORITY_EFFECT_ARTIFACTS.iter() {
        let _ = strata::language::admit_authority_policy_artifact(text, authority_effect);
    }
});
