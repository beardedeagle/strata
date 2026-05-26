use super::super::evaluation::evaluate_loaded_state_value;
use super::*;

pub(super) fn loaded_static_map_key_value(
    program: &LoadedProgram,
    template: &LoadedValueTemplate,
) -> Result<RuntimeValue> {
    evaluate_loaded_state_value(program, template, None, None).map(|value| value.value)
}

pub(super) fn loaded_template_is_static_map_key(template: &LoadedValueTemplate) -> bool {
    match template {
        LoadedValueTemplate::Literal { .. } => true,
        LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::EnumPayload { .. }
        | LoadedValueTemplate::RecordField { .. }
        | LoadedValueTemplate::ListElement { .. }
        | LoadedValueTemplate::ListPrefixElement { .. }
        | LoadedValueTemplate::ListRest { .. }
        | LoadedValueTemplate::MapValue { .. }
        | LoadedValueTemplate::MapRest { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. }
        | LoadedValueTemplate::EffectOutcome { .. }
        | LoadedValueTemplate::IfElse { .. }
        | LoadedValueTemplate::Equality { .. }
        | LoadedValueTemplate::BooleanNot { .. }
        | LoadedValueTemplate::BooleanBinary { .. } => false,
        LoadedValueTemplate::ScalarArithmetic { left, right, .. }
        | LoadedValueTemplate::ScalarOrdering { left, right, .. } => {
            loaded_template_is_static_map_key(left) && loaded_template_is_static_map_key(right)
        }
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_is_static_map_key(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .all(|field| loaded_template_is_static_map_key(&field.value)),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().all(loaded_template_is_static_map_key)
        }
        LoadedValueTemplate::Map { entries, .. } => entries.iter().all(|entry| {
            loaded_template_is_static_map_key(&entry.key)
                && loaded_template_is_static_map_key(&entry.value)
        }),
    }
}
