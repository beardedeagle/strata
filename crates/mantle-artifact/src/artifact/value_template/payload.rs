use super::model::ArtifactValue;
use crate::{ProcessId, Result, TypeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub ty: TypeId,
    pub value: ArtifactValue,
    pub process_ref: Option<ArtifactProcessRefPayload>,
}

impl ArtifactPayload {
    pub fn value(ty: TypeId, value: ArtifactValue) -> Result<Self> {
        value.validate_without_process_ref("payload value")?;
        Ok(Self {
            ty,
            value,
            process_ref: None,
        })
    }

    pub fn process_ref(ty: TypeId, target_process: ProcessId, pid: u64) -> Result<Self> {
        let value = ArtifactValue::process_ref(ty, pid);
        value.validate("process reference payload value")?;
        Ok(Self {
            ty,
            value,
            process_ref: Some(ArtifactProcessRefPayload {
                target_process,
                pid,
            }),
        })
    }

    pub fn label(&self) -> String {
        self.value.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProcessRefPayload {
    pub target_process: ProcessId,
    pub pid: u64,
}
