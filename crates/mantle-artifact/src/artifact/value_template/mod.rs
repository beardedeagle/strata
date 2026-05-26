mod model;
mod parsing;
mod payload;
mod projection;
mod scalar;
mod template;
mod value;

pub use model::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, ArtifactValueTemplate, ArtifactValueTemplateField,
    ArtifactValueTemplateMapEntry, MapProjectionMode,
};
pub use payload::{ArtifactPayload, ArtifactProcessRefPayload};
pub use scalar::{
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactScalarType,
    ArtifactScalarValue,
};
pub(in crate::artifact) use template::ValueTemplatePayloadValidation;
