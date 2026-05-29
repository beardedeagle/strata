use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::language::{
    Error as LanguageError, Identifier, MAX_SOURCE_BYTES, MAX_SOURCE_PROGRAM_BYTES,
    MAX_SOURCE_UNIT_COUNT, SourceProgram, SourceProvenanceHash, SourceUnit, SourceUnitId,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Language(LanguageError),
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
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Message(_) => None,
            Self::Language(err) => Some(err),
            Self::Io(err) => Some(err),
        }
    }
}

impl From<LanguageError> for Error {
    fn from(value: LanguageError) -> Self {
        Self::Language(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub struct LoadedSourceProgram {
    program: SourceProgram,
    source_hash: SourceProvenanceHash,
}

impl LoadedSourceProgram {
    pub fn into_parts(self) -> (SourceProgram, SourceProvenanceHash) {
        (self.program, self.source_hash)
    }
}

pub fn load_root_source_program(path: &Path) -> Result<LoadedSourceProgram> {
    let mut loader = SourceProgramLoader::default();
    let root = loader.load_unit(path, None)?;
    let program = SourceProgram::new(root, loader.units)?;
    let source_hash = program.source_provenance_hash();
    Ok(LoadedSourceProgram {
        program,
        source_hash,
    })
}

#[derive(Default)]
struct SourceProgramLoader {
    units: Vec<SourceUnit>,
    unit_paths: Vec<PathBuf>,
    by_path: BTreeMap<PathBuf, SourceUnitId>,
    by_module: BTreeMap<String, SourceUnitId>,
    total_source_bytes: usize,
}

impl SourceProgramLoader {
    fn load_unit(
        &mut self,
        path: &Path,
        expected_module: Option<&Identifier>,
    ) -> Result<SourceUnitId> {
        let OpenedSourceFile {
            file,
            metadata,
            canonical_path,
        } = open_canonical_source_file(path)?;
        if let Some(existing) = self.by_path.get(&canonical_path).copied() {
            self.validate_existing_module(existing, expected_module)?;
            return Ok(existing);
        }

        if self.units.len() >= MAX_SOURCE_UNIT_COUNT {
            return Err(Error::new(format!(
                "source_unit_count must be no greater than {MAX_SOURCE_UNIT_COUNT}"
            )));
        }

        let source = read_opened_source_file(path, file, &metadata)?;
        self.validate_total_source_bytes(source.len())?;
        let id = SourceUnitId::from_index(self.units.len())?;
        let source_unit = SourceUnit::parse(id, source)?;
        let module = source_unit.module();
        if let Some(expected_module) = expected_module {
            if module.name.as_str() != expected_module.as_str() {
                return Err(Error::new(format!(
                    "import {} loaded from {} declares module {}",
                    expected_module,
                    path.display(),
                    module.name
                )));
            }
        }

        if let Some(existing) = self.by_module.get(module.name.as_str()).copied() {
            let previous_path = self
                .unit_paths
                .get(existing.index())
                .map(PathBuf::as_path)
                .unwrap_or_else(|| Path::new("<unknown>"));
            return Err(Error::new(format!(
                "duplicate module identity {} declared by {} and {}",
                module.name,
                previous_path.display(),
                canonical_path.display()
            )));
        }

        self.by_path.insert(canonical_path.clone(), id);
        self.by_module.insert(module.name.to_string(), id);
        let imports = module.imports.clone();
        self.units.push(source_unit);
        self.unit_paths.push(canonical_path.clone());

        for import in imports {
            let import_path = validated_imported_source_path(&canonical_path, &import.module)?;
            self.load_unit(&import_path, Some(&import.module))?;
        }

        Ok(id)
    }

    fn validate_existing_module(
        &self,
        existing: SourceUnitId,
        expected_module: Option<&Identifier>,
    ) -> Result<()> {
        let Some(expected_module) = expected_module else {
            return Ok(());
        };
        let unit = self.units.get(existing.index()).ok_or_else(|| {
            Error::new(format!(
                "loaded source unit id {} is not declared",
                existing.as_u32()
            ))
        })?;
        if unit.module().name.as_str() == expected_module.as_str() {
            Ok(())
        } else {
            Err(Error::new(format!(
                "import {expected_module} resolved to loaded module {}",
                unit.module().name
            )))
        }
    }

    fn validate_total_source_bytes(&mut self, source_len: usize) -> Result<()> {
        let total = self
            .total_source_bytes
            .checked_add(source_len)
            .ok_or_else(|| Error::new("source program byte count overflowed"))?;
        if total > MAX_SOURCE_PROGRAM_BYTES {
            return Err(Error::new(format!(
                "source program exceeds maximum size of {MAX_SOURCE_PROGRAM_BYTES} bytes"
            )));
        }
        self.total_source_bytes = total;
        Ok(())
    }
}

fn canonical_source_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|err| {
        Error::new(format!(
            "source {} could not be resolved: {err}",
            path.display()
        ))
    })
}

fn imported_source_path(source_path: &Path, module: &Identifier) -> Result<PathBuf> {
    let parent = source_path.parent().ok_or_else(|| {
        Error::new(format!(
            "source {} has no parent directory for import {module}",
            source_path.display()
        ))
    })?;
    Ok(parent.join(format!("{module}.str")))
}

fn validated_imported_source_path(source_path: &Path, module: &Identifier) -> Result<PathBuf> {
    let import_path = imported_source_path(source_path, module)?;
    let canonical_path = canonical_source_path(&import_path)?;
    let source_dir = source_path.parent().ok_or_else(|| {
        Error::new(format!(
            "source {} has no parent directory for import {module}",
            source_path.display()
        ))
    })?;
    if canonical_path.parent() == Some(source_dir) {
        Ok(import_path)
    } else {
        Err(Error::new(format!(
            "import {module} resolved outside source directory: {}",
            canonical_path.display()
        )))
    }
}

#[cfg(test)]
fn read_source_file(path: &Path) -> Result<String> {
    let OpenedSourceFile { file, metadata, .. } = open_canonical_source_file(path)?;
    read_opened_source_file(path, file, &metadata)
}

struct OpenedSourceFile {
    file: fs::File,
    metadata: fs::Metadata,
    canonical_path: PathBuf,
}

fn open_canonical_source_file(path: &Path) -> Result<OpenedSourceFile> {
    let (file, metadata) = open_source_file(path)?;
    let canonical_path = canonical_source_path_for_opened_source(path)?;
    validate_canonical_source_path(path, &canonical_path, &metadata)?;
    Ok(OpenedSourceFile {
        file,
        metadata,
        canonical_path,
    })
}

#[cfg(unix)]
fn canonical_source_path_for_opened_source(path: &Path) -> Result<PathBuf> {
    canonical_source_path(path)
}

#[cfg(windows)]
fn canonical_source_path_for_opened_source(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(format!("source {} has no parent directory", path.display())))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::new(format!("source {} has no file name", path.display())))?;
    Ok(canonical_source_path(parent)?.join(file_name))
}

#[cfg(all(not(unix), not(windows)))]
fn canonical_source_path_for_opened_source(path: &Path) -> Result<PathBuf> {
    canonical_source_path(path)
}

#[cfg(unix)]
fn validate_canonical_source_path(
    original_path: &Path,
    canonical_path: &Path,
    opened_metadata: &fs::Metadata,
) -> Result<()> {
    let canonical_metadata = fs::metadata(canonical_path)?;
    validate_source_file_metadata(canonical_path, &canonical_metadata)?;
    if same_source_file(opened_metadata, &canonical_metadata) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "source path {} changed while resolving",
            original_path.display()
        )))
    }
}

#[cfg(windows)]
fn validate_canonical_source_path(
    original_path: &Path,
    _canonical_path: &Path,
    _opened_metadata: &fs::Metadata,
) -> Result<()> {
    Err(Error::new(format!(
        "source loading is unsupported on Windows because source file identity cannot be checked securely for {}",
        original_path.display()
    )))
}

#[cfg(all(not(unix), not(windows)))]
fn validate_canonical_source_path(
    original_path: &Path,
    _canonical_path: &Path,
    _opened_metadata: &fs::Metadata,
) -> Result<()> {
    Err(Error::new(format!(
        "source loading is unsupported on this target because source file identity cannot be checked for {}",
        original_path.display()
    )))
}

fn read_opened_source_file(
    path: &Path,
    mut file: fs::File,
    metadata: &fs::Metadata,
) -> Result<String> {
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(Error::new(format!(
            "source {} exceeds maximum size of {MAX_SOURCE_BYTES} bytes",
            path.display()
        )));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
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

#[cfg(unix)]
fn open_source_file(path: &Path) -> Result<(fs::File, fs::Metadata)> {
    let pre_open_metadata = reject_non_regular_source_path_before_open(path)?;
    match open_source_file_handle(path) {
        Ok(file) => {
            let metadata = validate_opened_source_file(path, &pre_open_metadata, &file)?;
            Ok((file, metadata))
        }
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

#[cfg(windows)]
fn open_source_file(path: &Path) -> Result<(fs::File, fs::Metadata)> {
    Err(Error::new(format!(
        "source loading is unsupported on Windows because source file identity cannot be checked securely for {}",
        path.display()
    )))
}

#[cfg(all(not(unix), not(windows)))]
fn open_source_file(path: &Path) -> Result<(fs::File, fs::Metadata)> {
    Err(Error::new(format!(
        "source loading is unsupported on this target because source file identity cannot be checked for {}",
        path.display()
    )))
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
        .custom_flags((OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW).bits())
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_source_file_handle(_path: &Path) -> std::io::Result<fs::File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "source file opening is unsupported on this target",
    ))
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

#[cfg(unix)]
fn reject_non_regular_source_path_before_open(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    validate_source_file_metadata(path, &metadata)?;
    Ok(metadata)
}

#[cfg(unix)]
fn validate_opened_source_file(
    path: &Path,
    expected: &fs::Metadata,
    file: &fs::File,
) -> Result<fs::Metadata> {
    let metadata = file.metadata()?;
    validate_source_file_metadata(path, &metadata)?;
    if !same_source_file(expected, &metadata) {
        return Err(Error::new(format!(
            "source path {} changed while opening",
            path.display()
        )));
    }
    Ok(metadata)
}

#[cfg(unix)]
fn validate_source_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if metadata.is_file() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(non_regular_source_path_error(path))
    }
}

#[cfg(unix)]
fn non_regular_source_path_error(path: &Path) -> Error {
    Error::new(format!(
        "source path {} is not a regular file",
        path.display()
    ))
}

#[cfg(unix)]
fn same_source_file(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    before.dev() == after.dev() && before.ino() == after.ino()
}

#[cfg(test)]
mod tests;
