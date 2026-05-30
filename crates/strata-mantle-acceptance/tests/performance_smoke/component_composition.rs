use std::hint::black_box;
use std::path::Path;

use super::{BenchmarkProfile, PerformanceBudget, assert_within_budget, measure_for};

const SOURCE_PATH: &str = "../../examples/component_composition_main.str";
pub(super) const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.check_lower",
    label: "component_composition_main load+check+lower",
};

pub(super) fn run_check_lower_profile() {
    let budget = PerformanceBudget::load(CHECK_LOWER_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let metrics = measure_for(budget.iterations, || {
        let loaded = strata::load_root_source_program(&source_path)
            .expect("component composition performance smoke source should load");
        let (program, source_hash) = loaded.into_parts();
        let checked = strata::language::check_source_program(program)
            .expect("component composition performance smoke source should check");
        let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
            .expect("component composition performance smoke source should lower");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}
