use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::{Error, MantleArtifact, Result, MAX_ARTIFACT_BYTES};

pub fn default_artifact_path(source_path: &Path) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Error::new(format!(
                "source path {} has no UTF-8 file stem",
                source_path.display()
            ))
        })?;
    Ok(Path::new("target")
        .join("strata")
        .join(format!("{stem}.mta")))
}

pub fn write_artifact(path: &Path, artifact: &MantleArtifact) -> Result<()> {
    artifact.validate()?;
    reject_non_regular_artifact_output_path_before_open(path)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut file = open_artifact_output_file(path)?;
    validate_artifact_file_metadata(path, &file.metadata()?)?;
    file.write_all(artifact.encode().as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn read_artifact(path: &Path) -> Result<MantleArtifact> {
    let metadata = fs::metadata(path)?;
    validate_artifact_file_metadata(path, &metadata)?;
    if metadata.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err(Error::new(format!(
            "artifact {} is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes",
            path.display()
        )));
    }
    let mut file = open_artifact_input_file(path)?;
    validate_artifact_file_metadata(path, &file.metadata()?)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((MAX_ARTIFACT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "artifact {} is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes",
            path.display()
        )));
    }
    let contents = String::from_utf8(bytes).map_err(|err| {
        Error::new(format!(
            "artifact {} is not valid UTF-8: {err}",
            path.display()
        ))
    })?;
    MantleArtifact::decode(&contents)
}

pub fn source_hash_fnv1a64(source: &str) -> String {
    format!("{:016x}", fnv1a64(source.as_bytes()))
}

fn reject_non_regular_artifact_output_path_before_open(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => validate_artifact_file_metadata(path, &metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
fn open_artifact_input_file(path: &Path) -> std::io::Result<fs::File> {
    use nix::fcntl::OFlag;
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(path)
}

#[cfg(not(unix))]
fn open_artifact_input_file(path: &Path) -> std::io::Result<fs::File> {
    fs::File::open(path)
}

#[cfg(unix)]
fn open_artifact_output_file(path: &Path) -> std::io::Result<fs::File> {
    use nix::fcntl::OFlag;
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(path)
}

#[cfg(not(unix))]
fn open_artifact_output_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
}

fn validate_artifact_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.is_file() {
        Ok(())
    } else {
        Err(non_regular_artifact_path_error(path))
    }
}

fn non_regular_artifact_path_error(path: &Path) -> Error {
    Error::new(format!(
        "artifact path {} is not a regular file",
        path.display()
    ))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn opened_non_regular_artifact_handle_is_rejected() {
        let path = unique_artifact_path("opened-fifo");
        create_fifo(&path);
        let file = open_artifact_input_file(&path).expect("FIFO input open should not block");

        let err = validate_artifact_file_metadata(&path, &file.metadata().expect("metadata"))
            .expect_err("opened FIFO handle should fail regular-file validation");

        assert!(err.to_string().contains("is not a regular file"));
        fs::remove_file(path).expect("test FIFO should be removed");
    }

    #[cfg(unix)]
    fn create_fifo(path: &Path) {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;

        mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test FIFO should be created");
    }

    #[cfg(unix)]
    fn unique_artifact_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "strata-artifact-{name}-{}-{nanos}.mta",
            std::process::id()
        ))
    }
}
