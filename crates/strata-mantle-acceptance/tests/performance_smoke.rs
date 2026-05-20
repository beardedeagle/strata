#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use mantle_runtime::{InMemoryRuntimeHost, RunLimits, run_artifact_with_host};

const PERFORMANCE_BASELINE: &str = include_str!("../../../benchmarks/performance-smoke.baseline");
const COLLECTION_STATE_SOURCE: &str = include_str!("../../../examples/collection_state.str");
const CHECK_LOWER_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.check_lower",
    label: "collection_state check+lower",
};
const IN_MEMORY_RUNTIME_PROFILE: BenchmarkProfile = BenchmarkProfile {
    key: "collection_state.in_memory_runtime",
    label: "collection_state in-memory runtime",
};
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
};

#[test]
#[ignore = "run through `just performance-smoke` so timing checks stay explicit"]
fn collection_state_compilation_and_runtime_performance_smoke() {
    let check_lower_budget = PerformanceBudget::load(CHECK_LOWER_PROFILE);
    let in_memory_runtime_budget = PerformanceBudget::load(IN_MEMORY_RUNTIME_PROFILE);

    let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should check");
    let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should lower");
    run_collection_state_artifact(&artifact);

    let compilation_metrics = measure_for(check_lower_budget.iterations, || {
        let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should check");
        let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should lower");
        black_box(artifact);
    });

    let runtime_metrics = measure_for(in_memory_runtime_budget.iterations, || {
        let report = run_collection_state_artifact(&artifact);
        black_box(report);
    });

    assert_within_budget(check_lower_budget, compilation_metrics);
    assert_within_budget(in_memory_runtime_budget, runtime_metrics);
}

#[derive(Clone, Copy, Debug)]
struct BenchmarkProfile {
    key: &'static str,
    label: &'static str,
}

#[allow(unsafe_code)]
#[path = "performance_smoke/allocation_meter.rs"]
mod allocation_meter;
#[path = "performance_smoke/platform_resources.rs"]
mod platform_resources;

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
