#![no_main]

use libfuzzer_sys::fuzz_target;
use mantle_runtime::{
    InMemoryRuntimeHost, RunLimits, SpawnAuthorityPolicy, run_artifact_with_host,
};

const DEFAULT_FUZZ_RUN_LIMITS: RunLimits = RunLimits {
    max_dispatches: 128,
    max_runtime_processes: 512,
    max_trace_bytes: 256 * 1024,
    max_emitted_output_bytes: 64 * 1024,
    spawn_authority_policy: SpawnAuthorityPolicy::AdmitDeclared,
};

const EXHAUSTED_SPAWN_FUZZ_RUN_LIMITS: RunLimits = RunLimits {
    max_dispatches: 128,
    max_runtime_processes: 1,
    max_trace_bytes: 256 * 1024,
    max_emitted_output_bytes: 64 * 1024,
    spawn_authority_policy: SpawnAuthorityPolicy::AdmitDeclared,
};

const FUZZ_RUN_LIMITS: &[RunLimits] = &[DEFAULT_FUZZ_RUN_LIMITS, EXHAUSTED_SPAWN_FUZZ_RUN_LIMITS];

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(checked) = strata::language::check_source(source) else {
        return;
    };

    let artifact = strata::language::lower_to_artifact(&checked, source)
        .expect("checked source should lower to a valid artifact");

    for limits in FUZZ_RUN_LIMITS {
        let mut host = InMemoryRuntimeHost::default();
        let _ = run_artifact_with_host(&artifact, &mut host, *limits);
    }
});
