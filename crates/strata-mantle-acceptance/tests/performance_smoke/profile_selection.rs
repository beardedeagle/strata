use super::BenchmarkProfile;

pub(super) fn validate_selected_profile(
    selected_profile: Option<&str>,
    profiles: &[BenchmarkProfile],
    profile_key_list: &str,
) {
    if let Some(selected_profile) = selected_profile {
        assert!(
            profiles
                .iter()
                .any(|profile| profile.key == selected_profile),
            "STRATA_PERFORMANCE_SMOKE_PROFILE must be one of: {profile_key_list}"
        );
    }
}

pub(super) fn profile_is_selected(
    selected_profile: Option<&str>,
    profile: BenchmarkProfile,
) -> bool {
    match selected_profile {
        Some(selected_profile) => selected_profile == profile.key,
        None => true,
    }
}
