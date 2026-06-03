#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use mantle_runtime::{
    InMemoryRuntimeHost, LocalSpawnBackend, RunLimits, SpawnAuthorityPolicy,
    run_artifact_with_host, run_artifact_with_limits,
};

const PERFORMANCE_BASELINE: &str = include_str!("../../../benchmarks/performance-smoke.baseline");
const COLLECTION_STATE_SOURCE: &str = include_str!("../../../examples/collection_state.str");
const LOCAL_SUPERVISION_SOURCE: &str =
    include_str!("../../../examples/local_supervision_restart.str");
const IMPORTS_MAIN_SOURCE_PATH: &str = "../../examples/imports_main.str";
const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.check_lower",
    label: "collection_state check+lower",
};
const IMPORTS_CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "imports_main.check_lower",
    label: "imports_main load+check+lower",
};
const IN_MEMORY_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.in_memory_runtime",
    label: "collection_state in-memory runtime",
};
const ARTIFACT_CODEC_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.artifact_codec",
    label: "collection_state artifact encode+decode",
};
const JSONL_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.jsonl_runtime",
    label: "collection_state JSONL runtime",
};
const LOCAL_SUPERVISION_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "local_supervision_restart.in_memory_runtime",
    label: "local_supervision_restart in-memory runtime",
};
const PROFILE_SELECTOR_ENV: &str = "STRATA_PERFORMANCE_SMOKE_PROFILE";
const ALL_PROFILES: [BenchmarkProfile; 14] = [
    CHECK_LOWER_PROFILE,
    IMPORTS_CHECK_LOWER_PROFILE,
    boundary_contracts::CHECK_LOWER_PROFILE,
    component_composition::CHECK_LOWER_PROFILE,
    component_composition::REPORT_PROFILE,
    component_composition::ARTIFACT_BUILD_PROFILE,
    component_composition::ARTIFACT_ADMIT_PROFILE,
    component_composition::TARGET_REQUIREMENTS_PROFILE,
    component_composition::RUNTIME_BINDING_PROFILE,
    IN_MEMORY_RUNTIME_PROFILE,
    boundary_contracts::RUNTIME_PROFILE,
    ARTIFACT_CODEC_PROFILE,
    JSONL_RUNTIME_PROFILE,
    LOCAL_SUPERVISION_RUNTIME_PROFILE,
];
const PROFILE_KEY_LIST: &str = "collection_state.check_lower, imports_main.check_lower, boundary_contracts_main.check_lower, component_composition_main.check_lower, component_composition_main.composition_report, component_composition_main.composition_artifact_build, component_composition_main.composition_artifact_admit, component_composition_main.target_requirements, component_composition_main.runtime_binding_run, collection_state.in_memory_runtime, boundary_contracts_main.in_memory_runtime, collection_state.artifact_codec, collection_state.jsonl_runtime, local_supervision_restart.in_memory_runtime";
const JSONL_RUNTIME_ARTIFACT_PATH: &str = "target/performance-smoke/collection_state.mta";
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
))]
const RESOURCE_METRICS_REQUIRED: bool = true;
#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
const RESOURCE_METRICS_REQUIRED: bool = false;
const PERF_RUN_LIMITS: RunLimits = RunLimits {
    max_dispatches: 128,
    max_runtime_processes: 128,
    max_trace_bytes: 256 * 1024,
    max_emitted_output_bytes: 64 * 1024,
    spawn_authority_policy: SpawnAuthorityPolicy::AdmitDeclared,
    local_spawn_backend: LocalSpawnBackend::Available,
};
#[test]
#[ignore = "run through `just performance-smoke` so timing checks stay explicit"]
fn collection_state_compilation_and_runtime_performance_smoke() {
    let selected_profile = std::env::var(PROFILE_SELECTOR_ENV).ok();
    let selected_profile = selected_profile.as_deref();
    profile_selection::validate_selected_profile(selected_profile, &ALL_PROFILES, PROFILE_KEY_LIST);

    macro_rules! run_profile {
        ($profile:expr, $run:path) => {
            if profile_selection::profile_is_selected(selected_profile, $profile) {
                $run();
            }
        };
    }

    run_profile!(CHECK_LOWER_PROFILE, run_check_lower_profile);
    run_profile!(IMPORTS_CHECK_LOWER_PROFILE, run_imports_check_lower_profile);
    run_profile!(
        boundary_contracts::CHECK_LOWER_PROFILE,
        boundary_contracts::run_check_lower_profile
    );
    run_profile!(
        component_composition::CHECK_LOWER_PROFILE,
        component_composition::run_check_lower_profile
    );
    run_profile!(
        component_composition::REPORT_PROFILE,
        component_composition::run_report_profile
    );
    run_profile!(
        component_composition::ARTIFACT_BUILD_PROFILE,
        component_composition::run_artifact_build_profile
    );
    run_profile!(
        component_composition::ARTIFACT_ADMIT_PROFILE,
        component_composition::run_artifact_admit_profile
    );
    run_profile!(
        component_composition::TARGET_REQUIREMENTS_PROFILE,
        component_composition::run_target_requirements_profile
    );
    run_profile!(
        component_composition::RUNTIME_BINDING_PROFILE,
        component_composition::run_runtime_binding_profile
    );
    run_profile!(IN_MEMORY_RUNTIME_PROFILE, run_in_memory_runtime_profile);
    run_profile!(
        boundary_contracts::RUNTIME_PROFILE,
        boundary_contracts::run_runtime_profile
    );
    run_profile!(ARTIFACT_CODEC_PROFILE, run_artifact_codec_profile);
    run_profile!(JSONL_RUNTIME_PROFILE, run_jsonl_runtime_profile);
    run_profile!(
        LOCAL_SUPERVISION_RUNTIME_PROFILE,
        run_local_supervision_runtime_profile
    );
}

fn run_check_lower_profile() {
    let budget = PerformanceBudget::load(CHECK_LOWER_PROFILE);
    let metrics = measure_for(budget.iterations, || {
        let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should check");
        let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should lower");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}

fn run_imports_check_lower_profile() {
    let budget = PerformanceBudget::load(IMPORTS_CHECK_LOWER_PROFILE);
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(IMPORTS_MAIN_SOURCE_PATH);
    let metrics = measure_for(budget.iterations, || {
        let loaded = strata::load_root_source_program(&source_path)
            .expect("imports performance smoke source should load");
        let (program, source_hash) = loaded.into_parts();
        let checked = strata::language::check_source_program(program)
            .expect("imports performance smoke source should check");
        let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
            .expect("imports performance smoke source should lower");
        black_box(artifact);
    });
    assert_within_budget(budget, metrics);
}

fn collection_state_artifact() -> mantle_artifact::MantleArtifact {
    let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should check");
    let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should lower");
    run_collection_state_artifact(&artifact);
    artifact
}

fn run_in_memory_runtime_profile() {
    let budget = PerformanceBudget::load(IN_MEMORY_RUNTIME_PROFILE);
    let artifact = collection_state_artifact();
    let metrics = measure_for(budget.iterations, || {
        let report = run_collection_state_artifact(&artifact);
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

fn run_artifact_codec_profile() {
    let budget = PerformanceBudget::load(ARTIFACT_CODEC_PROFILE);
    let artifact = collection_state_artifact();
    let metrics = measure_for(budget.iterations, || {
        let encoded = artifact.encode();
        let decoded = mantle_artifact::MantleArtifact::decode(&encoded)
            .expect("performance smoke encoded artifact should decode");
        black_box(decoded);
    });
    assert_within_budget(budget, metrics);
}

fn run_jsonl_runtime_profile() {
    let budget = PerformanceBudget::load(JSONL_RUNTIME_PROFILE);
    let artifact = collection_state_artifact();
    run_collection_state_artifact_with_jsonl_trace(&artifact);
    let metrics = measure_for(budget.iterations, || {
        let report = run_collection_state_artifact_with_jsonl_trace(&artifact);
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

fn local_supervision_artifact() -> mantle_artifact::MantleArtifact {
    let checked = strata::language::check_source(LOCAL_SUPERVISION_SOURCE)
        .expect("local supervision performance smoke source should check");
    let artifact = strata::language::lower_to_artifact(&checked, LOCAL_SUPERVISION_SOURCE)
        .expect("local supervision performance smoke source should lower");
    run_local_supervision_artifact(&artifact);
    artifact
}

fn run_local_supervision_runtime_profile() {
    let budget = PerformanceBudget::load(LOCAL_SUPERVISION_RUNTIME_PROFILE);
    let artifact = local_supervision_artifact();
    let metrics = measure_for(budget.iterations, || {
        let report = run_local_supervision_artifact(&artifact);
        black_box(report);
    });
    assert_within_budget(budget, metrics);
}

#[derive(Clone, Copy, Debug)]
struct BenchmarkProfile {
    key: &'static str,
    label: &'static str,
}

#[allow(unsafe_code)]
#[path = "performance_smoke/allocation_meter.rs"]
mod allocation_meter;
#[path = "performance_smoke/boundary_contracts.rs"]
mod boundary_contracts;
#[path = "performance_smoke/component_composition.rs"]
mod component_composition;
#[path = "performance_smoke/platform_resources.rs"]
mod platform_resources;
#[path = "performance_smoke/profile_selection.rs"]
mod profile_selection;

use platform_resources::{capture_cpu_time, capture_memory};
#[derive(Clone, Copy, Debug)]
struct PerformanceBudget {
    profile: BenchmarkProfile,
    iterations: usize,
    reference: ReferenceMetrics,
    wall_budget: Duration,
    cpu_budget: Duration,
    rss_budget_kib: u64,
    allocation_count_budget: u64,
    deallocation_count_budget: u64,
    allocated_bytes_budget: u64,
    deallocated_bytes_budget: u64,
    net_live_bytes_delta_budget: u64,
    peak_live_bytes_over_start_budget: u64,
}

#[derive(Clone, Copy, Debug)]
struct ReferenceMetrics {
    wall: Duration,
    cpu: Duration,
    rss_kib: u64,
    allocation_count: u64,
    deallocation_count: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    net_live_bytes_delta: i64,
    peak_live_bytes_over_start: u64,
}

impl PerformanceBudget {
    fn load(profile: BenchmarkProfile) -> Self {
        assert_eq!(
            baseline_u64("version"),
            1,
            "unsupported performance baseline version"
        );

        Self {
            profile,
            iterations: baseline_usize(&format!("{}.iterations", profile.key)),
            reference: ReferenceMetrics {
                wall: baseline_duration(&format!("{}.reference_wall_nanos", profile.key)),
                cpu: baseline_duration(&format!("{}.reference_cpu_nanos", profile.key)),
                rss_kib: baseline_u64(&format!("{}.reference_rss_kib", profile.key)),
                allocation_count: baseline_u64(&format!(
                    "{}.reference_allocation_count",
                    profile.key
                )),
                deallocation_count: baseline_u64(&format!(
                    "{}.reference_deallocation_count",
                    profile.key
                )),
                allocated_bytes: baseline_u64(&format!(
                    "{}.reference_allocated_bytes",
                    profile.key
                )),
                deallocated_bytes: baseline_u64(&format!(
                    "{}.reference_deallocated_bytes",
                    profile.key
                )),
                net_live_bytes_delta: baseline_i64(&format!(
                    "{}.reference_net_live_bytes_delta",
                    profile.key
                )),
                peak_live_bytes_over_start: baseline_u64(&format!(
                    "{}.reference_peak_live_bytes_over_start",
                    profile.key
                )),
            },
            wall_budget: baseline_duration(&format!("{}.wall_budget_nanos", profile.key)),
            cpu_budget: baseline_duration(&format!("{}.cpu_budget_nanos", profile.key)),
            rss_budget_kib: baseline_u64(&format!("{}.rss_budget_kib", profile.key)),
            allocation_count_budget: baseline_u64(&format!(
                "{}.allocation_count_budget",
                profile.key
            )),
            deallocation_count_budget: baseline_u64(&format!(
                "{}.deallocation_count_budget",
                profile.key
            )),
            allocated_bytes_budget: baseline_u64(&format!(
                "{}.allocated_bytes_budget",
                profile.key
            )),
            deallocated_bytes_budget: baseline_u64(&format!(
                "{}.deallocated_bytes_budget",
                profile.key
            )),
            net_live_bytes_delta_budget: baseline_u64(&format!(
                "{}.net_live_bytes_delta_budget",
                profile.key
            )),
            peak_live_bytes_over_start_budget: baseline_u64(&format!(
                "{}.peak_live_bytes_over_start_budget",
                profile.key
            )),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ResourceSnapshot {
    cpu: Option<Duration>,
    memory: Option<MemorySnapshot>,
    allocations: AllocationSnapshot,
}

#[derive(Clone, Copy, Debug)]
struct MemorySnapshot {
    current_rss_kib: u64,
    peak_rss_kib: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct ResourceMetrics {
    wall: Duration,
    cpu: Option<Duration>,
    current_rss_kib: Option<u64>,
    peak_rss_kib: Option<u64>,
    allocations: AllocationMetrics,
}

fn measure_for(iterations: usize, mut operation: impl FnMut()) -> ResourceMetrics {
    let start = ResourceSnapshot::capture_interval_start();
    let wall_start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let wall = wall_start.elapsed();
    let end = ResourceSnapshot::capture();
    start.measure_until(end, wall)
}

impl ResourceSnapshot {
    fn capture_interval_start() -> Self {
        let allocations = allocation_meter::capture_interval_start();
        Self::capture_with_allocations(allocations)
    }

    fn capture() -> Self {
        Self::capture_with_allocations(allocation_meter::capture())
    }

    fn capture_with_allocations(allocations: AllocationSnapshot) -> Self {
        let cpu = capture_cpu_time();
        let memory = capture_memory();
        if RESOURCE_METRICS_REQUIRED {
            assert!(cpu.is_some(), "performance smoke CPU metrics unavailable");
            assert!(
                memory.is_some(),
                "performance smoke RSS metrics unavailable"
            );
        }
        Self {
            cpu,
            memory,
            allocations,
        }
    }

    fn measure_until(self, end: Self, wall: Duration) -> ResourceMetrics {
        ResourceMetrics {
            wall,
            cpu: self
                .cpu
                .zip(end.cpu)
                .map(|(start, end)| end.saturating_sub(start)),
            current_rss_kib: end.memory.map(|memory| memory.current_rss_kib),
            peak_rss_kib: end.memory.and_then(|memory| memory.peak_rss_kib),
            allocations: self.allocations.measure_until(end.allocations),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AllocationSnapshot {
    allocations: u64,
    deallocations: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
struct AllocationMetrics {
    allocation_count: u64,
    deallocation_count: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    net_live_bytes_delta: i64,
    peak_live_bytes_over_start: u64,
}

impl AllocationSnapshot {
    fn measure_until(self, end: Self) -> AllocationMetrics {
        let interval_start_live_bytes = self.live_bytes;
        AllocationMetrics {
            allocation_count: end.allocations.saturating_sub(self.allocations),
            deallocation_count: end.deallocations.saturating_sub(self.deallocations),
            allocated_bytes: end.allocated_bytes.saturating_sub(self.allocated_bytes),
            deallocated_bytes: end.deallocated_bytes.saturating_sub(self.deallocated_bytes),
            net_live_bytes_delta: live_byte_delta(end.live_bytes, interval_start_live_bytes),
            peak_live_bytes_over_start: end
                .peak_live_bytes
                .saturating_sub(interval_start_live_bytes),
        }
    }
}

fn live_byte_delta(end: u64, start: u64) -> i64 {
    let delta = i128::from(end) - i128::from(start);
    i64::try_from(delta).expect("live-byte delta should fit i64")
}

#[test]
fn allocation_snapshot_reports_signed_net_live_byte_delta() {
    let start = AllocationSnapshot {
        allocations: 10,
        deallocations: 4,
        allocated_bytes: 160,
        deallocated_bytes: 32,
        live_bytes: 128,
        peak_live_bytes: 128,
    };
    let end = AllocationSnapshot {
        allocations: 11,
        deallocations: 5,
        allocated_bytes: 192,
        deallocated_bytes: 96,
        live_bytes: 96,
        peak_live_bytes: 160,
    };

    let metrics = start.measure_until(end);

    assert_eq!(metrics.net_live_bytes_delta, -32);
    assert_eq!(metrics.peak_live_bytes_over_start, 32);
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

fn run_collection_state_artifact_with_jsonl_trace(
    artifact: &mantle_artifact::MantleArtifact,
) -> mantle_runtime::RunReport {
    let artifact_path = Path::new(JSONL_RUNTIME_ARTIFACT_PATH);
    let report = run_artifact_with_limits(artifact_path, artifact, PERF_RUN_LIMITS)
        .expect("performance smoke artifact should run with JSONL trace");
    assert_eq!(report.spawned_processes.len(), 3);
    assert_eq!(report.delivered_messages.len(), 3);
    assert_eq!(report.emitted_outputs.len(), 2);
    report
}

fn run_local_supervision_artifact(
    artifact: &mantle_artifact::MantleArtifact,
) -> mantle_runtime::RuntimeReport {
    let mut host = InMemoryRuntimeHost::default();
    let report = run_artifact_with_host(artifact, &mut host, PERF_RUN_LIMITS)
        .expect("local supervision performance smoke artifact should run");
    assert!(!report.spawned_processes.is_empty());
    assert!(!report.processes.is_empty());
    report
}

fn assert_within_budget(budget: PerformanceBudget, metrics: ResourceMetrics) {
    let iterations =
        u32::try_from(budget.iterations).expect("performance smoke iteration count should fit u32");

    assert!(
        metrics.wall <= budget.wall_budget,
        "{} exceeded wall-time performance smoke budget: {:?} for {} iterations, budget {:?}",
        budget.profile.label,
        metrics.wall,
        budget.iterations,
        budget.wall_budget,
    );

    if let Some(cpu) = metrics.cpu {
        assert!(
            cpu <= budget.cpu_budget,
            "{} exceeded CPU performance smoke budget: {:?} for {} iterations, budget {:?}",
            budget.profile.label,
            cpu,
            budget.iterations,
            budget.cpu_budget,
        );
    }

    if let Some(current_rss_kib) = metrics.current_rss_kib {
        assert!(
            current_rss_kib <= budget.rss_budget_kib,
            "{} exceeded RSS performance smoke budget: {} KiB, budget {} KiB",
            budget.profile.label,
            current_rss_kib,
            budget.rss_budget_kib,
        );
    }

    assert_allocation_budget(
        budget.profile.label,
        "allocation count",
        metrics.allocations.allocation_count,
        budget.allocation_count_budget,
    );
    assert_allocation_budget(
        budget.profile.label,
        "deallocation count",
        metrics.allocations.deallocation_count,
        budget.deallocation_count_budget,
    );
    assert_allocation_budget(
        budget.profile.label,
        "allocated bytes",
        metrics.allocations.allocated_bytes,
        budget.allocated_bytes_budget,
    );
    assert_allocation_budget(
        budget.profile.label,
        "deallocated bytes",
        metrics.allocations.deallocated_bytes,
        budget.deallocated_bytes_budget,
    );
    assert_signed_allocation_budget(
        budget.profile.label,
        "net live-byte delta",
        metrics.allocations.net_live_bytes_delta,
        budget.net_live_bytes_delta_budget,
    );
    assert_allocation_budget(
        budget.profile.label,
        "peak live bytes over interval start",
        metrics.allocations.peak_live_bytes_over_start,
        budget.peak_live_bytes_over_start_budget,
    );

    println!(
        "{}: wall {:?} total for {} iterations ({:?} per iteration), cpu {}, rss {}, allocations {}; reviewed baseline wall {:?}, cpu {:?}, rss {} KiB, allocations {}",
        budget.profile.label,
        metrics.wall,
        budget.iterations,
        metrics.wall / iterations,
        format_optional_duration(metrics.cpu),
        format_memory(metrics),
        format_allocations(metrics.allocations),
        budget.reference.wall,
        budget.reference.cpu,
        budget.reference.rss_kib,
        format_reference_allocations(budget.reference),
    );
    println!(
        "PERFORMANCE_SMOKE_METRICS profile={} iterations={} wall_nanos={} cpu_nanos={} current_rss_kib={} peak_rss_kib={} allocation_count={} deallocation_count={} allocated_bytes={} deallocated_bytes={} net_live_bytes_delta={} peak_live_bytes_over_start={}",
        budget.profile.key,
        budget.iterations,
        metrics.wall.as_nanos(),
        format_optional_duration_nanos(metrics.cpu),
        format_optional_u64(metrics.current_rss_kib),
        format_optional_u64(metrics.peak_rss_kib),
        metrics.allocations.allocation_count,
        metrics.allocations.deallocation_count,
        metrics.allocations.allocated_bytes,
        metrics.allocations.deallocated_bytes,
        metrics.allocations.net_live_bytes_delta,
        metrics.allocations.peak_live_bytes_over_start,
    );
}

fn assert_allocation_budget(profile: &str, metric: &str, value: u64, budget: u64) {
    assert!(
        value <= budget,
        "{profile} exceeded {metric} performance smoke budget: {value}, budget {budget}"
    );
}

fn assert_signed_allocation_budget(profile: &str, metric: &str, value: i64, budget: u64) {
    assert!(
        i128::from(value) <= i128::from(budget),
        "{profile} exceeded {metric} performance smoke budget: {value}, budget {budget}"
    );
}

fn baseline_duration(key: &str) -> Duration {
    Duration::from_nanos(baseline_u64(key))
}

fn baseline_usize(key: &str) -> usize {
    let value = baseline_u64(key);
    let value = usize::try_from(value).expect("performance baseline value should fit usize");
    assert!(value > 0, "{key} must be greater than zero");
    value
}

fn baseline_u64(key: &str) -> u64 {
    let value = baseline_value(key);
    value
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{key} must be an unsigned base-10 integer, got {value:?}"))
}

fn baseline_i64(key: &str) -> i64 {
    let value = baseline_value(key);
    value
        .parse::<i64>()
        .unwrap_or_else(|_| panic!("{key} must be a signed base-10 integer, got {value:?}"))
}

fn baseline_value(key: &str) -> &str {
    let mut found = None;
    for (line_index, raw_line) in PERFORMANCE_BASELINE.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (candidate_key, value) = line.split_once('=').unwrap_or_else(|| {
            panic!(
                "performance baseline line {} must use key=value syntax",
                line_index + 1
            )
        });
        let candidate_key = candidate_key.trim();
        let value = value.trim();
        assert!(
            !candidate_key.is_empty(),
            "performance baseline line {} has empty key",
            line_index + 1
        );
        assert!(
            !value.is_empty(),
            "performance baseline line {} has empty value",
            line_index + 1
        );
        if candidate_key == key {
            assert!(
                found.replace(value).is_none(),
                "performance baseline key {key} is duplicated"
            );
        }
    }
    found.unwrap_or_else(|| panic!("performance baseline key {key} is missing"))
}

fn format_optional_duration(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "unavailable".to_owned(),
        |duration| format!("{duration:?}"),
    )
}

fn format_optional_duration_nanos(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "unavailable".to_owned(),
        |duration| duration.as_nanos().to_string(),
    )
}

fn format_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
}

fn format_memory(metrics: ResourceMetrics) -> String {
    match (metrics.current_rss_kib, metrics.peak_rss_kib) {
        (Some(current), Some(peak)) => format!("{current} KiB current, {peak} KiB peak"),
        (Some(current), None) => format!("{current} KiB current, peak unavailable"),
        (None, Some(peak)) => format!("current unavailable, {peak} KiB peak"),
        (None, None) => "unavailable".to_owned(),
    }
}

fn format_allocations(allocations: AllocationMetrics) -> String {
    format!(
        "{} allocs, {} deallocs, {} B allocated, {} B deallocated, {} B net live delta, {} B peak live over interval start",
        allocations.allocation_count,
        allocations.deallocation_count,
        allocations.allocated_bytes,
        allocations.deallocated_bytes,
        allocations.net_live_bytes_delta,
        allocations.peak_live_bytes_over_start,
    )
}

fn format_reference_allocations(reference: ReferenceMetrics) -> String {
    format!(
        "{} allocs, {} deallocs, {} B allocated, {} B deallocated, {} B net live delta, {} B peak live over interval start",
        reference.allocation_count,
        reference.deallocation_count,
        reference.allocated_bytes,
        reference.deallocated_bytes,
        reference.net_live_bytes_delta,
        reference.peak_live_bytes_over_start,
    )
}
