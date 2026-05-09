use mantle_artifact::{Error, Result};

use crate::event::RuntimeEventRecord;

pub(super) fn checked_trace_event_bytes(
    current: usize,
    event: &RuntimeEventRecord,
) -> Result<usize> {
    let event_line_bytes = event.jsonl_line_bytes_with_newline()?;
    current
        .checked_add(event_line_bytes)
        .ok_or_else(|| Error::new("runtime trace size overflowed"))
}

pub(super) fn checked_output_bytes(current: usize, next_output_len: usize) -> Result<usize> {
    let next_output_with_newline = next_output_len
        .checked_add(1)
        .ok_or_else(|| Error::new("emitted output size overflowed"))?;
    current
        .checked_add(next_output_with_newline)
        .ok_or_else(|| Error::new("emitted output size overflowed"))
}
