#![no_main]

use libfuzzer_sys::fuzz_target;
use mantle_runtime::{run_artifact_with_host, InMemoryRuntimeHost, RunLimits};

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(checked) = strata::language::check_source(source) else {
        return;
    };

    let Ok(artifact) = strata::language::lower_to_artifact(&checked, source) else {
        return;
    };

    let mut host = InMemoryRuntimeHost::default();
    let _ = run_artifact_with_host(&artifact, &mut host, RunLimits::default());
});
