#![deny(unsafe_op_in_unsafe_fn)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
))]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mantle_runtime::{InMemoryRuntimeHost, RunLimits, run_artifact_with_host};

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOCATION_COUNTERS: AllocationCounters = AllocationCounters::new();

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

struct CountingAllocator;

struct AllocationCounters {
    allocations: AtomicU64,
    deallocations: AtomicU64,
    allocated_bytes: AtomicU64,
    deallocated_bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
}

impl AllocationCounters {
    const fn new() -> Self {
        Self {
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            deallocated_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn record_allocation(&self, size: usize) {
        let size = allocation_size(size);
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
        let live = self
            .live_bytes
            .fetch_add(size, Ordering::Relaxed)
            .saturating_add(size);
        self.record_peak_live(live);
    }

    fn record_deallocation(&self, size: usize) {
        let size = allocation_size(size);
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.deallocated_bytes.fetch_add(size, Ordering::Relaxed);
        self.live_bytes.fetch_sub(size, Ordering::Relaxed);
    }

    fn record_peak_live(&self, live: u64) {
        let mut peak = self.peak_live_bytes.load(Ordering::Relaxed);
        while live > peak {
            match self.peak_live_bytes.compare_exchange_weak(
                peak,
                live,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next_peak) => peak = next_peak,
            }
        }
    }

    fn capture(&self) -> AllocationSnapshot {
        AllocationSnapshot {
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
            peak_live_bytes: self.peak_live_bytes.load(Ordering::Relaxed),
        }
    }

    fn capture_interval_start(&self) -> AllocationSnapshot {
        let live = self.live_bytes.load(Ordering::Relaxed);
        self.peak_live_bytes.store(live, Ordering::Relaxed);
        self.capture()
    }
}

// SAFETY: This test-only allocator delegates all allocation behavior to
// `System` and only records atomic counters after successful allocation calls.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forwarding the exact layout to the platform allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATION_COUNTERS.record_allocation(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Forwarding the original pointer and layout to the allocator.
        unsafe { System.dealloc(ptr, layout) };
        ALLOCATION_COUNTERS.record_deallocation(layout.size());
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Forwarding the exact layout to the platform allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATION_COUNTERS.record_allocation(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Forwarding the original pointer/layout and requested size.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCATION_COUNTERS.record_deallocation(layout.size());
            ALLOCATION_COUNTERS.record_allocation(new_size);
        }
        new_ptr
    }
}

fn allocation_size(size: usize) -> u64 {
    u64::try_from(size).unwrap_or(u64::MAX)
}

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
    current_live_bytes_budget: u64,
    peak_live_bytes_budget: u64,
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
    current_live_bytes: u64,
    peak_live_bytes: u64,
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
                current_live_bytes: baseline_u64(&format!(
                    "{}.reference_current_live_bytes",
                    profile.key
                )),
                peak_live_bytes: baseline_u64(&format!(
                    "{}.reference_peak_live_bytes",
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
            current_live_bytes_budget: baseline_u64(&format!(
                "{}.current_live_bytes_budget",
                profile.key
            )),
            peak_live_bytes_budget: baseline_u64(&format!(
                "{}.peak_live_bytes_budget",
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
        let allocations = ALLOCATION_COUNTERS.capture_interval_start();
        Self::capture_with_allocations(allocations)
    }

    fn capture() -> Self {
        Self::capture_with_allocations(ALLOCATION_COUNTERS.capture())
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
    current_live_bytes: u64,
    peak_live_bytes: u64,
}

impl AllocationSnapshot {
    fn measure_until(self, end: Self) -> AllocationMetrics {
        AllocationMetrics {
            allocation_count: end.allocations.saturating_sub(self.allocations),
            deallocation_count: end.deallocations.saturating_sub(self.deallocations),
            allocated_bytes: end.allocated_bytes.saturating_sub(self.allocated_bytes),
            deallocated_bytes: end.deallocated_bytes.saturating_sub(self.deallocated_bytes),
            current_live_bytes: end.live_bytes.saturating_sub(self.live_bytes),
            peak_live_bytes: end.peak_live_bytes.saturating_sub(self.live_bytes),
        }
    }
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
    assert_allocation_budget(
        budget.profile.label,
        "current live bytes",
        metrics.allocations.current_live_bytes,
        budget.current_live_bytes_budget,
    );
    assert_allocation_budget(
        budget.profile.label,
        "peak live bytes",
        metrics.allocations.peak_live_bytes,
        budget.peak_live_bytes_budget,
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
        "{} allocs, {} deallocs, {} B allocated, {} B deallocated, {} B current live, {} B peak live",
        allocations.allocation_count,
        allocations.deallocation_count,
        allocations.allocated_bytes,
        allocations.deallocated_bytes,
        allocations.current_live_bytes,
        allocations.peak_live_bytes,
    )
}

fn format_reference_allocations(reference: ReferenceMetrics) -> String {
    format!(
        "{} allocs, {} deallocs, {} B allocated, {} B deallocated, {} B current live, {} B peak live",
        reference.allocation_count,
        reference.deallocation_count,
        reference.allocated_bytes,
        reference.deallocated_bytes,
        reference.current_live_bytes,
        reference.peak_live_bytes,
    )
}

#[cfg(target_os = "linux")]
fn capture_cpu_time() -> Option<Duration> {
    capture_linux_stat_cpu_time().or_else(capture_linux_schedstat_cpu_time)
}

#[cfg(target_os = "linux")]
fn capture_memory() -> Option<MemorySnapshot> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    Some(MemorySnapshot {
        current_rss_kib: parse_status_kib(&status, "VmRSS:")?,
        peak_rss_kib: Some(parse_status_kib(&status, "VmHWM:")?),
    })
}

#[cfg(target_os = "linux")]
fn capture_linux_schedstat_cpu_time() -> Option<Duration> {
    let schedstat = std::fs::read_to_string("/proc/self/schedstat").ok()?;
    let cpu_nanoseconds = schedstat.split_whitespace().next()?.parse::<u64>().ok()?;
    Some(Duration::from_nanos(cpu_nanoseconds))
}

#[cfg(target_os = "linux")]
fn capture_linux_stat_cpu_time() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields = stat
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    let ticks_per_second = command_stdout("getconf", &["CLK_TCK"])?
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs_f64(
        (user_ticks + system_ticks) as f64 / ticks_per_second as f64,
    ))
}

#[cfg(target_os = "linux")]
fn parse_status_kib(status: &str, field: &str) -> Option<u64> {
    let value = status.lines().find_map(|line| line.strip_prefix(field))?;
    let mut parts = value.split_whitespace();
    let amount = parts.next()?.parse::<u64>().ok()?;
    match parts.next() {
        Some("kB") => Some(amount),
        _ => None,
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
fn capture_cpu_time() -> Option<Duration> {
    let pid = std::process::id().to_string();
    parse_ps_duration(&command_stdout("ps", &["-o", "cputime=", "-p", &pid])?)
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
fn capture_memory() -> Option<MemorySnapshot> {
    let pid = std::process::id().to_string();
    Some(MemorySnapshot {
        current_rss_kib: command_stdout("ps", &["-o", "rss=", "-p", &pid])?
            .parse()
            .ok()?,
        peak_rss_kib: None,
    })
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
fn parse_ps_duration(value: &str) -> Option<Duration> {
    let (days, time) = match value.split_once('-') {
        Some((days, time)) => (days.parse::<u64>().ok()?, time),
        None => (0, value),
    };
    let parts = time.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0, minutes.parse::<u64>().ok()?, parse_seconds(seconds)?),
        [hours, minutes, seconds] => (
            hours.parse::<u64>().ok()?,
            minutes.parse::<u64>().ok()?,
            parse_seconds(seconds)?,
        ),
        _ => return None,
    };
    Some(Duration::from_secs(days * 86_400 + hours * 3_600 + minutes * 60) + seconds)
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
fn parse_seconds(value: &str) -> Option<Duration> {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = seconds.parse::<u64>().ok()?;
    let nanoseconds = fraction
        .chars()
        .take(9)
        .try_fold((0_u32, 100_000_000_u32), |(total, scale), digit| {
            let digit = digit.to_digit(10)?;
            Some((total + digit * scale, scale / 10))
        })?
        .0;
    Some(Duration::new(seconds, nanoseconds))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
fn capture_cpu_time() -> Option<Duration> {
    None
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
fn capture_memory() -> Option<MemorySnapshot> {
    None
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
))]
fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
