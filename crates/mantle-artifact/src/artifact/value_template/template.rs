use std::collections::BTreeSet;

mod admission;
mod dependencies;
mod evaluation;

use super::super::{
    ArtifactStateValue, ArtifactType, ArtifactTypeField, ArtifactTypeKind, ArtifactValueShape,
    MantleArtifact,
};
use super::model::{
    ArtifactMapEntry, ArtifactRecordField, ArtifactValue, ArtifactValueBooleanOperator,
    ArtifactValueEqualityOperator, ArtifactValueTemplate, ArtifactValueTemplateField,
};
use super::payload::ArtifactPayload;
use super::projection::{
    ProjectionKeySetKind, validate_projection_key_set, validate_projection_keys,
};
use crate::validation::{validate_count, validate_ident_field, validate_value_label};
use crate::{Error, MAX_VALUE_TEMPLATE_DEPTH, MAX_VALUE_TEMPLATE_FIELDS, Result, TypeId};

#[derive(Debug, Clone, Copy)]
pub(in crate::artifact) struct ValueTemplatePayloadValidation {
    expected_type: Option<TypeId>,
    received_payload_type: Option<TypeId>,
    current_state_payload_type: Option<TypeId>,
    allow_direct_process_ref: bool,
}

impl ValueTemplatePayloadValidation {
    pub(in crate::artifact) const fn new(
        expected_type: Option<TypeId>,
        received_payload_type: Option<TypeId>,
        current_state_payload_type: Option<TypeId>,
        allow_direct_process_ref: bool,
    ) -> Self {
        Self {
            expected_type,
            received_payload_type,
            current_state_payload_type,
            allow_direct_process_ref,
        }
    }

    const fn nested(self) -> Self {
        Self {
            expected_type: None,
            allow_direct_process_ref: false,
            ..self
        }
    }

    const fn with_expected_type(self, expected_type: Option<TypeId>) -> Self {
        Self {
            expected_type,
            ..self
        }
    }
}

impl ArtifactValueTemplate {
    pub fn result_type(&self) -> TypeId {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::EnumPayload { ty, .. }
            | Self::RecordField { ty, .. }
            | Self::ListElement { ty, .. }
            | Self::ListPrefixElement { ty, .. }
            | Self::ListRest { ty, .. }
            | Self::MapValue { ty, .. }
            | Self::MapRest { ty, .. }
            | Self::ProcessRef { ty, .. }
            | Self::LoopElement { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. }
            | Self::List { ty, .. }
            | Self::Map { ty, .. }
            | Self::IfElse { ty, .. }
            | Self::Equality { ty, .. }
            | Self::ScalarArithmetic { ty, .. }
            | Self::ScalarOrdering { ty, .. }
            | Self::BooleanNot { ty, .. }
            | Self::BooleanBinary { ty, .. } => *ty,
        }
    }
}
