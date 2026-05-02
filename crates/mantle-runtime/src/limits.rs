use mantle_artifact::{Error, Result};

pub const DEFAULT_MAX_DISPATCHES: usize = 10_000;
pub const DEFAULT_MAX_TRACE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_EMITTED_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub max_dispatches: usize,
    pub max_trace_bytes: usize,
    pub max_emitted_output_bytes: usize,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_dispatches: DEFAULT_MAX_DISPATCHES,
            max_trace_bytes: DEFAULT_MAX_TRACE_BYTES,
            max_emitted_output_bytes: DEFAULT_MAX_EMITTED_OUTPUT_BYTES,
        }
    }
}

impl RunLimits {
    pub(crate) fn validate(self) -> Result<()> {
        if self.max_dispatches == 0 {
            return Err(Error::new("max_dispatches must be greater than zero"));
        }
        if self.max_trace_bytes == 0 {
            return Err(Error::new("max_trace_bytes must be greater than zero"));
        }
        if self.max_emitted_output_bytes == 0 {
            return Err(Error::new(
                "max_emitted_output_bytes must be greater than zero",
            ));
        }
        Ok(())
    }
}
