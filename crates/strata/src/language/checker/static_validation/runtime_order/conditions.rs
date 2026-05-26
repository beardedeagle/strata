use std::collections::BTreeMap;

use mantle_artifact::ArtifactValue;

use super::effect_outcomes::StaticEffectOutcomeBinding;
use super::templates::evaluate_checked_runtime_template;
use super::{StaticLoopElementBinding, StaticProcessId};
use crate::language::checked::{
    CheckedPayloadValue, CheckedProcess, CheckedProcessRefId, CheckedValueTemplate,
};
use crate::language::diagnostic::{Error, Result};

pub(super) fn evaluate_checked_bool_condition(
    condition: &CheckedValueTemplate,
    received_payload: Option<&CheckedPayloadValue>,
    current_state_payload: Option<&CheckedPayloadValue>,
    process: &CheckedProcess,
    process_refs: &BTreeMap<CheckedProcessRefId, StaticProcessId>,
    loop_elements: &[StaticLoopElementBinding],
    effect_outcomes: &[StaticEffectOutcomeBinding],
) -> Result<bool> {
    let value = evaluate_checked_runtime_template(
        condition,
        received_payload,
        current_state_payload,
        process,
        process_refs,
        loop_elements,
        effect_outcomes,
    )?;
    let value = value.value().ok_or_else(|| {
        Error::new(format!(
            "process {} if condition produced a process reference payload",
            process.debug_name()
        ))
    })?;
    let ArtifactValue::Atom(label) = value else {
        return Err(Error::new(format!(
            "process {} if condition produced non-Bool value {}",
            process.debug_name(),
            value.label()
        )));
    };
    match label.as_str() {
        "True" => Ok(true),
        "False" => Ok(false),
        _ => Err(Error::new(format!(
            "process {} if condition produced invalid Bool value {}",
            process.debug_name(),
            label
        ))),
    }
}
