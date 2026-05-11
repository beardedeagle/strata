#![forbid(unsafe_code)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use mantle_runtime::{InMemoryRuntimeHost, RunLimits, run_artifact_with_host};

const COLLECTION_STATE_SOURCE: &str = include_str!("../../../examples/collection_state.str");
const COMPILATION_ITERATIONS: usize = 64;
const RUNTIME_ITERATIONS: usize = 64;
const COMPILATION_BUDGET: Duration = Duration::from_secs(5);
const RUNTIME_BUDGET: Duration = Duration::from_secs(5);
const PERF_RUN_LIMITS: RunLimits = RunLimits {
    max_dispatches: 128,
    max_runtime_processes: 128,
    max_trace_bytes: 256 * 1024,
    max_emitted_output_bytes: 64 * 1024,
};

#[test]
#[ignore = "run through `just performance-smoke` so timing checks stay explicit"]
fn collection_state_compilation_and_runtime_performance_smoke() {
    let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should check");
    let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should lower");
    run_collection_state_artifact(&artifact);

    let compilation_elapsed = elapsed_for(COMPILATION_ITERATIONS, || {
        let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should check");
        let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should lower");
        black_box(artifact);
    });

    let runtime_elapsed = elapsed_for(RUNTIME_ITERATIONS, || {
        let report = run_collection_state_artifact(&artifact);
        black_box(report);
    });

    assert_within_budget(
        "collection_state check+lower",
        compilation_elapsed,
        COMPILATION_ITERATIONS,
        COMPILATION_BUDGET,
    );
    assert_within_budget(
        "collection_state in-memory runtime",
        runtime_elapsed,
        RUNTIME_ITERATIONS,
        RUNTIME_BUDGET,
    );
}

fn elapsed_for(iterations: usize, mut operation: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    start.elapsed()
}

fn run_collection_state_artifact(
    artifact: &mantle_artifact::MantleArtifact,
) -> mantle_runtime::RuntimeReport {
    let mut host = InMemoryRuntimeHost::default();
    let report = run_artifact_with_host(artifact, &mut host, PERF_RUN_LIMITS)
        .expect("performance smoke artifact should run");
    assert_eq!(report.spawned_processes.len(), 3);
    assert_eq!(report.delivered_messages.len(), 3);
    assert_eq!(report.emitted_outputs.len(), 2);
    report
}

fn assert_within_budget(label: &str, elapsed: Duration, iterations: usize, budget: Duration) {
    assert!(
        elapsed <= budget,
        "{label} exceeded performance smoke budget: {elapsed:?} for {iterations} iterations, budget {budget:?}"
    );
    println!(
        "{label}: {elapsed:?} total for {iterations} iterations ({:?} per iteration)",
        elapsed / iterations as u32
    );
}
