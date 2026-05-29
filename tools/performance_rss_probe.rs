#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use mantle_runtime::{
    run_artifact_with_host, InMemoryRuntimeHost, RunLimits, SpawnAuthorityPolicy,
};

const PROFILE_ENV: &str = "STRATA_RSS_PROBE_PROFILE";
const COLLECTION_STATE_SOURCE: &str = include_str!("../../../examples/collection_state.str");
const LOCAL_SUPERVISION_SOURCE_PATH: &str = "../../examples/local_supervision_restart.str";
const IMPORTS_MAIN_SOURCE_PATH: &str = "../../examples/imports_main.str";
const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.check_lower",
    iterations: 64,
};
const IN_MEMORY_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.in_memory_runtime",
    iterations: 64,
};
const LOCAL_SUPERVISION_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "local_supervision_restart.in_memory_runtime",
    iterations: 32,
};
const IMPORTS_CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "imports_main.check_lower",
    iterations: 32,
};
const ALL_PROFILES: [BenchmarkProfile; 4] = [
    CHECK_LOWER_PROFILE,
    IN_MEMORY_RUNTIME_PROFILE,
    LOCAL_SUPERVISION_RUNTIME_PROFILE,
    IMPORTS_CHECK_LOWER_PROFILE,
];
const PROFILE_KEY_LIST: &str = "collection_state.check_lower, collection_state.in_memory_runtime, local_supervision_restart.in_memory_runtime, imports_main.check_lower";
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
};

#[derive(Clone, Copy, Debug)]
struct BenchmarkProfile {
    key: &'static str,
    iterations: usize,
}

// This file is copied into `crates/strata-mantle-acceptance/tests/` before it
// is compiled, so these paths are relative to that destination.
#[allow(unsafe_code)]
#[path = "performance_smoke/allocation_meter.rs"]
mod allocation_meter;
#[path = "performance_smoke/platform_resources.rs"]
mod platform_resources;

use platform_resources::{capture_cpu_time, capture_memory};

#[test]
#[ignore = "run through `just performance-rss-compare` for fresh-process RSS samples"]
fn rss_probe_runs_selected_profile() {
    let profile = selected_profile();
    match profile.key {
        "collection_state.check_lower" => measure_check_lower_profile(profile),
        "collection_state.in_memory_runtime" => measure_collection_state_runtime_profile(profile),
        "local_supervision_restart.in_memory_runtime" => {
            measure_local_supervision_runtime_profile(profile);
        }
        "imports_main.check_lower" => measure_imports_check_lower_profile(profile),
        _ => unreachable!("selected profile is validated before dispatch"),
    }
}

fn selected_profile() -> BenchmarkProfile {
    let selected = std::env::var(PROFILE_ENV)
        .unwrap_or_else(|_| panic!("{PROFILE_ENV} must be set to one of: {PROFILE_KEY_LIST}"));
    ALL_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.key == selected)
        .unwrap_or_else(|| panic!("{PROFILE_ENV} must be one of: {PROFILE_KEY_LIST}"))
}

fn measure_check_lower_profile(profile: BenchmarkProfile) {
    let metrics = measure_for(profile.iterations, || {
        let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
            .expect("RSS probe source should check");
        let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
            .expect("RSS probe source should lower");
        black_box(artifact);
    });
    print_metrics(profile, metrics);
}

fn measure_collection_state_runtime_profile(profile: BenchmarkProfile) {
    let artifact = collection_state_artifact();
    run_collection_state_artifact(&artifact);
    let metrics = measure_for(profile.iterations, || {
        let report = run_collection_state_artifact(&artifact);
        black_box(report);
    });
    print_metrics(profile, metrics);
}

fn measure_local_supervision_runtime_profile(profile: BenchmarkProfile) {
    let source = read_workspace_source(LOCAL_SUPERVISION_SOURCE_PATH);
    let artifact = source_artifact(&source);
    run_local_supervision_artifact(&artifact);
    let metrics = measure_for(profile.iterations, || {
        let report = run_local_supervision_artifact(&artifact);
        black_box(report);
    });
    print_metrics(profile, metrics);
}

fn measure_imports_check_lower_profile(profile: BenchmarkProfile) {
    let source_path = workspace_source_path(IMPORTS_MAIN_SOURCE_PATH);
    let metrics = measure_for(profile.iterations, || {
        let loaded = strata::load_root_source_program(&source_path)
            .expect("RSS probe imports source should load");
        let (program, source_hash) = loaded.into_parts();
        let checked = strata::language::check_source_program(program)
            .expect("RSS probe imports source should check");
        let artifact = strata::language::lower_to_artifact_with_source_hash(&checked, source_hash)
            .expect("RSS probe imports source should lower");
        black_box(artifact);
    });
    print_metrics(profile, metrics);
}

fn read_workspace_source(path: &str) -> String {
    fs::read_to_string(workspace_source_path(path))
        .unwrap_or_else(|err| panic!("RSS probe source {path} should be readable: {err}"))
}

fn workspace_source_path(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn collection_state_artifact() -> mantle_artifact::MantleArtifact {
    source_artifact(COLLECTION_STATE_SOURCE)
}

fn source_artifact(source: &str) -> mantle_artifact::MantleArtifact {
    let checked = strata::language::check_source(source).expect("RSS probe source should check");
    strata::language::lower_to_artifact(&checked, source).expect("RSS probe source should lower")
}

fn run_collection_state_artifact(
    artifact: &mantle_artifact::MantleArtifact,
) -> mantle_runtime::RuntimeReport {
    let mut host = InMemoryRuntimeHost::default();
    let report = run_artifact_with_host(artifact, &mut host, PERF_RUN_LIMITS)
        .expect("RSS probe artifact should run");
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
        .expect("RSS probe local supervision artifact should run");
    assert!(!report.spawned_processes.is_empty());
    assert!(!report.processes.is_empty());
    report
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
            assert!(cpu.is_some(), "RSS probe CPU metrics unavailable");
            assert!(memory.is_some(), "RSS probe memory metrics unavailable");
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

fn print_metrics(profile: BenchmarkProfile, metrics: ResourceMetrics) {
    println!(
        "RSS_PROBE_METRICS profile={} iterations={} wall_nanos={} cpu_nanos={} current_rss_kib={} peak_rss_kib={} allocation_count={} deallocation_count={} allocated_bytes={} deallocated_bytes={} net_live_bytes_delta={} peak_live_bytes_over_start={}",
        profile.key,
        profile.iterations,
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

fn format_optional_duration_nanos(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "unavailable".to_owned(),
        |duration| duration.as_nanos().to_string(),
    )
}

fn format_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
}
