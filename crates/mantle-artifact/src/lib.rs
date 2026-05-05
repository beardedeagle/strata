#![forbid(unsafe_code)]

mod artifact;
mod constants;
mod error;
mod fields;
mod ids;
mod io;
mod validation;

pub use artifact::{
    ArtifactAction, ArtifactProcess, ArtifactProcessRef, ArtifactTransition, MantleArtifact,
    NextState, StepResult,
};
pub use constants::*;
pub use error::{Error, Result};
pub use ids::{MessageId, OutputId, ProcessId, ProcessRefId, StateId};
pub use io::{default_artifact_path, read_artifact, source_hash_fnv1a64, write_artifact};
pub use validation::validate_state_value_label;

#[cfg(test)]
mod tests;
