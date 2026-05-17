use crate::{EnumVariantId, LoopElementId, ProcessId, ProcessRefId, TypeId};

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
