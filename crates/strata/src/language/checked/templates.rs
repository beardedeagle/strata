use std::sync::Arc;

use mantle_artifact::{ArtifactValue, MapProjectionMode};

use crate::language::ast::Identifier;

use super::{
    CheckedEffectOutcomeId, CheckedEnumVariantId, CheckedLoopElementId, CheckedProcessId,
    CheckedProcessRefId, CheckedStateId, CheckedTypeRef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedNextState {
    Current,
    Value(CheckedStateId),
    Template(CheckedValueTemplate),
    IfElse {
        condition: CheckedValueTemplate,
        then_state: Box<CheckedNextState>,
        else_state: Box<CheckedNextState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedPayloadValue {
    ty: CheckedTypeRef,
    value: Option<ArtifactValue>,
    label: String,
    process_ref: Option<CheckedProcessRefPayload>,
}

impl CheckedPayloadValue {
    pub(in crate::language) fn new(ty: CheckedTypeRef, value: ArtifactValue) -> Self {
        let label = value.label();
        Self {
            ty,
            value: Some(value),
            label,
            process_ref: None,
        }
    }

    pub(in crate::language) fn process_ref(
        ty: CheckedTypeRef,
        label: String,
        target: CheckedProcessId,
        pid: u64,
    ) -> Self {
        Self {
            ty,
            value: None,
            label,
            process_ref: Some(CheckedProcessRefPayload { target, pid }),
        }
    }

    pub(in crate::language) fn ty(&self) -> &CheckedTypeRef {
        &self.ty
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn value(&self) -> Option<&ArtifactValue> {
        self.value.as_ref()
    }

    pub(in crate::language) fn process_ref_payload(&self) -> Option<CheckedProcessRefPayload> {
        self.process_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedStateValue {
    ty: CheckedTypeRef,
    value: ArtifactValue,
    label: String,
    payload: Option<CheckedPayloadValue>,
}

impl CheckedStateValue {
    pub(in crate::language) fn new(ty: CheckedTypeRef, value: ArtifactValue) -> Self {
        let label = value.label();
        Self {
            ty,
            value,
            label,
            payload: None,
        }
    }

    pub(in crate::language) fn enum_variant(
        ty: CheckedTypeRef,
        value: ArtifactValue,
        payload: Option<CheckedPayloadValue>,
    ) -> Self {
        let label = value.label();
        Self {
            ty,
            value,
            label,
            payload,
        }
    }

    pub(in crate::language) fn ty(&self) -> &CheckedTypeRef {
        &self.ty
    }

    pub(in crate::language) fn value(&self) -> &ArtifactValue {
        &self.value
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn payload(&self) -> Option<&CheckedPayloadValue> {
        self.payload.as_ref()
    }

    pub(in crate::language) fn has_same_identity_as_payload(
        &self,
        payload: &CheckedPayloadValue,
    ) -> bool {
        self.ty == *payload.ty()
            && payload
                .value()
                .is_some_and(|payload_value| &self.value == payload_value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcessRefPayload {
    target: CheckedProcessId,
    pid: u64,
}

impl CheckedProcessRefPayload {
    pub(in crate::language) fn target(self) -> CheckedProcessId {
        self.target
    }

    pub(in crate::language) fn pid(self) -> u64 {
        self.pid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedValueTemplate {
    Literal(CheckedPayloadValue),
    ReceivedPayload {
        ty: CheckedTypeRef,
    },
    CurrentStatePayload {
        ty: CheckedTypeRef,
    },
    EnumPayload {
        ty: CheckedTypeRef,
        value: Box<CheckedValueTemplate>,
        variant: CheckedEnumVariantId,
    },
    RecordField {
        ty: CheckedTypeRef,
        record: Box<CheckedValueTemplate>,
        field: Identifier,
    },
    ListElement {
        ty: CheckedTypeRef,
        list: Box<CheckedValueTemplate>,
        index: usize,
        len: usize,
    },
    ListPrefixElement {
        ty: CheckedTypeRef,
        list: Box<CheckedValueTemplate>,
        index: usize,
        prefix_len: usize,
    },
    ListRest {
        ty: CheckedTypeRef,
        list: Box<CheckedValueTemplate>,
        prefix_len: usize,
    },
    MapValue {
        ty: CheckedTypeRef,
        map: Box<CheckedValueTemplate>,
        key: ArtifactValue,
        keys: Arc<[ArtifactValue]>,
        projection: MapProjectionMode,
    },
    MapRest {
        ty: CheckedTypeRef,
        map: Box<CheckedValueTemplate>,
        excluded_keys: Arc<[ArtifactValue]>,
    },
    ProcessRef {
        ty: CheckedTypeRef,
        target: CheckedProcessId,
        process_ref: CheckedProcessRefId,
    },
    LoopElement {
        ty: CheckedTypeRef,
        element: CheckedLoopElementId,
    },
    EffectOutcome {
        ty: CheckedTypeRef,
        outcome: CheckedEffectOutcomeId,
    },
    EnumVariant {
        ty: CheckedTypeRef,
        variant: CheckedEnumVariantId,
        payload: Box<CheckedValueTemplate>,
    },
    Record {
        ty: CheckedTypeRef,
        fields: Vec<CheckedValueTemplateField>,
    },
    List {
        ty: CheckedTypeRef,
        items: Vec<CheckedValueTemplate>,
    },
    Map {
        ty: CheckedTypeRef,
        entries: Vec<CheckedValueTemplateMapEntry>,
    },
    IfElse {
        ty: CheckedTypeRef,
        condition: Box<CheckedValueTemplate>,
        then_value: Box<CheckedValueTemplate>,
        else_value: Box<CheckedValueTemplate>,
    },
    Equality {
        ty: CheckedTypeRef,
        operand_ty: CheckedTypeRef,
        operator: CheckedValueEqualityOperator,
        left: Box<CheckedValueTemplate>,
        right: Box<CheckedValueTemplate>,
    },
    ScalarArithmetic {
        ty: CheckedTypeRef,
        operator: CheckedScalarArithmeticOperator,
        left: Box<CheckedValueTemplate>,
        right: Box<CheckedValueTemplate>,
    },
    ScalarOrdering {
        ty: CheckedTypeRef,
        operand_ty: CheckedTypeRef,
        operator: CheckedScalarOrderingOperator,
        left: Box<CheckedValueTemplate>,
        right: Box<CheckedValueTemplate>,
    },
    BooleanNot {
        ty: CheckedTypeRef,
        operand: Box<CheckedValueTemplate>,
    },
    BooleanBinary {
        ty: CheckedTypeRef,
        operator: CheckedValueBooleanOperator,
        left: Box<CheckedValueTemplate>,
        right: Box<CheckedValueTemplate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedValueEqualityOperator {
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedScalarArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedScalarOrderingOperator {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedValueBooleanOperator {
    And,
    Or,
}

impl CheckedValueTemplate {
    pub(in crate::language) fn result_type(&self) -> &CheckedTypeRef {
        match self {
            Self::Literal(value) => value.ty(),
            Self::ReceivedPayload { ty }
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
            | Self::EffectOutcome { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. }
            | Self::List { ty, .. }
            | Self::Map { ty, .. }
            | Self::IfElse { ty, .. }
            | Self::Equality { ty, .. }
            | Self::ScalarArithmetic { ty, .. }
            | Self::ScalarOrdering { ty, .. }
            | Self::BooleanNot { ty, .. }
            | Self::BooleanBinary { ty, .. } => ty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedValueTemplateField {
    name: Identifier,
    value: CheckedValueTemplate,
}

impl CheckedValueTemplateField {
    pub(in crate::language) fn new(name: Identifier, value: CheckedValueTemplate) -> Self {
        Self { name, value }
    }

    pub(in crate::language) fn name(&self) -> &Identifier {
        &self.name
    }

    pub(in crate::language) fn value(&self) -> &CheckedValueTemplate {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedValueTemplateMapEntry {
    key: CheckedValueTemplate,
    value: CheckedValueTemplate,
}

impl CheckedValueTemplateMapEntry {
    pub(in crate::language) fn new(key: CheckedValueTemplate, value: CheckedValueTemplate) -> Self {
        Self { key, value }
    }

    pub(in crate::language) fn key(&self) -> &CheckedValueTemplate {
        &self.key
    }

    pub(in crate::language) fn value(&self) -> &CheckedValueTemplate {
        &self.value
    }
}
