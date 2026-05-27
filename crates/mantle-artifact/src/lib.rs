#![forbid(unsafe_code)]

mod artifact;
mod authority_summary;
mod constants;
mod error;
mod fields;
mod ids;
mod io;
mod validation;

pub use artifact::{
    ArtifactAction, ArtifactAuthority, ArtifactBranch, ArtifactCapabilityDescriptor,
    ArtifactEffect, ArtifactEnumVariant, ArtifactLoopElement, ArtifactMapEntry,
    ArtifactMessageVariant, ArtifactPayload, ArtifactProcess, ArtifactProcessRef,
    ArtifactProcessRefPayload, ArtifactRecordField, ArtifactScalarArithmeticOperator,
    ArtifactScalarOrderingOperator, ArtifactScalarType, ArtifactScalarValue, ArtifactSendTarget,
    ArtifactSpawnKind, ArtifactSpawnSite, ArtifactStateValue, ArtifactTransition, ArtifactType,
    ArtifactTypeField, ArtifactTypeKind, ArtifactValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, ArtifactValueShape, ArtifactValueTemplate,
    ArtifactValueTemplateField, ArtifactValueTemplateMapEntry, MantleArtifact, MapProjectionMode,
    NextState, StepResult, validate_value_enum_membership,
};
pub use authority_summary::{AuthoritySummaryFormat, render_artifact_authority_summary};
pub use constants::*;
pub use error::{Error, Result};
pub use ids::{
    AuthorityId, EffectOutcomeId, EnumVariantId, LoopElementId, MessageId, OutputId, ProcessId,
    ProcessRefId, SpawnSiteId, StateId, TypeId,
};
pub use io::{read_artifact, source_hash_fnv1a64, write_artifact};
pub use validation::{
    validate_message_label, validate_payload_value_label, validate_state_value_identity_label,
    validate_state_value_label,
};

#[cfg(test)]
mod tests;
