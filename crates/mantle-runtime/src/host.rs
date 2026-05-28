use std::fs::{self, File};
use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};

use mantle_artifact::{Error, Result};

use crate::{RuntimeEvent, RuntimeEventRecord};

pub trait RuntimeHost {
    fn record_event(&mut self, event: RuntimeEventRecord) -> Result<()>;
    fn emit_stdout(&mut self, text: &str) -> Result<()>;
    fn monotonic_ms(&mut self) -> Result<u64>;
    fn flush(&mut self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryRuntimeHost {
    events: Vec<RuntimeEvent>,
    stdout: Vec<String>,
    monotonic_ms: u64,
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
    fn record_event(&mut self, event: RuntimeEventRecord) -> Result<()> {
        self.events.push(event.into_event());
        Ok(())
    }

    fn emit_stdout(&mut self, text: &str) -> Result<()> {
        self.stdout.push(text.to_string());
        Ok(())
    }

    fn monotonic_ms(&mut self) -> Result<u64> {
        let now = self.monotonic_ms;
        self.monotonic_ms = self
            .monotonic_ms
            .checked_add(1)
            .ok_or_else(|| Error::new("runtime monotonic clock overflowed"))?;
        Ok(now)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct JsonlTraceHost {
    file: LineWriter<File>,
    bytes_written: usize,
    max_bytes: usize,
    started_at: std::time::Instant,
}

impl JsonlTraceHost {
    pub(crate) fn new(file: File, max_bytes: usize) -> Self {
        Self {
            file: LineWriter::new(file),
            bytes_written: 0,
            max_bytes,
            started_at: std::time::Instant::now(),
        }
    }
}

impl RuntimeHost for JsonlTraceHost {
    fn record_event(&mut self, event: RuntimeEventRecord) -> Result<()> {
        let line_bytes = event.jsonl_line_bytes_with_newline()?;
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

        event.write_jsonl_line(&mut self.file)?;
        self.file.write_all(b"\n")?;
        self.bytes_written = next_bytes;
        Ok(())
    }

    fn emit_stdout(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn monotonic_ms(&mut self) -> Result<u64> {
        u64::try_from(self.started_at.elapsed().as_millis())
            .map_err(|_| Error::new("runtime monotonic clock cannot fit into u64 milliseconds"))
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }
}

pub(crate) fn prepare_trace_file(path: &Path) -> Result<File> {
    reject_symlink_path_components(path)?;
    reject_non_regular_trace_path_before_open(path)?;
    prepare_trace_parent(path)?;
    let file = open_trace_file(path)?;
    validate_trace_file_metadata(path, &file.metadata()?)?;
    Ok(file)
}

fn reject_non_regular_trace_path_before_open(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_trace_file_metadata(path, &metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
fn prepare_trace_parent(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn prepare_trace_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
            reject_symlink_path_components(parent)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn open_trace_file(path: &Path) -> std::io::Result<File> {
    let file_name = secure_leaf_name(path)?;
    let parent = open_secure_parent_directory(path, true)?;
    let fd = nix::fcntl::openat(&parent, file_name, output_file_flags(), output_file_mode())
        .map_err(nix_to_io_error)?;
    Ok(File::from(fd))
}

#[cfg(all(not(unix), not(windows)))]
fn open_trace_file(path: &Path) -> std::io::Result<File> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
}

#[cfg(windows)]
fn open_trace_file(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(unix)]
fn secure_leaf_name(path: &Path) -> std::io::Result<&std::ffi::OsStr> {
    path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("runtime trace path {} has no file name", path.display()),
        )
    })
}

#[cfg(unix)]
fn open_secure_parent_directory(
    path: &Path,
    create_missing: bool,
) -> std::io::Result<std::os::fd::OwnedFd> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let start = if parent.is_absolute() {
        Path::new("/")
    } else {
        Path::new(".")
    };
    let mut directory = nix::fcntl::openat(
        nix::fcntl::AT_FDCWD,
        start,
        directory_flags(),
        nix::sys::stat::Mode::empty(),
    )
    .map_err(nix_to_io_error)?;

    for component in parent.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => {
                directory = open_secure_child_directory(&directory, name, create_missing)?;
            }
            std::path::Component::ParentDir => {
                directory =
                    open_secure_child_directory(&directory, std::ffi::OsStr::new(".."), false)?;
            }
            std::path::Component::Prefix(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "runtime trace path {} uses an unsupported prefix",
                        path.display()
                    ),
                ));
            }
        }
    }

    Ok(directory)
}

#[cfg(unix)]
fn open_secure_child_directory(
    parent: &std::os::fd::OwnedFd,
    name: &std::ffi::OsStr,
    create_missing: bool,
) -> std::io::Result<std::os::fd::OwnedFd> {
    match nix::fcntl::openat(
        parent,
        name,
        directory_flags(),
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(directory) => Ok(directory),
        Err(nix::errno::Errno::ENOENT) if create_missing => {
            match nix::sys::stat::mkdirat(parent, name, directory_mode()) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(err) => return Err(nix_to_io_error(err)),
            }
            nix::fcntl::openat(
                parent,
                name,
                directory_flags(),
                nix::sys::stat::Mode::empty(),
            )
            .map_err(nix_to_io_error)
        }
        Err(err) => Err(nix_to_io_error(err)),
    }
}

#[cfg(unix)]
fn directory_flags() -> nix::fcntl::OFlag {
    nix::fcntl::OFlag::O_RDONLY
        | nix::fcntl::OFlag::O_DIRECTORY
        | nix::fcntl::OFlag::O_NOFOLLOW
        | nix::fcntl::OFlag::O_CLOEXEC
}

#[cfg(unix)]
fn output_file_flags() -> nix::fcntl::OFlag {
    nix::fcntl::OFlag::O_CREAT
        | nix::fcntl::OFlag::O_TRUNC
        | nix::fcntl::OFlag::O_WRONLY
        | nix::fcntl::OFlag::O_NONBLOCK
        | nix::fcntl::OFlag::O_NOFOLLOW
        | nix::fcntl::OFlag::O_CLOEXEC
}

#[cfg(unix)]
fn directory_mode() -> nix::sys::stat::Mode {
    nix::sys::stat::Mode::from_bits_truncate(0o777)
}

#[cfg(unix)]
fn output_file_mode() -> nix::sys::stat::Mode {
    nix::sys::stat::Mode::from_bits_truncate(0o666)
}

#[cfg(unix)]
fn nix_to_io_error(err: nix::errno::Errno) -> std::io::Error {
    std::io::Error::from_raw_os_error(err as i32)
}

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

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

fn reject_symlink_path_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::new(format!(
                    "runtime trace path {} must not include symbolic link component {}",
                    path.display(),
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn prepare_trace_file_rejects_fifo_trace_path_before_opening() {
        let path = unique_trace_path("fifo");
        create_fifo(&path);

        let err = prepare_trace_file(&path).expect_err("FIFO trace path should fail closed");

        assert!(err.to_string().contains("runtime trace path"));
        assert!(err.to_string().contains("is not a regular file"));
        fs::remove_file(path).expect("test FIFO should be removed");
    }

    #[test]
    fn prepare_trace_file_rejects_symlink_trace_path_without_touching_target() {
        use std::os::unix::fs::symlink;

        let target = unique_trace_path("symlink-target");
        let link = unique_trace_path("symlink-link");
        fs::write(&target, "unchanged").expect("test symlink target should be written");
        symlink(&target, &link).expect("test symlink should be created");

        let err = prepare_trace_file(&link).expect_err("symlink trace path should fail");

        assert!(err.to_string().contains("symbolic link component"));
        assert_eq!(
            fs::read_to_string(&target).expect("target should be readable"),
            "unchanged"
        );

        fs::remove_file(link).expect("test symlink should be removed");
        fs::remove_file(target).expect("test target should be removed");
    }

    #[test]
    fn prepare_trace_file_rejects_symlink_parent_path() {
        use std::os::unix::fs::symlink;

        let real_dir = unique_trace_path("real-parent");
        let link_dir = unique_trace_path("link-parent");
        fs::create_dir_all(&real_dir).expect("test real dir should be created");
        symlink(&real_dir, &link_dir).expect("test parent symlink should be created");
        let trace_path = link_dir.join("trace.observability.jsonl");

        let err = prepare_trace_file(&trace_path).expect_err("symlink parent path should fail");

        assert!(err.to_string().contains("symbolic link component"));
        assert!(
            !real_dir.join("trace.observability.jsonl").exists(),
            "symlink parent must not receive trace output"
        );

        fs::remove_file(link_dir).expect("test parent symlink should be removed");
        fs::remove_dir(real_dir).expect("test real dir should be removed");
    }

    #[test]
    fn secure_trace_open_rejects_symlink_parent_without_preflight() {
        use std::os::unix::fs::symlink;

        let real_dir = unique_trace_path("real-parent-open");
        let link_dir = unique_trace_path("link-parent-open");
        fs::create_dir_all(&real_dir).expect("test real dir should be created");
        symlink(&real_dir, &link_dir).expect("test parent symlink should be created");
        let path = link_dir.join("trace.observability.jsonl");

        let err =
            open_trace_file(&path).expect_err("descriptor-relative trace open should fail closed");

        assert!(!err.to_string().is_empty());
        assert!(
            !real_dir.join("trace.observability.jsonl").exists(),
            "secure trace open must not create files through a symlink parent"
        );

        fs::remove_file(link_dir).expect("test parent symlink should be removed");
        fs::remove_dir(real_dir).expect("test real dir should be removed");
    }

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

    fn open_trace_read_handle_for_test(path: &Path) -> std::io::Result<File> {
        use nix::fcntl::OFlag;
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .read(true)
            .custom_flags(OFlag::O_NONBLOCK.bits())
            .open(path)
    }

    fn create_fifo(path: &Path) {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;

        mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test FIFO should be created");
    }

    fn unique_trace_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        let temp_dir = std::env::temp_dir()
            .canonicalize()
            .unwrap_or_else(|_| std::env::temp_dir());
        temp_dir.join(format!(
            "mantle-runtime-trace-{name}-{}-{nanos}.observability.jsonl",
            std::process::id()
        ))
    }
}
