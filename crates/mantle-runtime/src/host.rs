use std::fs::{self, File, OpenOptions};
use std::io::{LineWriter, Write};
use std::path::Path;

use mantle_artifact::{Error, Result};

use crate::{RuntimeEvent, RuntimeEventRecord};

pub trait RuntimeHost {
    fn record_event(&mut self, event: &RuntimeEventRecord) -> Result<()>;
    fn emit_stdout(&mut self, text: &str) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryRuntimeHost {
    events: Vec<RuntimeEvent>,
    stdout: Vec<String>,
}

impl InMemoryRuntimeHost {
    pub fn events(&self) -> &[RuntimeEvent] {
        &self.events
    }

    pub fn stdout(&self) -> &[String] {
        &self.stdout
    }
}

impl RuntimeHost for InMemoryRuntimeHost {
    fn record_event(&mut self, event: &RuntimeEventRecord) -> Result<()> {
        self.events.push(event.event().clone());
        Ok(())
    }

    fn emit_stdout(&mut self, text: &str) -> Result<()> {
        self.stdout.push(text.to_string());
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct JsonlTraceHost {
    file: LineWriter<File>,
    bytes_written: usize,
    max_bytes: usize,
}

impl JsonlTraceHost {
    pub(crate) fn new(file: File, max_bytes: usize) -> Self {
        Self {
            file: LineWriter::new(file),
            bytes_written: 0,
            max_bytes,
        }
    }

    fn push(&mut self, line: &str) -> Result<()> {
        let line_bytes = line
            .len()
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime trace line size overflowed"))?;
        let next_bytes = self
            .bytes_written
            .checked_add(line_bytes)
            .ok_or_else(|| Error::new("runtime trace size overflowed"))?;
        if next_bytes > self.max_bytes {
            return Err(Error::new(format!(
                "runtime trace exceeded maximum size of {} bytes",
                self.max_bytes
            )));
        }

        let mut trace_line = String::with_capacity(line_bytes);
        trace_line.push_str(line);
        trace_line.push('\n');
        self.file.write_all(trace_line.as_bytes())?;
        self.bytes_written = next_bytes;
        Ok(())
    }
}

impl RuntimeHost for JsonlTraceHost {
    fn record_event(&mut self, event: &RuntimeEventRecord) -> Result<()> {
        self.push(event.jsonl_line())
    }

    fn emit_stdout(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }
}

pub(crate) fn prepare_trace_file(path: &Path) -> Result<File> {
    reject_non_regular_trace_path_before_open(path)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let file = open_trace_file(path)?;
    validate_trace_file_metadata(path, &file.metadata()?)?;
    Ok(file)
}

fn reject_non_regular_trace_path_before_open(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => validate_trace_file_metadata(path, &metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
fn open_trace_file(path: &Path) -> std::io::Result<File> {
    use nix::fcntl::OFlag;
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(path)
}

#[cfg(not(unix))]
fn open_trace_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
}

fn validate_trace_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.is_file() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "runtime trace path {} is not a regular file",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(unix)]
    #[test]
    fn prepare_trace_file_rejects_fifo_trace_path_before_opening() {
        let path = unique_trace_path("fifo");
        create_fifo(&path);

        let err = prepare_trace_file(&path).expect_err("FIFO trace path should fail closed");

        assert!(err.to_string().contains("runtime trace path"));
        assert!(err.to_string().contains("is not a regular file"));
        fs::remove_file(path).expect("test FIFO should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn opened_non_regular_trace_handle_is_rejected() {
        let path = unique_trace_path("opened-fifo");
        create_fifo(&path);
        let file =
            open_trace_read_handle_for_test(&path).expect("FIFO input open should not block");

        let err = validate_trace_file_metadata(&path, &file.metadata().expect("metadata"))
            .expect_err("opened FIFO handle should fail regular-file validation");

        assert!(err.to_string().contains("is not a regular file"));
        fs::remove_file(path).expect("test FIFO should be removed");
    }

    #[cfg(unix)]
    fn open_trace_read_handle_for_test(path: &Path) -> std::io::Result<File> {
        use nix::fcntl::OFlag;
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(OFlag::O_NONBLOCK.bits())
            .open(path)
    }

    #[cfg(unix)]
    fn create_fifo(path: &Path) {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;

        mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test FIFO should be created");
    }

    #[cfg(unix)]
    fn unique_trace_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "strata-runtime-trace-{name}-{}-{nanos}.observability.jsonl",
            std::process::id()
        ))
    }
}
