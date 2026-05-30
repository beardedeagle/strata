use std::hint::black_box;
use std::path::Path;

use super::{BenchmarkProfile, PerformanceBudget, assert_within_budget, measure_for};

const SOURCE_PATH: &str = "../../examples/component_composition_main.str";
pub(super) const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.check_lower",
    label: "component_composition_main load+check+lower",
};
pub(super) const REPORT_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.composition_report",
    label: "component_composition_main composition-report render",
};
pub(super) const TARGET_REQUIREMENTS_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.target_requirements",
    label: "component_composition_main target-requirements render",
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

pub(super) fn run_report_profile() {
    let budget = PerformanceBudget::load(REPORT_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("component composition report performance smoke source should load");
    let (program, _) = loaded.into_parts();
    let report_input = strata::language::CompositionAdmissionReport::from_source_program(program)
        .expect("component composition report performance smoke source should check");
    let metrics = measure_for(budget.iterations, || {
        let report = strata::language::render_composition_admission_report(
            black_box(&report_input),
            SOURCE_PATH,
            strata::language::CompositionAdmissionReportFormat::Json,
        );
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_target_requirements_profile() {
    let budget = PerformanceBudget::load(TARGET_REQUIREMENTS_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("target requirements performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("target requirements performance smoke source should check");
    let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("target requirements performance smoke source should lower");
    let metrics = measure_for(budget.iterations, || {
        let report = mantle_artifact::render_artifact_target_requirements(
            black_box(&artifact),
            SOURCE_PATH,
            mantle_artifact::TargetRequirementsFormat::Json,
        )
        .expect("target requirements should render");
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}
