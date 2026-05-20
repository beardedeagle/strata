use std::time::Duration;

use super::MemorySnapshot;

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
))]
use std::process::Command;

#[cfg(target_os = "linux")]
pub(super) fn capture_cpu_time() -> Option<Duration> {
    capture_linux_stat_cpu_time().or_else(capture_linux_schedstat_cpu_time)
}

#[cfg(target_os = "linux")]
pub(super) fn capture_memory() -> Option<MemorySnapshot> {
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
pub(super) fn capture_cpu_time() -> Option<Duration> {
    let pid = std::process::id().to_string();
    parse_ps_duration(&command_stdout("ps", &["-o", "cputime=", "-p", &pid])?)
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
pub(super) fn capture_memory() -> Option<MemorySnapshot> {
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
pub(super) fn capture_cpu_time() -> Option<Duration> {
    None
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
pub(super) fn capture_memory() -> Option<MemorySnapshot> {
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
