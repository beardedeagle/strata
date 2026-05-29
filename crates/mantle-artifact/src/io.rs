use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use crate::{Error, MAX_ARTIFACT_BYTES, MantleArtifact, Result};

pub fn write_artifact(path: &Path, artifact: &MantleArtifact) -> Result<()> {
    artifact.validate()?;
    reject_symlink_path_components(path)?;
    reject_non_regular_artifact_output_path_before_open(path)?;
    prepare_artifact_output_parent(path)?;
    let mut file = open_artifact_output_file(path)?;
    validate_artifact_file_metadata(path, &file.metadata()?)?;
    validate_opened_artifact_path(path, &file)?;
    file.write_all(artifact.encode().as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn read_artifact(path: &Path) -> Result<MantleArtifact> {
    reject_symlink_path_components(path)?;
    let metadata = fs::symlink_metadata(path)?;
    validate_artifact_file_metadata(path, &metadata)?;
    if metadata.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err(Error::new(format!(
            "artifact {} is too large; maximum supported size is {MAX_ARTIFACT_BYTES} bytes",
            path.display()
        )));
    }
    let mut file = open_artifact_input_file(path)?;
    validate_artifact_file_metadata(path, &file.metadata()?)?;
    validate_opened_artifact_path(path, &file)?;
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

pub struct SourceHashFnv1a64 {
    hash: u64,
}

impl SourceHashFnv1a64 {
    pub fn new() -> Self {
        Self {
            hash: FNV1A64_OFFSET,
        }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.hash = fnv1a64_update(self.hash, bytes);
    }

    pub fn finish_hex(self) -> String {
        // Diagnostic correlation only; never use FNV as authority or integrity proof.
        format!("{:016x}", self.hash)
    }
}

impl Default for SourceHashFnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn source_hash_fnv1a64(source: &str) -> String {
    let mut hasher = SourceHashFnv1a64::new();
    hasher.update(source.as_bytes());
    hasher.finish_hex()
}

fn reject_non_regular_artifact_output_path_before_open(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_artifact_file_metadata(path, &metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
fn prepare_artifact_output_parent(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn prepare_artifact_output_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
            reject_symlink_path_components(parent)?;
        }
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn prepare_artifact_output_parent(_path: &Path) -> Result<()> {
    Err(unsupported_secure_artifact_io().into())
}

#[cfg(unix)]
fn open_artifact_input_file(path: &Path) -> std::io::Result<fs::File> {
    let file_name = secure_leaf_name(path)?;
    let parent = open_secure_parent_directory(path, false)?;
    let fd = nix::fcntl::openat(
        &parent,
        file_name,
        input_file_flags(),
        nix::sys::stat::Mode::empty(),
    )
    .map_err(nix_to_io_error)?;
    Ok(fs::File::from(fd))
}

#[cfg(windows)]
fn open_artifact_input_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_artifact_input_file(_path: &Path) -> std::io::Result<fs::File> {
    Err(unsupported_secure_artifact_io())
}

#[cfg(unix)]
fn open_artifact_output_file(path: &Path) -> std::io::Result<fs::File> {
    let file_name = secure_leaf_name(path)?;
    let parent = open_secure_parent_directory(path, true)?;
    let fd = nix::fcntl::openat(&parent, file_name, output_file_flags(), output_file_mode())
        .map_err(nix_to_io_error)?;
    Ok(fs::File::from(fd))
}

#[cfg(windows)]
fn open_artifact_output_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_artifact_output_file(_path: &Path) -> std::io::Result<fs::File> {
    Err(unsupported_secure_artifact_io())
}

#[cfg(unix)]
fn secure_leaf_name(path: &Path) -> std::io::Result<&std::ffi::OsStr> {
    path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("artifact path {} has no file name", path.display()),
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
                        "artifact path {} uses an unsupported prefix",
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
fn input_file_flags() -> nix::fcntl::OFlag {
    nix::fcntl::OFlag::O_RDONLY
        | nix::fcntl::OFlag::O_NONBLOCK
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

#[cfg(all(not(unix), not(windows)))]
fn unsupported_secure_artifact_io() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "artifact I/O requires secure file identity support on this target",
    )
}

fn validate_artifact_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if is_regular_artifact_file(metadata) {
        Ok(())
    } else {
        Err(non_regular_artifact_path_error(path))
    }
}

#[cfg(not(windows))]
fn is_regular_artifact_file(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

#[cfg(windows)]
fn is_regular_artifact_file(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

#[cfg(not(windows))]
fn validate_opened_artifact_path(_path: &Path, _file: &fs::File) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn validate_opened_artifact_path(path: &Path, file: &fs::File) -> Result<()> {
    let canonical_path = canonical_artifact_path_for_opened_path(path)?;
    let canonical_file = open_artifact_input_file(&canonical_path)?;
    validate_artifact_file_metadata(&canonical_path, &canonical_file.metadata()?)?;
    if same_opened_artifact_file(file, &canonical_file)? {
        Ok(())
    } else {
        Err(Error::new(format!(
            "artifact path {} changed while opening",
            path.display()
        )))
    }
}

#[cfg(windows)]
fn canonical_artifact_path_for_opened_path(path: &Path) -> Result<std::path::PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::new(format!("artifact path {} has no file name", path.display())))?;
    Ok(fs::canonicalize(parent)?.join(file_name))
}

#[cfg(windows)]
fn same_opened_artifact_file(before: &fs::File, after: &fs::File) -> Result<bool> {
    Ok(windows_file_fingerprint(&before.metadata()?)
        == windows_file_fingerprint(&after.metadata()?))
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowsFileFingerprint {
    file_attributes: u32,
    creation_time: u64,
    last_write_time: u64,
    file_size: u64,
}

#[cfg(windows)]
fn windows_file_fingerprint(metadata: &fs::Metadata) -> WindowsFileFingerprint {
    use std::os::windows::fs::MetadataExt;

    WindowsFileFingerprint {
        file_attributes: metadata.file_attributes(),
        creation_time: metadata.creation_time(),
        last_write_time: metadata.last_write_time(),
        file_size: metadata.file_size(),
    }
}

#[cfg(unix)]
fn reject_symlink_path_components(path: &Path) -> Result<()> {
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::new(format!(
                    "artifact path {} must not include symbolic link component {}",
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

#[cfg(windows)]
fn reject_symlink_path_components(path: &Path) -> Result<()> {
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match component {
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::CurDir => continue,
            std::path::Component::ParentDir | std::path::Component::Normal(_) => {}
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if !is_regular_path_component(&metadata) => {
                return Err(Error::new(format!(
                    "artifact path {} must not include symbolic link component {}",
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

#[cfg(windows)]
fn is_regular_path_component(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

#[cfg(all(not(unix), not(windows)))]
fn reject_symlink_path_components(_path: &Path) -> Result<()> {
    Err(unsupported_secure_artifact_io().into())
}

fn non_regular_artifact_path_error(path: &Path) -> Error {
    Error::new(format!(
        "artifact path {} is not a regular file",
        path.display()
    ))
}

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

fn fnv1a64_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

#[cfg(test)]
mod hash_tests {
    use super::*;

    #[test]
    fn streaming_source_hash_matches_contiguous_source_hash() {
        let mut hasher = SourceHashFnv1a64::new();
        hasher.update(b"abc");
        hasher.update(b"");
        hasher.update(b"def");

        assert_eq!(hasher.finish_hex(), source_hash_fnv1a64("abcdef"));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    #[test]
    fn secure_artifact_output_open_rejects_symlink_parent_without_preflight() {
        use std::os::unix::fs::symlink;

        let real_dir = unique_artifact_path("real-parent");
        let link_dir = unique_artifact_path("link-parent");
        fs::create_dir_all(&real_dir).expect("test real dir should be created");
        symlink(&real_dir, &link_dir).expect("test parent symlink should be created");
        let path = link_dir.join("out.mta");

        let err = open_artifact_output_file(&path)
            .expect_err("descriptor-relative output open should reject symlink parent");

        assert!(!err.to_string().is_empty());
        assert!(
            !real_dir.join("out.mta").exists(),
            "secure output open must not create files through a symlink parent"
        );

        fs::remove_file(link_dir).expect("test parent symlink should be removed");
        fs::remove_dir(real_dir).expect("test real dir should be removed");
    }

    #[test]
    fn secure_artifact_input_open_rejects_symlink_parent_without_preflight() {
        use std::os::unix::fs::symlink;

        let real_dir = unique_artifact_path("real-parent-input");
        let link_dir = unique_artifact_path("link-parent-input");
        fs::create_dir_all(&real_dir).expect("test real dir should be created");
        fs::write(real_dir.join("in.mta"), "artifact").expect("test input should be written");
        symlink(&real_dir, &link_dir).expect("test parent symlink should be created");
        let path = link_dir.join("in.mta");

        let err = open_artifact_input_file(&path)
            .expect_err("descriptor-relative input open should reject symlink parent");

        assert!(!err.to_string().is_empty());

        fs::remove_file(link_dir).expect("test parent symlink should be removed");
        fs::remove_file(real_dir.join("in.mta")).expect("test input should be removed");
        fs::remove_dir(real_dir).expect("test real dir should be removed");
    }

    fn create_fifo(path: &Path) {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;

        mkfifo(path, Mode::S_IRUSR | Mode::S_IWUSR).expect("test FIFO should be created");
    }

    fn unique_artifact_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        let temp_dir = std::env::temp_dir()
            .canonicalize()
            .unwrap_or_else(|_| std::env::temp_dir());
        temp_dir.join(format!(
            "mantle-artifact-{name}-{}-{nanos}.mta",
            std::process::id()
        ))
    }
}
