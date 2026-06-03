use std::hint::black_box;
use std::path::Path;

use super::{BenchmarkProfile, PerformanceBudget, assert_within_budget, measure_for};

const SOURCE_PATH: &str = "../../examples/component_composition_main.str";
const RUNTIME_BINDING_ARTIFACT_PATH: &str =
    "target/performance-smoke/component_composition_main.mta";
const RUNTIME_BINDING_PATH: &str =
    "target/performance-smoke/component_composition_main.deployment-composition.json";
pub(super) const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.check_lower",
    label: "component_composition_main load+check+lower",
};
pub(super) const REPORT_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.composition_report",
    label: "component_composition_main composition-report render",
};
pub(super) const ARTIFACT_BUILD_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.composition_artifact_build",
    label: "component_composition_main composition artifact build",
};
pub(super) const ARTIFACT_ADMIT_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.composition_artifact_admit",
    label: "component_composition_main composition artifact admit",
};
pub(super) const TARGET_REQUIREMENTS_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.target_requirements",
    label: "component_composition_main target-requirements render",
};
pub(super) const RUNTIME_BINDING_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.runtime_binding_run",
    label: "component_composition_main Mantle run with composition binding",
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

pub(super) fn run_artifact_build_profile() {
    let budget = PerformanceBudget::load(ARTIFACT_BUILD_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("component composition artifact performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("component composition artifact performance smoke source should check");
    let metrics = measure_for(budget.iterations, || {
        let artifact = strata::language::render_component_composition_artifact(
            black_box(&checked),
            SOURCE_PATH,
            black_box(&source_hash),
            None,
        )
        .expect("component composition artifact should render");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_artifact_admit_profile() {
    let budget = PerformanceBudget::load(ARTIFACT_ADMIT_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("component composition artifact admit performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("component composition artifact admit performance smoke source should check");
    let artifact = strata::language::render_component_composition_artifact(
        &checked,
        SOURCE_PATH,
        &source_hash,
        None,
    )
    .expect("component composition artifact should render");
    let metrics = measure_for(budget.iterations, || {
        let summary = strata::language::admit_component_composition_artifact(black_box(&artifact))
            .expect("component composition artifact should admit");
        black_box(summary);
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

pub(super) fn run_runtime_binding_profile() {
    let budget = PerformanceBudget::load(RUNTIME_BINDING_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("runtime binding performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("runtime binding performance smoke source should check");
    let artifact =
        strata::language::lower_to_artifact_with_source_hash(&checked, source_hash.clone())
            .expect("runtime binding performance smoke source should lower");
    let composition_artifact = strata::language::render_component_composition_artifact(
        &checked,
        SOURCE_PATH,
        &source_hash,
        None,
    )
    .expect("component composition artifact should render");
    let runtime_binding =
        strata::language::render_runtime_composition_binding(&composition_artifact, &artifact)
            .expect("runtime composition binding should render");
    let artifact_path = Path::new(RUNTIME_BINDING_ARTIFACT_PATH);
    let binding_path = Path::new(RUNTIME_BINDING_PATH);
    mantle_artifact::write_artifact(artifact_path, &artifact)
        .expect("runtime binding performance artifact should write");
    mantle_artifact::write_text_artifact(binding_path, &runtime_binding)
        .expect("runtime binding performance artifact should write");

    let metrics = measure_for(budget.iterations, || {
        let report = mantle_runtime::run_artifact_path_with_limits_and_composition_binding(
            black_box(artifact_path),
            super::PERF_RUN_LIMITS,
            black_box(binding_path),
        )
        .expect("performance smoke runtime composition binding should run");
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}
