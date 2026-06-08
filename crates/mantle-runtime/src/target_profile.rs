use mantle_artifact::{Error, Result, RuntimeFeature};

pub(crate) const LOCAL_RUNTIME_TARGET_PROFILE: RuntimeTargetProfile = RuntimeTargetProfile {
    id: "mantle.local_only.v1",
    execution_scope: "single_host_local_runtime",
    host_authority: "explicit_local_host_sinks",
    network_authority: "none",
    transport_authority: "none",
    cluster_membership: "none",
    remote_operation_policy: "non_admitted",
    unsupported_fail_stage: "before_artifact_loaded",
    supported_features: SUPPORTED_RUNTIME_FEATURES,
    implementation_limits: IMPLEMENTATION_LIMITS,
    unsupported_boundaries: UNSUPPORTED_RUNTIME_BOUNDARIES,
};

const SUPPORTED_RUNTIME_FEATURES: &[RuntimeFeature] = &[
    RuntimeFeature::BoundedMailbox,
    RuntimeFeature::ComponentCompositionMetadata,
    RuntimeFeature::EmitEffect,
    RuntimeFeature::JsonlTrace,
    RuntimeFeature::LocalExecution,
    RuntimeFeature::LocalSend,
    RuntimeFeature::LocalSpawn,
    RuntimeFeature::LocalSupervision,
    RuntimeFeature::RuntimeBranching,
    RuntimeFeature::RuntimeForEach,
    RuntimeFeature::ScalarValueTemplates,
    RuntimeFeature::TypedBoundaryTables,
    RuntimeFeature::TypedEffectOutcomes,
    RuntimeFeature::TypedValueTemplates,
];

const IMPLEMENTATION_LIMITS: &[RuntimeFeature] = &[
    RuntimeFeature::DistributedTransport,
    RuntimeFeature::RemoteSend,
    RuntimeFeature::RemoteSpawn,
];

const UNSUPPORTED_RUNTIME_BOUNDARIES: &[UnsupportedRuntimeBoundary] = &[
    UnsupportedRuntimeBoundary {
        feature: RuntimeFeature::DistributedTransport,
        boundary: "distributed_transport",
        required_authority: "admitted_transport_profile",
        fail_stage: "before_artifact_loaded",
    },
    UnsupportedRuntimeBoundary {
        feature: RuntimeFeature::RemoteSend,
        boundary: "remote_process_send",
        required_authority: "admitted_transport_profile_and_remote_process_authority",
        fail_stage: "before_artifact_loaded",
    },
    UnsupportedRuntimeBoundary {
        feature: RuntimeFeature::RemoteSpawn,
        boundary: "remote_process_spawn",
        required_authority: "admitted_node_spawn_authority_and_cluster_membership",
        fail_stage: "before_artifact_loaded",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeTargetProfile {
    pub(crate) id: &'static str,
    pub(crate) execution_scope: &'static str,
    pub(crate) host_authority: &'static str,
    pub(crate) network_authority: &'static str,
    pub(crate) transport_authority: &'static str,
    pub(crate) cluster_membership: &'static str,
    pub(crate) remote_operation_policy: &'static str,
    pub(crate) unsupported_fail_stage: &'static str,
    supported_features: &'static [RuntimeFeature],
    implementation_limits: &'static [RuntimeFeature],
    unsupported_boundaries: &'static [UnsupportedRuntimeBoundary],
}

impl RuntimeTargetProfile {
    pub(crate) fn supported_features(self) -> &'static [RuntimeFeature] {
        self.supported_features
    }

    pub(crate) fn implementation_limits(self) -> &'static [RuntimeFeature] {
        self.implementation_limits
    }

    pub(crate) fn unsupported_boundaries(self) -> &'static [UnsupportedRuntimeBoundary] {
        self.unsupported_boundaries
    }

    pub(crate) fn unsupported_boundary(
        self,
        feature: RuntimeFeature,
    ) -> Option<&'static UnsupportedRuntimeBoundary> {
        self.unsupported_boundaries
            .iter()
            .find(|boundary| boundary.feature == feature)
    }

    pub(crate) fn validate_features(self, features: &[RuntimeFeature]) -> Result<()> {
        for feature in features {
            if self.supported_features.contains(feature) {
                continue;
            }
            if let Some(boundary) = self.unsupported_boundary(*feature) {
                return Err(Error::new(format!(
                    "target runtime feature {} is not supported by this Mantle runtime (profile {} rejects boundary {} at {} without {})",
                    feature.as_str(),
                    self.id,
                    boundary.boundary,
                    boundary.fail_stage,
                    boundary.required_authority,
                )));
            }
            return Err(Error::new(format!(
                "target runtime feature {} is not supported by this Mantle runtime (profile {} does not declare the feature)",
                feature.as_str(),
                self.id,
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedRuntimeBoundary {
    pub(crate) feature: RuntimeFeature,
    pub(crate) boundary: &'static str,
    pub(crate) required_authority: &'static str,
    pub(crate) fail_stage: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_runtime_target_profile_classifies_every_runtime_feature() {
        let profile = LOCAL_RUNTIME_TARGET_PROFILE;

        for feature in RuntimeFeature::ALL {
            let supported = profile.supported_features().contains(&feature);
            let unsupported = profile.unsupported_boundary(feature).is_some();
            assert_ne!(
                supported,
                unsupported,
                "runtime feature {} must be classified exactly once",
                feature.as_str()
            );
        }
        assert_eq!(
            profile.supported_features().len() + profile.implementation_limits().len(),
            RuntimeFeature::COUNT
        );
        let boundary_features = profile
            .unsupported_boundaries()
            .iter()
            .map(|boundary| boundary.feature)
            .collect::<Vec<_>>();
        assert_eq!(
            profile.implementation_limits(),
            boundary_features.as_slice()
        );
    }

    #[test]
    fn local_runtime_target_profile_rejects_remote_distributed_features() {
        let profile = LOCAL_RUNTIME_TARGET_PROFILE;

        for feature in profile.implementation_limits() {
            let err = profile
                .validate_features(&[*feature])
                .expect_err("remote/distributed feature must stay non-admitted");

            assert!(err.to_string().contains(feature.as_str()), "{err}");
            assert!(err.to_string().contains(profile.id), "{err}");
            assert!(
                err.to_string().contains(profile.unsupported_fail_stage),
                "{err}"
            );
        }
    }

    #[test]
    fn local_runtime_target_profile_supports_only_local_runtime_features() {
        let profile = LOCAL_RUNTIME_TARGET_PROFILE;

        profile
            .validate_features(profile.supported_features())
            .expect("all supported profile features should admit");
        for unsupported in [
            RuntimeFeature::DistributedTransport,
            RuntimeFeature::RemoteSend,
            RuntimeFeature::RemoteSpawn,
        ] {
            assert!(!profile.supported_features().contains(&unsupported));
        }
    }
}
