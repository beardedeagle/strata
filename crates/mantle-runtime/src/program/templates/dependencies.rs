use super::support::*;

pub(in crate::program) fn loaded_template_depends_on_received_payload(
    template: &LoadedValueTemplate,
) -> bool {
    match template {
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. }
        | LoadedValueTemplate::EffectOutcome { .. } => false,
        LoadedValueTemplate::ReceivedPayload { .. } => true,
        LoadedValueTemplate::CurrentStatePayload { .. } => false,
        LoadedValueTemplate::EnumPayload { value, .. } => {
            loaded_template_depends_on_received_payload(value)
        }
        LoadedValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_received_payload(record)
        }
        LoadedValueTemplate::ListElement { list, .. }
        | LoadedValueTemplate::ListPrefixElement { list, .. }
        | LoadedValueTemplate::ListRest { list, .. } => {
            loaded_template_depends_on_received_payload(list)
        }
        LoadedValueTemplate::MapValue { map, .. } => {
            loaded_template_depends_on_received_payload(map)
        }
        LoadedValueTemplate::MapRest { map, .. } => {
            loaded_template_depends_on_received_payload(map)
        }
        LoadedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            loaded_template_depends_on_received_payload(condition)
                || loaded_template_depends_on_received_payload(then_value)
                || loaded_template_depends_on_received_payload(else_value)
        }
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_received_payload(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_received_payload(&field.value)),
        LoadedValueTemplate::List { items, .. } => items
            .iter()
            .any(loaded_template_depends_on_received_payload),
        LoadedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_received_payload(&entry.key)
                || loaded_template_depends_on_received_payload(&entry.value)
        }),
        LoadedValueTemplate::Equality { left, right, .. }
        | LoadedValueTemplate::ScalarArithmetic { left, right, .. }
        | LoadedValueTemplate::ScalarOrdering { left, right, .. } => {
            loaded_template_depends_on_received_payload(left)
                || loaded_template_depends_on_received_payload(right)
        }
        LoadedValueTemplate::BooleanNot { operand, .. } => {
            loaded_template_depends_on_received_payload(operand)
        }
        LoadedValueTemplate::BooleanBinary { left, right, .. } => {
            loaded_template_depends_on_received_payload(left)
                || loaded_template_depends_on_received_payload(right)
        }
    }
}

pub(in crate::program) fn loaded_template_depends_on_loop_element(
    template: &LoadedValueTemplate,
) -> bool {
    match template {
        LoadedValueTemplate::LoopElement { .. } => true,
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::EffectOutcome { .. } => false,
        LoadedValueTemplate::EnumPayload { value, .. } => {
            loaded_template_depends_on_loop_element(value)
        }
        LoadedValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_loop_element(record)
        }
        LoadedValueTemplate::ListElement { list, .. }
        | LoadedValueTemplate::ListPrefixElement { list, .. }
        | LoadedValueTemplate::ListRest { list, .. } => {
            loaded_template_depends_on_loop_element(list)
        }
        LoadedValueTemplate::MapValue { map, .. } => loaded_template_depends_on_loop_element(map),
        LoadedValueTemplate::MapRest { map, .. } => loaded_template_depends_on_loop_element(map),
        LoadedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            loaded_template_depends_on_loop_element(condition)
                || loaded_template_depends_on_loop_element(then_value)
                || loaded_template_depends_on_loop_element(else_value)
        }
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_loop_element(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_loop_element(&field.value)),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().any(loaded_template_depends_on_loop_element)
        }
        LoadedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_loop_element(&entry.key)
                || loaded_template_depends_on_loop_element(&entry.value)
        }),
        LoadedValueTemplate::Equality { left, right, .. }
        | LoadedValueTemplate::ScalarArithmetic { left, right, .. }
        | LoadedValueTemplate::ScalarOrdering { left, right, .. } => {
            loaded_template_depends_on_loop_element(left)
                || loaded_template_depends_on_loop_element(right)
        }
        LoadedValueTemplate::BooleanNot { operand, .. } => {
            loaded_template_depends_on_loop_element(operand)
        }
        LoadedValueTemplate::BooleanBinary { left, right, .. } => {
            loaded_template_depends_on_loop_element(left)
                || loaded_template_depends_on_loop_element(right)
        }
    }
}

pub(in crate::program) fn loaded_template_depends_on_effect_outcome(
    template: &LoadedValueTemplate,
) -> bool {
    match template {
        LoadedValueTemplate::EffectOutcome { .. } => true,
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. } => false,
        LoadedValueTemplate::EnumPayload { value, .. } => {
            loaded_template_depends_on_effect_outcome(value)
        }
        LoadedValueTemplate::RecordField { record, .. } => {
            loaded_template_depends_on_effect_outcome(record)
        }
        LoadedValueTemplate::ListElement { list, .. }
        | LoadedValueTemplate::ListPrefixElement { list, .. }
        | LoadedValueTemplate::ListRest { list, .. } => {
            loaded_template_depends_on_effect_outcome(list)
        }
        LoadedValueTemplate::MapValue { map, .. } => loaded_template_depends_on_effect_outcome(map),
        LoadedValueTemplate::MapRest { map, .. } => loaded_template_depends_on_effect_outcome(map),
        LoadedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            loaded_template_depends_on_effect_outcome(condition)
                || loaded_template_depends_on_effect_outcome(then_value)
                || loaded_template_depends_on_effect_outcome(else_value)
        }
        LoadedValueTemplate::EnumVariant { payload, .. } => {
            loaded_template_depends_on_effect_outcome(payload)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| loaded_template_depends_on_effect_outcome(&field.value)),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().any(loaded_template_depends_on_effect_outcome)
        }
        LoadedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            loaded_template_depends_on_effect_outcome(&entry.key)
                || loaded_template_depends_on_effect_outcome(&entry.value)
        }),
        LoadedValueTemplate::Equality { left, right, .. }
        | LoadedValueTemplate::ScalarArithmetic { left, right, .. }
        | LoadedValueTemplate::ScalarOrdering { left, right, .. }
        | LoadedValueTemplate::BooleanBinary { left, right, .. } => {
            loaded_template_depends_on_effect_outcome(left)
                || loaded_template_depends_on_effect_outcome(right)
        }
        LoadedValueTemplate::BooleanNot { operand, .. } => {
            loaded_template_depends_on_effect_outcome(operand)
        }
    }
}
