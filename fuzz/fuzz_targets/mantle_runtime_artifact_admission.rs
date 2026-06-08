#![no_main]

use libfuzzer_sys::fuzz_target;
use mantle_artifact::{MantleArtifact, RuntimeFeature};
use mantle_runtime::{
    InMemoryRuntimeHost, LocalSpawnBackend, RunLimits, SpawnAuthorityPolicy, run_artifact_with_host,
};

const FUZZ_RUN_LIMITS: RunLimits = RunLimits {
    max_dispatches: 128,
    max_runtime_processes: 512,
    max_trace_bytes: 256 * 1024,
    max_emitted_output_bytes: 64 * 1024,
    spawn_authority_policy: SpawnAuthorityPolicy::AdmitDeclared,
    local_spawn_backend: LocalSpawnBackend::Available,
};

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(artifact) = MantleArtifact::decode(contents) else {
        return;
    };

    let mut host = InMemoryRuntimeHost::default();
    let result = run_artifact_with_host(&artifact, &mut host, FUZZ_RUN_LIMITS);
    if declares_remote_or_distributed_target_requirement(&artifact) {
        assert!(result.is_err());
        assert!(host.events().is_empty());
    }
});

fn declares_remote_or_distributed_target_requirement(artifact: &MantleArtifact) -> bool {
    artifact.target_requirements.features.iter().any(|feature| {
        matches!(
            feature,
            RuntimeFeature::DistributedTransport
                | RuntimeFeature::RemoteSend
                | RuntimeFeature::RemoteSpawn
        )
    })
}
