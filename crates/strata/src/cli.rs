use std::env;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use mantle_artifact::{default_artifact_path, write_artifact};

use crate::language::{check_source, lower_to_artifact, MAX_SOURCE_BYTES};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Language(crate::language::Error),
    Artifact(mantle_artifact::Error),
    Io(std::io::Error),
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Language(err) => write!(f, "{err}"),
            Self::Artifact(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Message(_) => None,
            Self::Language(err) => Some(err),
            Self::Artifact(err) => Some(err),
            Self::Io(err) => Some(err),
        }
    }
}

impl From<crate::language::Error> for Error {
    fn from(value: crate::language::Error) -> Self {
        Self::Language(value)
    }
}

impl From<mantle_artifact::Error> for Error {
    fn from(value: mantle_artifact::Error) -> Self {
        Self::Artifact(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn strata_main<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    match args.next().as_deref() {
        Some("check") => {
            let path = required_path(args.next(), "strata check <path>")?;
            ensure_no_extra_args(args)?;
            let source = read_source_file(&path)?;
            let checked = check_source(&source)?;
            let _artifact = lower_to_artifact(&checked, &source)?;
            let entry = checked
                .processes
                .get(checked.entry_process.index())
                .map(|process| process.debug_name.as_str())
                .ok_or_else(|| Error::new("checked entry process is not defined"))?;
            println!(
                "strata: checked {} (module {}, entry {})",
                path.display(),
                checked.module.name,
                entry
            );
            Ok(())
        }
        Some("build") => {
            let path = required_path(args.next(), "strata build <path> [--output <path>]")?;
            let mut output = None;
            let mut rest = args.peekable();
            while let Some(arg) = rest.next() {
                if arg == "--output" {
                    if output.is_some() {
                        return Err(Error::new("duplicate --output argument"));
                    }
                    output = Some(required_path(
                        rest.next(),
                        "strata build <path> --output <path>",
                    )?);
                } else {
                    return Err(Error::new(format!("unexpected argument {arg:?}")));
                }
            }
            let source = read_source_file(&path)?;
            let checked = check_source(&source)?;
            let artifact = lower_to_artifact(&checked, &source)?;
            let artifact_path = output.unwrap_or(default_artifact_path(&path)?);
            write_artifact(&artifact_path, &artifact)?;
            println!(
                "strata: built {} -> {}",
                path.display(),
                artifact_path.display()
            );
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_strata_usage();
            Ok(())
        }
        Some(other) => Err(Error::new(format!("unknown strata command {other:?}"))),
        None => {
            print_strata_usage();
            Err(Error::new("missing strata command"))
        }
    }
}

fn required_path(value: Option<String>, usage: &str) -> Result<PathBuf> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(format!("missing path; usage: {usage}")))
}

fn read_source_file(path: &Path) -> Result<String> {
    let mut file = open_source_file(path)?;
    let metadata = file.metadata()?;
    validate_source_file_metadata(path, &metadata)?;
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(Error::new(format!(
            "source {} exceeds maximum size of {MAX_SOURCE_BYTES} bytes",
            path.display()
        )));
    }

    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(Error::new(format!(
            "source {} exceeds maximum size of {MAX_SOURCE_BYTES} bytes",
            path.display()
        )));
    }

    String::from_utf8(bytes).map_err(|err| {
        Error::new(format!(
            "source {} is not valid UTF-8: {err}",
            path.display()
        ))
    })
}

fn open_source_file(path: &Path) -> Result<fs::File> {
    reject_non_regular_source_path_before_open(path)?;
    match open_source_file_handle(path) {
        Ok(file) => Ok(file),
        Err(open_err) => {
            if fs::metadata(path)
                .map(|metadata| !metadata.is_file())
                .unwrap_or(false)
            {
                return Err(non_regular_source_path_error(path));
            }
            Err(open_err.into())
        }
    }
}

#[cfg(all(
    unix,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )
))]
fn open_source_file_handle(path: &Path) -> std::io::Result<fs::File> {
    use nix::fcntl::OFlag;
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(path)
}

#[cfg(not(unix))]
fn open_source_file_handle(path: &Path) -> std::io::Result<fs::File> {
    fs::File::open(path)
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))
))]
fn open_source_file_handle(_path: &Path) -> std::io::Result<fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "source file opening requires a nonblocking open flag for this Unix target",
    ))
}

fn reject_non_regular_source_path_before_open(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)?;
    validate_source_file_metadata(path, &metadata)
}

fn validate_source_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.is_file() {
        Ok(())
    } else {
        Err(non_regular_source_path_error(path))
    }
}

fn non_regular_source_path_error(path: &Path) -> Error {
    Error::new(format!(
        "source path {} is not a regular file",
        path.display()
    ))
}

fn ensure_no_extra_args(args: impl IntoIterator<Item = String>) -> Result<()> {
    let extras: Vec<String> = args.into_iter().collect();
    if extras.is_empty() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "unexpected arguments: {}",
            extras.join(" ")
        )))
    }
}

fn print_strata_usage() {
    println!("usage:");
    println!("  strata check <path.str>");
    println!("  strata build <path.str> [--output <path.mta>]");
}

pub fn run_strata_from_env() -> Result<()> {
    strata_main(env::args())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantle_artifact::{MAX_PROCESS_COUNT, MAX_STATE_VALUES_PER_PROCESS};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SOURCE_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn read_source_file_rejects_oversized_source() {
        let path = unique_source_path("oversized");
        fs::write(&path, vec![b'a'; MAX_SOURCE_BYTES + 1])
            .expect("oversized test source should be written");

        let err = read_source_file(&path).expect_err("oversized source should fail");

        assert!(err.to_string().contains("exceeds maximum size"));

        fs::remove_file(path).expect("test source should be removed");
    }

    #[test]
    fn read_source_file_rejects_non_utf8_source() {
        let path = unique_source_path("non-utf8");
        fs::write(&path, [0xff]).expect("non-UTF-8 test source should be written");

        let err = read_source_file(&path).expect_err("non-UTF-8 source should fail");

        assert!(err.to_string().contains("is not valid UTF-8"));

        fs::remove_file(path).expect("test source should be removed");
    }

    #[test]
    fn read_source_file_rejects_directory_source() {
        let path = unique_source_path("directory");
        fs::create_dir_all(&path).expect("test source dir should be created");

        let err = read_source_file(&path).expect_err("directory source should fail");

        assert!(err.to_string().contains("is not a regular file"));

        fs::remove_dir(path).expect("test source dir should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn read_source_file_rejects_fifo_source_before_opening() {
        let path = unique_source_path("fifo");
        create_fifo(&path);

        let err = read_source_file(&path).expect_err("fifo source should fail");

        assert!(err.to_string().contains("is not a regular file"));

        fs::remove_file(path).expect("test fifo should be removed");
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    #[test]
    fn open_source_file_handle_does_not_block_on_fifo_source() {
        let path = unique_source_path("fifo-handle");
        create_fifo(&path);

        let file = open_source_file_handle(&path).expect("fifo open should not block");
        let metadata = file.metadata().expect("fifo metadata should be available");

        assert!(!metadata.is_file());

        fs::remove_file(path).expect("test fifo should be removed");
    }

    #[cfg(unix)]
    fn create_fifo(path: &Path) {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;

        mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test fifo should be created");
    }

    #[test]
    fn strata_build_rejects_duplicate_output_argument() {
        let err = strata_main([
            "strata".to_string(),
            "build".to_string(),
            "examples/hello.str".to_string(),
            "--output".to_string(),
            "one.mta".to_string(),
            "--output".to_string(),
            "two.mta".to_string(),
        ])
        .expect_err("duplicate output should fail");

        assert!(err.to_string().contains("duplicate --output argument"));
    }

    #[test]
    fn strata_check_rejects_source_that_cannot_lower_to_artifact() {
        let path = unique_source_path("artifact-too-large-check");
        fs::write(&path, oversized_artifact_source())
            .expect("oversized-artifact test source should be written");

        let err = strata_main([
            "strata".to_string(),
            "check".to_string(),
            path.display().to_string(),
        ])
        .expect_err("check should fail when lowering rejects the checked source");

        assert_artifact_size_error(&err);

        fs::remove_file(path).expect("test source should be removed");
    }

    #[test]
    fn strata_build_rejects_lowering_failure_before_writing_output() {
        let source_path = unique_source_path("artifact-too-large-build");
        let output_path = unique_artifact_path("artifact-too-large-build-output");
        fs::write(&source_path, oversized_artifact_source())
            .expect("oversized-artifact test source should be written");

        let err = strata_main([
            "strata".to_string(),
            "build".to_string(),
            source_path.display().to_string(),
            "--output".to_string(),
            output_path.display().to_string(),
        ])
        .expect_err("build should fail when lowering rejects the checked source");

        assert_artifact_size_error(&err);
        assert!(
            !output_path.exists(),
            "build must not write an artifact after lowering failure"
        );

        fs::remove_file(source_path).expect("test source should be removed");
    }

    fn assert_artifact_size_error(err: &Error) {
        let Error::Artifact(artifact_err) = err else {
            panic!("expected artifact lowering error, got {err}");
        };
        assert!(artifact_err
            .to_string()
            .contains("encoded artifact exceeds maximum size"));
    }

    fn oversized_artifact_source() -> String {
        let state_values = (0..MAX_STATE_VALUES_PER_PROCESS)
            .map(|index| format!("S{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut source = format!(
            "module oversized_artifact;\nrecord Marker;\nenum MainState {{ {state_values} }}\nenum MainMsg {{ Start }}\n"
        );
        for process_index in 0..MAX_PROCESS_COUNT {
            let process_name = if process_index == 0 {
                "Main".to_string()
            } else {
                format!("Proc{process_index}")
            };
            source.push_str(&format!(
                r#"
proc {process_name} mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return S0; }}
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
            ));
        }
        assert!(
            source.len() <= MAX_SOURCE_BYTES,
            "test source must stay below the source size limit"
        );
        source
    }

    fn unique_source_path(name: &str) -> PathBuf {
        let index = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "strata-source-{name}-{}-{index}.str",
            std::process::id()
        ))
    }

    fn unique_artifact_path(name: &str) -> PathBuf {
        let index = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "strata-artifact-{name}-{}-{index}.mta",
            std::process::id()
        ))
    }
}
