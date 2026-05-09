use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactValueTemplate {
    Literal {
        ty: TypeId,
        value: String,
    },
    ReceivedPayload {
        ty: TypeId,
    },
    CurrentStatePayload {
        ty: TypeId,
    },
    ProcessRef {
        ty: TypeId,
        target_process: ProcessId,
        process_ref: ProcessRefId,
    },
    EnumVariant {
        ty: TypeId,
        variant: String,
        payload: Box<ArtifactValueTemplate>,
    },
    Record {
        ty: TypeId,
        fields: Vec<ArtifactValueTemplateField>,
    },
}

impl ArtifactValueTemplate {
    pub fn result_type(&self) -> TypeId {
        match self {
            Self::Literal { ty, .. }
            | Self::ReceivedPayload { ty }
            | Self::CurrentStatePayload { ty }
            | Self::ProcessRef { ty, .. }
            | Self::EnumVariant { ty, .. }
            | Self::Record { ty, .. } => *ty,
        }
    }

    pub fn evaluate_state_value(
        &self,
        received_payload: Option<&ArtifactPayload>,
        current_state_payload: Option<&ArtifactPayload>,
        type_label: &dyn Fn(TypeId) -> Result<String>,
    ) -> Result<ArtifactStateValue> {
        match self {
            Self::Literal { ty, value } => Ok(ArtifactStateValue::new(*ty, value.clone())),
            Self::ReceivedPayload { ty } => {
                let payload = received_payload.ok_or_else(|| {
                    Error::new("received payload template requires a payload-bearing message")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                Ok(ArtifactStateValue::new(payload.ty, payload.value.clone()))
            }
            Self::CurrentStatePayload { ty } => {
                let payload = current_state_payload.ok_or_else(|| {
                    Error::new("current state payload template requires a payload-bearing state")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "current state payload has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                if payload.process_ref.is_some() {
                    return Err(Error::new(
                        "process reference payloads are not valid state values",
                    ));
                }
                Ok(ArtifactStateValue::new(payload.ty, payload.value.clone()))
            }
            Self::ProcessRef { .. } => Err(Error::new(
                "process reference template requires runtime process reference bindings",
            )),
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                let payload = payload.evaluate_state_value(
                    received_payload,
                    current_state_payload,
                    type_label,
                )?;
                let value = format!("{variant}({})", payload.value);
                let label = format!("{variant}({})", payload.label);
                validate_value_label("enum variant template value", &value)?;
                validate_value_label("enum variant template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
            Self::Record { ty, fields } => {
                let ty_label = type_label(*ty)?;
                let mut parts = Vec::with_capacity(fields.len());
                let mut labels = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = field.value.evaluate_state_value(
                        received_payload,
                        current_state_payload,
                        type_label,
                    )?;
                    parts.push(format!("{}:{}", field.name, value.value));
                    labels.push(format!("{}:{}", field.name, value.label));
                }
                let value = format!("{ty_label}{{{}}}", parts.join(","));
                let label = format!("{ty_label}{{{}}}", labels.join(","));
                validate_value_label("record template value", &value)?;
                validate_value_label("record template label", &label)?;
                Ok(ArtifactStateValue::with_label(*ty, value, label))
            }
        }
    }

    pub(super) fn depends_on_received_payload(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => true,
            Self::CurrentStatePayload { .. } => false,
            Self::ProcessRef { .. } => false,
            Self::EnumVariant { payload, .. } => payload.depends_on_received_payload(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_received_payload()),
        }
    }

    pub(super) fn validate_for_received_payload(
        &self,
        artifact: &MantleArtifact,
        field: &str,
        expected_type: Option<TypeId>,
        received_payload_type: Option<TypeId>,
        current_state_payload_type: Option<TypeId>,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_VALUE_TEMPLATE_DEPTH {
            return Err(Error::new(format!(
                "{field} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
            )));
        }
        artifact.type_entry(self.result_type())?;
        if let Some(expected_type) = expected_type {
            if self.result_type() != expected_type {
                return Err(Error::new(format!(
                    "{field} has type id {}, expected {}",
                    self.result_type().as_u32(),
                    expected_type.as_u32()
                )));
            }
        }
        match self {
            Self::Literal { ty, value } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_value_label(field, value)
            }
            Self::ReceivedPayload { ty } => {
                let Some(received_payload_type) = received_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing transition message"
                    )));
                };
                if *ty != received_payload_type {
                    return Err(Error::new(format!(
                        "{field} has received payload type id {}, expected {}",
                        ty.as_u32(),
                        received_payload_type.as_u32()
                    )));
                }
                if expected_type.is_none()
                    && matches!(
                        artifact.type_entry(*ty)?.kind,
                        ArtifactTypeKind::ProcessRef { .. }
                    )
                {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                Ok(())
            }
            Self::CurrentStatePayload { ty } => {
                let Some(current_state_payload_type) = current_state_payload_type else {
                    return Err(Error::new(format!(
                        "{field} requires a payload-bearing current state"
                    )));
                };
                if *ty != current_state_payload_type {
                    return Err(Error::new(format!(
                        "{field} has current state payload type id {}, expected {}",
                        ty.as_u32(),
                        current_state_payload_type.as_u32()
                    )));
                }
                Ok(())
            }
            Self::ProcessRef {
                ty, target_process, ..
            } => {
                if expected_type.is_none() {
                    return Err(Error::new(format!(
                        "{field} process reference template must be a direct message payload"
                    )));
                }
                artifact.validate_process_ref_type_id_target(
                    &format!("{field}.type_id"),
                    *ty,
                    *target_process,
                )
            }
            Self::EnumVariant {
                ty,
                variant,
                payload,
            } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_ident_field(&format!("{field}.variant"), variant)?;
                payload.validate_for_received_payload(
                    artifact,
                    &format!("{field}.payload"),
                    None,
                    received_payload_type,
                    current_state_payload_type,
                    depth + 1,
                )
            }
            Self::Record { ty, fields } => {
                artifact.validate_value_type(&format!("{field}.type_id"), *ty)?;
                validate_count(
                    &format!("{field}.field_count"),
                    fields.len(),
                    1,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?;
                let mut seen = BTreeSet::new();
                for record_field in fields {
                    validate_ident_field(&format!("{field}.field"), &record_field.name)?;
                    if !seen.insert(record_field.name.as_str()) {
                        return Err(Error::new(format!(
                            "{field} duplicates field {}",
                            record_field.name
                        )));
                    }
                    record_field.value.validate_for_received_payload(
                        artifact,
                        &format!("{field}.field.{}", record_field.name),
                        None,
                        received_payload_type,
                        current_state_payload_type,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactValueTemplateField {
    pub name: String,
    pub value: ArtifactValueTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub ty: TypeId,
    pub value: String,
    pub process_ref: Option<ArtifactProcessRefPayload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProcessRefPayload {
    pub target_process: ProcessId,
    pub pid: u64,
}
