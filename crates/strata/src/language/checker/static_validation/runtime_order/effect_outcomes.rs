use mantle_artifact::{ArtifactValue, TypeId};

use super::StaticActionState;
use super::templates::checked_payload_value;
use crate::language::checked::{
    CheckedEffectOutcomeId, CheckedMessageId, CheckedPayloadValue, CheckedProcess, CheckedTypeId,
    CheckedTypeKind, CheckedTypeRef, CheckedValueShape,
};
use crate::language::diagnostic::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StaticEffectOutcomeBinding {
    pub(super) id: CheckedEffectOutcomeId,
    pub(super) value: CheckedPayloadValue,
}

pub(super) fn bind_static_effect_outcome(
    process: &CheckedProcess,
    state: &mut StaticActionState<'_>,
    id: CheckedEffectOutcomeId,
    outcome_ty: &CheckedTypeRef,
    value: CheckedPayloadValue,
) -> Result<()> {
    if value.ty() != outcome_ty {
        return Err(Error::new(format!(
            "process {} effect outcome id {} has type {}, expected {}",
            process.debug_name(),
            id.as_u32(),
            value.ty(),
            outcome_ty
        )));
    }
    if state.effect_outcomes.iter().any(|binding| binding.id == id) {
        return Err(Error::new(format!(
            "process {} effect outcome id {} is bound more than once",
            process.debug_name(),
            id.as_u32()
        )));
    }
    state
        .effect_outcomes
        .push(StaticEffectOutcomeBinding { id, value });
    Ok(())
}

pub(super) fn ok_unit_outcome(outcome_ty: &CheckedTypeRef) -> Result<CheckedPayloadValue> {
    result_outcome(outcome_ty, "Ok", ArtifactValue::Atom("Unit".to_string()))
}

pub(super) fn ok_process_ref_outcome(
    outcome_ty: &CheckedTypeRef,
    pid: u64,
) -> Result<CheckedPayloadValue> {
    let ok_ty = result_variant_payload_type(outcome_ty, "Ok")?;
    result_outcome(
        outcome_ty,
        "Ok",
        ArtifactValue::process_ref(TypeId::new(ok_ty.as_u32()), pid),
    )
}

pub(super) fn send_error_outcome(
    outcome_ty: &CheckedTypeRef,
    error_variant: &str,
    original_message: ArtifactValue,
) -> Result<CheckedPayloadValue> {
    result_outcome(
        outcome_ty,
        "Err",
        ArtifactValue::EnumVariant {
            variant: error_variant.to_string(),
            payload: Box::new(original_message),
        },
    )
}

pub(super) fn spawn_error_outcome(
    outcome_ty: &CheckedTypeRef,
    error_variant: &str,
) -> Result<CheckedPayloadValue> {
    result_outcome(
        outcome_ty,
        "Err",
        ArtifactValue::EnumVariant {
            variant: error_variant.to_string(),
            payload: Box::new(ArtifactValue::Atom("Unit".to_string())),
        },
    )
}

pub(super) fn static_original_message(
    process: &CheckedProcess,
    message: CheckedMessageId,
    payload: Option<&CheckedPayloadValue>,
) -> Result<ArtifactValue> {
    let message_case = process
        .message_cases()
        .get(message.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} does not accept message id {}",
                process.debug_name(),
                message.as_u32()
            ))
        })?;
    match payload {
        Some(payload) => Ok(ArtifactValue::EnumVariant {
            variant: message_case.label().to_string(),
            payload: Box::new(checked_payload_value(payload)?),
        }),
        None => Ok(ArtifactValue::Atom(message_case.label().to_string())),
    }
}

fn result_outcome(
    outcome_ty: &CheckedTypeRef,
    variant: &str,
    payload: ArtifactValue,
) -> Result<CheckedPayloadValue> {
    ensure_payload_variant(outcome_ty, variant)?;
    Ok(CheckedPayloadValue::new(
        outcome_ty.clone(),
        ArtifactValue::EnumVariant {
            variant: variant.to_string(),
            payload: Box::new(payload),
        },
    ))
}

fn ensure_payload_variant(ty: &CheckedTypeRef, variant_label: &str) -> Result<()> {
    result_variant_payload_type(ty, variant_label).map(|_| ())
}

fn result_variant_payload_type(ty: &CheckedTypeRef, variant_label: &str) -> Result<CheckedTypeId> {
    let CheckedTypeKind::Value {
        shape: CheckedValueShape::Enum { variants },
    } = ty.kind()
    else {
        return Err(Error::new(format!(
            "effect outcome type {} must be an enum value type",
            ty
        )));
    };
    let variant = variants
        .iter()
        .find(|variant| variant.name.as_str() == variant_label)
        .ok_or_else(|| {
            Error::new(format!(
                "effect outcome type {} is missing variant {variant_label}",
                ty
            ))
        })?;
    variant.payload_type.ok_or_else(|| {
        Error::new(format!(
            "effect outcome type {} variant {variant_label} must carry a payload",
            ty
        ))
    })
}
