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
    ArtifactComponent, ArtifactEffect, ArtifactEnumVariant, ArtifactLoopElement, ArtifactMapEntry,
    ArtifactMessageVariant, ArtifactPayload, ArtifactPort, ArtifactProcess, ArtifactProcessRef,
    ArtifactProcessRefPayload, ArtifactProtocol, ArtifactRecordField,
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactScalarType,
    ArtifactScalarValue, ArtifactSendTarget, ArtifactSpawnKind, ArtifactSpawnSite,
    ArtifactStateValue, ArtifactSupervisorChild, ArtifactSupervisorChildMode,
    ArtifactSupervisorPlan, ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy,
    ArtifactTransition, ArtifactType, ArtifactTypeField, ArtifactTypeKind, ArtifactValue,
    ArtifactValueBooleanOperator, ArtifactValueEqualityOperator, ArtifactValueShape,
    ArtifactValueTemplate, ArtifactValueTemplateField, ArtifactValueTemplateMapEntry,
    MantleArtifact, MapProjectionMode, NextState, StepResult, validate_value_enum_membership,
};
pub use authority_summary::{AuthoritySummaryFormat, render_artifact_authority_summary};
pub use constants::*;
pub use error::{Error, Result};
pub use ids::{
    AuthorityId, ComponentId, EffectOutcomeId, EnumVariantId, LoopElementId, MessageId, OutputId,
    PortId, ProcessId, ProcessRefId, ProtocolId, RecordFieldId, SpawnSiteId, StateId,
    SupervisorChildId, SupervisorId, TypeId,
};
pub use io::{SourceHashFnv1a64, read_artifact, source_hash_fnv1a64, write_artifact};
pub use validation::{
    validate_message_label, validate_payload_value_label, validate_state_value_identity_label,
    validate_state_value_label,
};

#[cfg(test)]
mod tests;
