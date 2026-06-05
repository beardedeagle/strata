use super::{
    ARTIFACT_CODEC_PROFILE, BenchmarkProfile, CHECK_LOWER_PROFILE, IMPORTS_CHECK_LOWER_PROFILE,
    IN_MEMORY_RUNTIME_PROFILE, JSONL_RUNTIME_PROFILE, LOCAL_SUPERVISION_RUNTIME_PROFILE,
    authority_effect, boundary_contracts, component_composition,
};

pub(super) const ALL_PROFILES: [BenchmarkProfile; 21] = [
    CHECK_LOWER_PROFILE,
    IMPORTS_CHECK_LOWER_PROFILE,
    boundary_contracts::CHECK_LOWER_PROFILE,
    component_composition::CHECK_LOWER_PROFILE,
    component_composition::REPORT_PROFILE,
    component_composition::ARTIFACT_BUILD_PROFILE,
    component_composition::ARTIFACT_ADMIT_PROFILE,
    component_composition::TARGET_REQUIREMENTS_PROFILE,
    component_composition::RUNTIME_BINDING_PROFILE,
    authority_effect::ARTIFACT_BUILD_PROFILE,
    authority_effect::ARTIFACT_ADMIT_PROFILE,
    authority_effect::POLICY_BUILD_PROFILE,
    authority_effect::POLICY_ADMIT_PROFILE,
    authority_effect::RUNTIME_BINDING_PROFILE,
    authority_effect::COMPONENT_RUNTIME_BINDING_PROFILE,
    authority_effect::RUNTIME_RUN_PROFILE,
    IN_MEMORY_RUNTIME_PROFILE,
    boundary_contracts::RUNTIME_PROFILE,
    ARTIFACT_CODEC_PROFILE,
    JSONL_RUNTIME_PROFILE,
    LOCAL_SUPERVISION_RUNTIME_PROFILE,
];

pub(super) const PROFILE_KEY_LIST: &str = "collection_state.check_lower, imports_main.check_lower, boundary_contracts_main.check_lower, component_composition_main.check_lower, component_composition_main.composition_report, component_composition_main.composition_artifact_build, component_composition_main.composition_artifact_admit, component_composition_main.target_requirements, component_composition_main.runtime_binding_run, effect_outcome_spawn_denied.authority_effect_artifact_build, effect_outcome_spawn_denied.authority_effect_artifact_admit, effect_outcome_spawn_denied.authority_policy_artifact_build, effect_outcome_spawn_denied.authority_policy_artifact_admit, effect_outcome_spawn_denied.authority_effect_runtime_binding, component_composition_main.authority_effect_runtime_binding, effect_outcome_spawn_denied.authority_effect_runtime_run, collection_state.in_memory_runtime, boundary_contracts_main.in_memory_runtime, collection_state.artifact_codec, collection_state.jsonl_runtime, local_supervision_restart.in_memory_runtime";

pub(super) fn validate_selected_profile(selected_profile: Option<&str>) {
    super::profile_selection::validate_selected_profile(
        selected_profile,
        &ALL_PROFILES,
        PROFILE_KEY_LIST,
    );
}
