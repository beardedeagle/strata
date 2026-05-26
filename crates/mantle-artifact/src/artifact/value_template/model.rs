use crate::{EnumVariantId, LoopElementId, ProcessId, ProcessRefId, TypeId};

use super::scalar::{
    ArtifactScalarArithmeticOperator, ArtifactScalarOrderingOperator, ArtifactScalarValue,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArtifactRecordField {
    pub name: String,
    pub value: ArtifactValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArtifactMapEntry {
    pub key: ArtifactValue,
    pub value: ArtifactValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapProjectionMode {
    Exact,
    Subset,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactValue {
    Atom(String),
    Scalar(ArtifactScalarValue),
    EnumVariant {
        variant: String,
        payload: Box<ArtifactValue>,
    },
    Record {
        constructor: String,
        fields: Vec<ArtifactRecordField>,
    },
    List(Vec<ArtifactValue>),
    Map(Vec<ArtifactMapEntry>),
    ProcessRef {
        type_id: TypeId,
        pid: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueTemplate {
    Literal {
        ty: TypeId,
        value: ArtifactValue,
    },
    ReceivedPayload {
        ty: TypeId,
    },
    CurrentStatePayload {
        ty: TypeId,
    },
    EnumPayload {
        ty: TypeId,
        value: Box<ArtifactValueTemplate>,
        variant: EnumVariantId,
    },
    RecordField {
        ty: TypeId,
        record: Box<ArtifactValueTemplate>,
        field: String,
    },
    ListElement {
        ty: TypeId,
        list: Box<ArtifactValueTemplate>,
        index: usize,
        len: usize,
    },
    ListPrefixElement {
        ty: TypeId,
        list: Box<ArtifactValueTemplate>,
        index: usize,
        prefix_len: usize,
    },
    ListRest {
        ty: TypeId,
        list: Box<ArtifactValueTemplate>,
        prefix_len: usize,
    },
    MapValue {
        ty: TypeId,
        map: Box<ArtifactValueTemplate>,
        key: ArtifactValue,
        keys: Vec<ArtifactValue>,
        projection: MapProjectionMode,
    },
    MapRest {
        ty: TypeId,
        map: Box<ArtifactValueTemplate>,
        excluded_keys: Vec<ArtifactValue>,
    },
    ProcessRef {
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    LoopElement {
        ty: TypeId,
        element: LoopElementId,
    },
    EnumVariant {
        ty: TypeId,
        variant: EnumVariantId,
        payload: Box<ArtifactValueTemplate>,
    },
    Record {
        ty: TypeId,
        fields: Vec<ArtifactValueTemplateField>,
    },
    List {
        ty: TypeId,
        items: Vec<ArtifactValueTemplate>,
    },
    Map {
        ty: TypeId,
        entries: Vec<ArtifactValueTemplateMapEntry>,
    },
    IfElse {
        ty: TypeId,
        condition: Box<ArtifactValueTemplate>,
        then_value: Box<ArtifactValueTemplate>,
        else_value: Box<ArtifactValueTemplate>,
    },
    Equality {
        ty: TypeId,
        operand_ty: TypeId,
        operator: ArtifactValueEqualityOperator,
        left: Box<ArtifactValueTemplate>,
        right: Box<ArtifactValueTemplate>,
    },
    ScalarArithmetic {
        ty: TypeId,
        operator: ArtifactScalarArithmeticOperator,
        left: Box<ArtifactValueTemplate>,
        right: Box<ArtifactValueTemplate>,
    },
    ScalarOrdering {
        ty: TypeId,
        operand_ty: TypeId,
        operator: ArtifactScalarOrderingOperator,
        left: Box<ArtifactValueTemplate>,
        right: Box<ArtifactValueTemplate>,
    },
    BooleanNot {
        ty: TypeId,
        operand: Box<ArtifactValueTemplate>,
    },
    BooleanBinary {
        ty: TypeId,
        operator: ArtifactValueBooleanOperator,
        left: Box<ArtifactValueTemplate>,
        right: Box<ArtifactValueTemplate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactValueEqualityOperator {
    Equal,
    NotEqual,
}

impl ArtifactValueEqualityOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "eq",
            Self::NotEqual => "ne",
        }
    }

    pub fn parse(field: &str, value: &str) -> crate::Result<Self> {
        match value {
            "eq" => Ok(Self::Equal),
            "ne" => Ok(Self::NotEqual),
            _ => Err(crate::Error::new(format!(
                "{field} has invalid equality operator {value:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactValueBooleanOperator {
    And,
    Or,
}

impl ArtifactValueBooleanOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }

    pub fn parse(field: &str, value: &str) -> crate::Result<Self> {
        match value {
            "and" => Ok(Self::And),
            "or" => Ok(Self::Or),
            _ => Err(crate::Error::new(format!(
                "{field} has invalid boolean operator {value:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateField {
    pub name: String,
    pub value: ArtifactValueTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateMapEntry {
    pub key: ArtifactValueTemplate,
    pub value: ArtifactValueTemplate,
}
