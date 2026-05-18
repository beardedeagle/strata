mod model;
mod parsing;
mod payload;
mod projection;
mod template;
mod value;

pub use model::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, MapProjectionMode,
};
pub use payload::{ArtifactPayload, ArtifactProcessRefPayload};
pub(in crate::artifact) use template::ValueTemplatePayloadValidation;
