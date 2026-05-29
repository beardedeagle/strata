use mantle_artifact::{ArtifactValueShape, RecordFieldId};

use crate::language::Identifier;
use crate::language::checked::CheckedTypeRef;

use super::ArtifactTypeMap;

impl ArtifactTypeMap {
    pub(super) fn record_field_id(
        &self,
        record_ty: &CheckedTypeRef,
        field: &Identifier,
    ) -> mantle_artifact::Result<RecordFieldId> {
        let artifact_type = self.artifacts.get(record_ty.id().index()).ok_or_else(|| {
            mantle_artifact::Error::new(format!(
                "checked record type id {} is not in the checked type table",
                record_ty.id().as_u32()
            ))
        })?;
        let Some(ArtifactValueShape::Record { fields }) = &artifact_type.shape else {
            return Err(mantle_artifact::Error::new(format!(
                "checked type id {} is not a record type",
                record_ty.id().as_u32()
            )));
        };
        fields
            .iter()
            .position(|candidate| candidate.name == field.as_str())
            .map(RecordFieldId::from_index)
            .transpose()?
            .ok_or_else(|| {
                mantle_artifact::Error::new(format!(
                    "checked record type id {} has no field {}",
                    record_ty.id().as_u32(),
                    field
                ))
            })
    }
}
