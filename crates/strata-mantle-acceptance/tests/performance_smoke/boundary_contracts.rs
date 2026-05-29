use std::hint::black_box;
use std::path::Path;

use mantle_runtime::{InMemoryRuntimeHost, run_artifact_with_host};

use super::{
    BenchmarkProfile, PERF_RUN_LIMITS, PerformanceBudget, assert_within_budget, measure_for,
};

const SOURCE_PATH: &str = "../../examples/boundary_contracts_main.str";
pub(super) const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "boundary_contracts_main.check_lower",
    label: "boundary_contracts_main load+check+lower",
};
pub(super) const RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "boundary_contracts_main.in_memory_runtime",
    label: "boundary_contracts_main in-memory runtime",
};

pub(super) fn run_check_lower_profile() {
    let budget = PerformanceBudget::load(CHECK_LOWER_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let metrics = measure_for(budget.iterations, || {
        let loaded = strata::load_root_source_program(&source_path)
            .expect("boundary performance smoke source should load");
        let (program, source_hash) = loaded.into_parts();
        let checked = strata::language::check_source_program(program)
            .expect("boundary performance smoke source should check");
        let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
            .expect("boundary performance smoke source should lower");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_runtime_profile() {
    let budget = PerformanceBudget::load(RUNTIME_PROFILE);
    let artifact = boundary_contracts_artifact();
    let metrics = measure_for(budget.iterations, || {
        let report = run_boundary_contracts_artifact(&artifact);
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

fn boundary_contracts_artifact() -> mantle_artifact::MantleArtifact {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded =
        strata::load_root_source_program(&source_path).expect("boundary smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("boundary smoke source should check");
    let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("boundary smoke source should lower");
    run_boundary_contracts_artifact(&artifact);
    artifact
}

fn run_boundary_contracts_artifact(
    artifact: &mantle_artifact::MantleArtifact,
) -> mantle_runtime::RuntimeReport {
    let mut host = InMemoryRuntimeHost::default();
    let report = run_artifact_with_host(artifact, &mut host, PERF_RUN_LIMITS)
        .expect("boundary performance smoke artifact should run");
    assert_eq!(report.spawned_processes.len(), 2);
    assert_eq!(report.delivered_messages.len(), 2);
    assert_eq!(report.emitted_outputs.len(), 1);
    report
}
