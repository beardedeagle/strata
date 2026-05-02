use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use mantle_artifact::{default_artifact_path, write_artifact, Error, Result};

use crate::language::{check_source, MAX_SOURCE_BYTES};

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
            let artifact = checked.to_artifact(&source)?;
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
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(unix_nonblocking_open_flag())
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

#[cfg(any(target_os = "linux", target_os = "android"))]
fn unix_nonblocking_open_flag() -> i32 {
    0o4000
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn unix_nonblocking_open_flag() -> i32 {
    0x0004
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

    fn unique_source_path(name: &str) -> PathBuf {
        let index = TEST_SOURCE_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "strata-source-{name}-{}-{index}.str",
            std::process::id()
        ))
    }
}
