#![forbid(unsafe_code)]

mod artifact;
mod constants;
mod error;
mod fields;
mod ids;
mod io;
mod validation;

pub use artifact::{
    ArtifactAction, ArtifactEffect, ArtifactMessageVariant, ArtifactPayload, ArtifactProcess,
    ArtifactProcessRef, ArtifactProcessRefPayload, ArtifactSendTarget, ArtifactStateValue,
    ArtifactTransition, ArtifactType, ArtifactTypeKind, ArtifactValue, ArtifactValueTemplate,
    ArtifactValueTemplateField, ArtifactValueTemplateMapEntry, MantleArtifact, MapProjectionMode,
    NextState, StepResult,
};
pub use constants::*;
pub use error::{Error, Result};
pub use ids::{MessageId, OutputId, ProcessId, ProcessRefId, StateId, TypeId};
pub use io::{read_artifact, source_hash_fnv1a64, write_artifact};
pub use validation::{
    validate_message_label, validate_payload_value_label, validate_state_value_identity_label,
    validate_state_value_label,
};

#[cfg(test)]
mod tests;
