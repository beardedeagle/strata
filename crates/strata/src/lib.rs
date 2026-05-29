#![forbid(unsafe_code)]

pub mod cli;
pub mod language;
mod source_loader;

pub use source_loader::{Error as SourceLoadError, LoadedSourceProgram};

pub fn load_root_source_program(
    path: &std::path::Path,
) -> std::result::Result<LoadedSourceProgram, SourceLoadError> {
    source_loader::load_root_source_program(path)
}
