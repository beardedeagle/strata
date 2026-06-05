use std::hint::black_box;
use std::path::Path;

use super::{BenchmarkProfile, PerformanceBudget, assert_within_budget, measure_for};

const SOURCE_PATH: &str = "../../examples/effect_outcome_spawn_denied.str";
const COMPONENT_SOURCE_PATH: &str = "../../examples/component_composition_main.str";
const RUNTIME_RUN_ARTIFACT_PATH: &str =
    "target/performance-smoke/effect_outcome_spawn_denied.authority-effect.mta";
const RUNTIME_RUN_BINDING_PATH: &str =
    "target/performance-smoke/effect_outcome_spawn_denied.authority-effect-binding.json";

pub(super) const ARTIFACT_BUILD_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_effect_artifact_build",
    label: "effect_outcome_spawn_denied authority/effect artifact build",
};
pub(super) const ARTIFACT_ADMIT_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_effect_artifact_admit",
    label: "effect_outcome_spawn_denied authority/effect artifact admit",
};
pub(super) const POLICY_BUILD_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_policy_artifact_build",
    label: "effect_outcome_spawn_denied authority policy artifact build",
};
pub(super) const POLICY_ADMIT_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_policy_artifact_admit",
    label: "effect_outcome_spawn_denied authority policy artifact admit",
};
pub(super) const RUNTIME_BINDING_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_effect_runtime_binding",
    label: "effect_outcome_spawn_denied authority/effect runtime binding",
};
pub(super) const COMPONENT_RUNTIME_BINDING_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "component_composition_main.authority_effect_runtime_binding",
    label: "component_composition_main authority/effect runtime binding",
};
pub(super) const RUNTIME_RUN_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "effect_outcome_spawn_denied.authority_effect_runtime_run",
    label: "effect_outcome_spawn_denied authority/effect runtime run",
};

pub(super) fn run_artifact_build_profile() {
    let budget = PerformanceBudget::load(ARTIFACT_BUILD_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("authority/effect performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("authority/effect performance smoke source should check");
    let metrics = measure_for(budget.iterations, || {
        let artifact = strata::language::render_authority_effect_artifact(
            black_box(&checked),
            SOURCE_PATH,
            black_box(&source_hash),
        )
        .expect("authority/effect artifact should render");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_artifact_admit_profile() {
    let budget = PerformanceBudget::load(ARTIFACT_ADMIT_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("authority/effect admission performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("authority/effect admission performance smoke source should check");
    let artifact =
        strata::language::render_authority_effect_artifact(&checked, SOURCE_PATH, &source_hash)
            .expect("authority/effect artifact should render");
    let metrics = measure_for(budget.iterations, || {
        let summary = strata::language::admit_authority_effect_artifact(black_box(&artifact))
            .expect("authority/effect artifact should admit");
        black_box(summary);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_policy_build_profile() {
    let budget = PerformanceBudget::load(POLICY_BUILD_PROFILE);
    let authority_effect = authority_effect_artifact_for_source(SOURCE_PATH);
    let options = strata::language::AuthorityPolicyBuildOptions {
        spawn_authority_decision: strata::language::AuthorityPolicyDecision::Deny,
        port_authority_decision: strata::language::AuthorityPolicyDecision::Admit,
    };
    let metrics = measure_for(budget.iterations, || {
        let policy = strata::language::render_authority_policy_artifact(
            black_box(&authority_effect),
            options,
        )
        .expect("authority policy artifact should render");
        black_box(policy);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_policy_admit_profile() {
    let budget = PerformanceBudget::load(POLICY_ADMIT_PROFILE);
    let authority_effect = authority_effect_artifact_for_source(SOURCE_PATH);
    let policy = strata::language::render_authority_policy_artifact(
        &authority_effect,
        strata::language::AuthorityPolicyBuildOptions {
            spawn_authority_decision: strata::language::AuthorityPolicyDecision::Deny,
            port_authority_decision: strata::language::AuthorityPolicyDecision::Admit,
        },
    )
    .expect("authority policy artifact should render");
    let metrics = measure_for(budget.iterations, || {
        let summary = strata::language::admit_authority_policy_artifact(
            black_box(&policy),
            black_box(&authority_effect),
        )
        .expect("authority policy artifact should admit");
        black_box(summary);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_runtime_binding_profile() {
    let budget = PerformanceBudget::load(RUNTIME_BINDING_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("authority/effect runtime binding performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("authority/effect runtime binding performance smoke source should check");
    let authority_effect =
        strata::language::render_authority_effect_artifact(&checked, SOURCE_PATH, &source_hash)
            .expect("authority/effect artifact should render");
    let policy = strata::language::render_authority_policy_artifact(
        &authority_effect,
        strata::language::AuthorityPolicyBuildOptions {
            spawn_authority_decision: strata::language::AuthorityPolicyDecision::Deny,
            port_authority_decision: strata::language::AuthorityPolicyDecision::Admit,
        },
    )
    .expect("authority policy artifact should render");
    let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("authority/effect runtime binding performance smoke source should lower");
    let metrics = measure_for(budget.iterations, || {
        let binding = strata::language::render_runtime_authority_effect_binding(
            black_box(&authority_effect),
            black_box(&policy),
            black_box(&artifact),
        )
        .expect("authority/effect runtime binding should render");
        mantle_runtime::validate_runtime_authority_effect_binding_text(
            black_box(&binding),
            black_box(&artifact),
        )
        .expect("authority/effect runtime binding should admit");
        black_box(binding);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_component_runtime_binding_profile() {
    let budget = PerformanceBudget::load(COMPONENT_RUNTIME_BINDING_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(COMPONENT_SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("component authority/effect runtime binding smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("component authority/effect runtime binding smoke source should check");
    let authority_effect = strata::language::render_authority_effect_artifact(
        &checked,
        COMPONENT_SOURCE_PATH,
        &source_hash,
    )
    .expect("component authority/effect artifact should render");
    let policy = strata::language::render_authority_policy_artifact(
        &authority_effect,
        strata::language::AuthorityPolicyBuildOptions::default(),
    )
    .expect("component authority policy artifact should render");
    let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("component authority/effect runtime binding smoke source should lower");
    let metrics = measure_for(budget.iterations, || {
        let binding = strata::language::render_runtime_authority_effect_binding(
            black_box(&authority_effect),
            black_box(&policy),
            black_box(&artifact),
        )
        .expect("component authority/effect runtime binding should render");
        mantle_runtime::validate_runtime_authority_effect_binding_text(
            black_box(&binding),
            black_box(&artifact),
        )
        .expect("component authority/effect runtime binding should admit");
        black_box(binding);
    });
    assert_within_budget(budget, metrics);
}

pub(super) fn run_runtime_run_profile() {
    let budget = PerformanceBudget::load(RUNTIME_RUN_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_PATH);
    let loaded = strata::load_root_source_program(&source_path)
        .expect("authority/effect runtime-run performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("authority/effect runtime-run performance smoke source should check");
    let authority_effect =
        strata::language::render_authority_effect_artifact(&checked, SOURCE_PATH, &source_hash)
            .expect("authority/effect artifact should render");
    let policy = strata::language::render_authority_policy_artifact(
        &authority_effect,
        strata::language::AuthorityPolicyBuildOptions {
            spawn_authority_decision: strata::language::AuthorityPolicyDecision::Deny,
            port_authority_decision: strata::language::AuthorityPolicyDecision::Admit,
        },
    )
    .expect("authority policy artifact should render");
    let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
        .expect("authority/effect runtime-run performance smoke source should lower");
    let binding = strata::language::render_runtime_authority_effect_binding(
        &authority_effect,
        &policy,
        &artifact,
    )
    .expect("authority/effect runtime binding should render");
    let artifact_path = Path::new(RUNTIME_RUN_ARTIFACT_PATH);
    let binding_path = Path::new(RUNTIME_RUN_BINDING_PATH);
    mantle_artifact::write_artifact(artifact_path, &artifact)
        .expect("authority/effect runtime-run artifact should write");
    mantle_artifact::write_text_artifact(binding_path, &binding)
        .expect("authority/effect runtime-run binding should write");

    let metrics = measure_for(budget.iterations, || {
        let report = mantle_runtime::run_artifact_path_with_limits_and_authority_effect_binding(
            black_box(artifact_path),
            super::PERF_RUN_LIMITS,
            black_box(binding_path),
        )
        .expect("authority/effect runtime run should execute with binding");
        assert!(
            report
                .emitted_outputs
                .iter()
                .any(|output| output == "spawn denied"),
            "authority/effect runtime run should apply the denied binding policy"
        );
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

fn authority_effect_artifact_for_source(source_path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(source_path);
    let loaded = strata::load_root_source_program(&path)
        .expect("authority/effect performance smoke source should load");
    let (program, source_hash) = loaded.into_parts();
    let checked = strata::language::check_source_program(program)
        .expect("authority/effect performance smoke source should check");
    strata::language::render_authority_effect_artifact(&checked, source_path, &source_hash)
        .expect("authority/effect artifact should render")
}
