pub(super) use std::collections::{BTreeMap, VecDeque};

pub(super) use super::super::runtime_order::*;
pub(super) use super::super::templates::validate_value_template_payload_labels;
pub(super) use super::super::validate_action_references;
pub(super) use crate::language::STATIC_RUNTIME_PROCESS_LIMIT;
pub(super) use crate::language::ast::{Effect, Identifier};
pub(super) use crate::language::checked::{
    CheckedAction, CheckedEnumVariant, CheckedEnumVariantId, CheckedLoopElement,
    CheckedLoopElementId, CheckedMessageCase, CheckedMessageId, CheckedMessageVariantId,
    CheckedNextState, CheckedOutputId, CheckedPayloadValue, CheckedProcess, CheckedProcessId,
    CheckedProcessParts, CheckedProcessRef, CheckedProcessRefId, CheckedSendTarget, CheckedStateId,
    CheckedStateValue, CheckedStepResult, CheckedTransition, CheckedTransitionParts,
    CheckedTypeKind, CheckedTypeRef, CheckedValueShape, CheckedValueTemplate,
    CheckedValueTemplateField, CheckedValueTemplateMapEntry,
};
pub(super) use mantle_artifact::ArtifactValue;

pub(super) fn checked_process_with_declared_refs(process_ref_count: usize) -> CheckedProcess {
    CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: value_type("MainState"),
        state_values: checked_state_values("MainState", &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: (0..process_ref_count)
            .map(|index| {
                CheckedProcessRef::new(ident(&format!("worker_{index}")), checked_process_id(1))
            })
            .collect(),
        mailbox_bound: 1,
        init_state: CheckedStateId::from_index(0).expect("valid checked state id"),
        transitions: Vec::new(),
    })
}

pub(super) fn ident(value: &str) -> Identifier {
    Identifier::new(value).expect("test identifier should be valid")
}

pub(super) fn value_type(label: &str) -> CheckedTypeRef {
    CheckedTypeRef::test_value(label)
}

pub(super) fn enum_value_type(label: &str, variants: &[&str]) -> CheckedTypeRef {
    CheckedTypeRef::test_enum_value(label, variants)
}

pub(super) fn enum_value_type_with_payloads(
    label: &str,
    variants: &[(&str, Option<CheckedTypeRef>)],
) -> CheckedTypeRef {
    let variant_entries = variants
        .iter()
        .map(|(name, payload_type)| CheckedEnumVariant {
            name: ident(name),
            payload_type: payload_type.as_ref().map(CheckedTypeRef::id),
        })
        .collect();
    let id = value_type(label).id();
    CheckedTypeRef::new(
        id,
        label.to_string(),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum {
                variants: variant_entries,
            },
        },
    )
}

pub(super) fn process_ref_type(target: &str) -> CheckedTypeRef {
    let target_process = match target {
        "Worker" => checked_process_id(1),
        other => panic!("test process ref target {other} is not mapped"),
    };
    CheckedTypeRef::test_process_ref(
        &format!("__strata_checked_process_ref_{}", target_process.as_u32()),
        target_process,
    )
}

pub(super) fn checked_state_values(ty: &str, values: &[&str]) -> Vec<CheckedStateValue> {
    values
        .iter()
        .map(|value| CheckedStateValue::new(value_type(ty), artifact_value(value)))
        .collect()
}

pub(super) fn checked_state_values_for_type(
    ty: CheckedTypeRef,
    values: &[&str],
) -> Vec<CheckedStateValue> {
    values
        .iter()
        .map(|value| CheckedStateValue::new(ty.clone(), artifact_value(value)))
        .collect()
}

pub(super) fn artifact_value(value: &str) -> ArtifactValue {
    ArtifactValue::parse(value).expect("test artifact value should be valid")
}

pub(super) fn checked_process_id(index: usize) -> CheckedProcessId {
    CheckedProcessId::from_index(index).expect("valid checked process id")
}

pub(super) fn checked_process_ref_id(index: usize) -> CheckedProcessRefId {
    CheckedProcessRefId::from_index(index).expect("valid checked process reference id")
}

pub(super) fn checked_state_id(index: usize) -> CheckedStateId {
    CheckedStateId::from_index(index).expect("valid checked state id")
}

pub(super) fn checked_message_id(index: usize) -> CheckedMessageId {
    CheckedMessageId::from_index(index).expect("valid checked message id")
}

pub(super) fn checked_output_id(index: usize) -> CheckedOutputId {
    CheckedOutputId::from_index(index).expect("valid checked output id")
}

pub(super) fn checked_enum_variant_id(index: usize) -> CheckedEnumVariantId {
    CheckedEnumVariantId::from_index(index).expect("valid checked enum variant id")
}
