use std::time::Duration;

use super::{AllocationMetrics, ReferenceMetrics, ResourceMetrics};

pub(super) fn format_optional_duration(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "unavailable".to_owned(),
        |duration| format!("{duration:?}"),
    )
}

pub(super) fn format_optional_duration_nanos(duration: Option<Duration>) -> String {
    duration.map_or_else(
        || "unavailable".to_owned(),
        |duration| duration.as_nanos().to_string(),
    )
}

pub(super) fn format_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
}

pub(super) fn format_memory(metrics: ResourceMetrics) -> String {
    match (metrics.current_rss_kib, metrics.peak_rss_kib) {
        (Some(current), Some(peak)) => format!("{current} KiB current, {peak} KiB peak"),
        (Some(current), None) => format!("{current} KiB current, peak unavailable"),
        (None, Some(peak)) => format!("current unavailable, {peak} KiB peak"),
        (None, None) => "unavailable".to_owned(),
    }
}

pub(super) fn format_allocations(allocations: AllocationMetrics) -> String {
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

pub(super) fn format_reference_allocations(reference: ReferenceMetrics) -> String {
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
