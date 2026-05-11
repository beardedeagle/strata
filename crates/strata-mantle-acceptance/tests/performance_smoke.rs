#![forbid(unsafe_code)]

use std::hint::black_box;
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
))]
use std::process::Command;
use std::time::{Duration, Instant};

use mantle_runtime::{InMemoryRuntimeHost, RunLimits, run_artifact_with_host};

const COLLECTION_STATE_SOURCE: &str = include_str!("../../../examples/collection_state.str");
const COMPILATION_ITERATIONS: usize = 64;
const RUNTIME_ITERATIONS: usize = 64;
const COMPILATION_BUDGET: Duration = Duration::from_secs(5);
const RUNTIME_BUDGET: Duration = Duration::from_secs(5);
const COMPILATION_CPU_BUDGET: Duration = Duration::from_secs(5);
const RUNTIME_CPU_BUDGET: Duration = Duration::from_secs(5);
const RSS_BUDGET_KIB: u64 = 512 * 1024;
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
    let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should check");
    let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
        .expect("performance smoke source should lower");
    run_collection_state_artifact(&artifact);

    let compilation_metrics = measure_for(COMPILATION_ITERATIONS, || {
        let checked = strata::language::check_source(COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should check");
        let artifact = strata::language::lower_to_artifact(&checked, COLLECTION_STATE_SOURCE)
            .expect("performance smoke source should lower");
        black_box(artifact);
    });

    let runtime_metrics = measure_for(RUNTIME_ITERATIONS, || {
        let report = run_collection_state_artifact(&artifact);
        black_box(report);
    });

    assert_within_budget(
        "collection_state check+lower",
        compilation_metrics,
        COMPILATION_ITERATIONS,
        COMPILATION_BUDGET,
        COMPILATION_CPU_BUDGET,
    );
    assert_within_budget(
        "collection_state in-memory runtime",
        runtime_metrics,
        RUNTIME_ITERATIONS,
        RUNTIME_BUDGET,
        RUNTIME_CPU_BUDGET,
    );
}

#[derive(Clone, Copy, Debug)]
struct ResourceSnapshot {
    cpu: Option<Duration>,
    memory: Option<MemorySnapshot>,
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
}

fn measure_for(iterations: usize, mut operation: impl FnMut()) -> ResourceMetrics {
    let start = ResourceSnapshot::capture();
    let wall_start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let wall = wall_start.elapsed();
    let end = ResourceSnapshot::capture();
    start.measure_until(end, wall)
}

impl ResourceSnapshot {
    fn capture() -> Self {
        let cpu = capture_cpu_time();
        let memory = capture_memory();
        if RESOURCE_METRICS_REQUIRED {
            assert!(cpu.is_some(), "performance smoke CPU metrics unavailable");
            assert!(
                memory.is_some(),
                "performance smoke RSS metrics unavailable"
            );
        }
        Self { cpu, memory }
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

fn assert_within_budget(
    label: &str,
    metrics: ResourceMetrics,
    iterations: usize,
    wall_budget: Duration,
    cpu_budget: Duration,
) {
    assert!(
        metrics.wall <= wall_budget,
        "{label} exceeded wall-time performance smoke budget: {:?} for {iterations} iterations, budget {wall_budget:?}",
        metrics.wall,
    );

    if let Some(cpu) = metrics.cpu {
        assert!(
            cpu <= cpu_budget,
            "{label} exceeded CPU performance smoke budget: {cpu:?} for {iterations} iterations, budget {cpu_budget:?}"
        );
    }

    if let Some(peak_rss_kib) = metrics.peak_rss_kib {
        assert!(
            peak_rss_kib <= RSS_BUDGET_KIB,
            "{label} exceeded peak RSS performance smoke budget: {peak_rss_kib} KiB, budget {RSS_BUDGET_KIB} KiB"
        );
    } else if let Some(current_rss_kib) = metrics.current_rss_kib {
        assert!(
            current_rss_kib <= RSS_BUDGET_KIB,
            "{label} exceeded RSS performance smoke budget: {current_rss_kib} KiB, budget {RSS_BUDGET_KIB} KiB"
        );
    }

    println!(
        "{label}: wall {:?} total for {iterations} iterations ({:?} per iteration), cpu {}, rss {}",
        metrics.wall,
        metrics.wall / iterations as u32,
        format_optional_duration(metrics.cpu),
        format_memory(metrics),
    );
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

#[cfg(target_os = "linux")]
fn capture_cpu_time() -> Option<Duration> {
    capture_linux_schedstat_cpu_time().or_else(capture_linux_stat_cpu_time)
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
