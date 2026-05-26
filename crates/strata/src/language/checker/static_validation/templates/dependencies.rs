use super::*;

pub(in crate::language::checker::static_validation) fn checked_template_depends_on_received_payload(
    template: &CheckedValueTemplate,
) -> bool {
    match template {
        CheckedValueTemplate::Literal(_) => false,
        CheckedValueTemplate::ReceivedPayload { .. } => true,
        CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::LoopElement { .. }
        | CheckedValueTemplate::EffectOutcome { .. } => false,
        CheckedValueTemplate::EnumPayload { value, .. } => {
            checked_template_depends_on_received_payload(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_received_payload(record)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            checked_template_depends_on_received_payload(list)
        }
        CheckedValueTemplate::MapValue { map, .. } | CheckedValueTemplate::MapRest { map, .. } => {
            checked_template_depends_on_received_payload(map)
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_received_payload(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_received_payload(field.value())),
        CheckedValueTemplate::List { items, .. } => items
            .iter()
            .any(checked_template_depends_on_received_payload),
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_received_payload(entry.key())
                || checked_template_depends_on_received_payload(entry.value())
        }),
        CheckedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            checked_template_depends_on_received_payload(condition)
                || checked_template_depends_on_received_payload(then_value)
                || checked_template_depends_on_received_payload(else_value)
        }
        CheckedValueTemplate::Equality { left, right, .. }
        | CheckedValueTemplate::ScalarArithmetic { left, right, .. }
        | CheckedValueTemplate::ScalarOrdering { left, right, .. }
        | CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            checked_template_depends_on_received_payload(left)
                || checked_template_depends_on_received_payload(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            checked_template_depends_on_received_payload(operand)
        }
    }
}

pub(in crate::language::checker::static_validation) fn checked_template_depends_on_loop_element(
    template: &CheckedValueTemplate,
) -> bool {
    match template {
        CheckedValueTemplate::LoopElement { .. } => true,
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::EffectOutcome { .. } => false,
        CheckedValueTemplate::EnumPayload { value, .. } => {
            checked_template_depends_on_loop_element(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_loop_element(record)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            checked_template_depends_on_loop_element(list)
        }
        CheckedValueTemplate::MapValue { map, .. } | CheckedValueTemplate::MapRest { map, .. } => {
            checked_template_depends_on_loop_element(map)
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_loop_element(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_loop_element(field.value())),
        CheckedValueTemplate::List { items, .. } => {
            items.iter().any(checked_template_depends_on_loop_element)
        }
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_loop_element(entry.key())
                || checked_template_depends_on_loop_element(entry.value())
        }),
        CheckedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            checked_template_depends_on_loop_element(condition)
                || checked_template_depends_on_loop_element(then_value)
                || checked_template_depends_on_loop_element(else_value)
        }
        CheckedValueTemplate::Equality { left, right, .. }
        | CheckedValueTemplate::ScalarArithmetic { left, right, .. }
        | CheckedValueTemplate::ScalarOrdering { left, right, .. }
        | CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            checked_template_depends_on_loop_element(left)
                || checked_template_depends_on_loop_element(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            checked_template_depends_on_loop_element(operand)
        }
    }
}

pub(in crate::language::checker::static_validation) fn checked_template_depends_on_effect_outcome(
    template: &CheckedValueTemplate,
) -> bool {
    match template {
        CheckedValueTemplate::EffectOutcome { .. } => true,
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::LoopElement { .. } => false,
        CheckedValueTemplate::EnumPayload { value, .. } => {
            checked_template_depends_on_effect_outcome(value)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            checked_template_depends_on_effect_outcome(record)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            checked_template_depends_on_effect_outcome(list)
        }
        CheckedValueTemplate::MapValue { map, .. } | CheckedValueTemplate::MapRest { map, .. } => {
            checked_template_depends_on_effect_outcome(map)
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            checked_template_depends_on_effect_outcome(payload)
        }
        CheckedValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| checked_template_depends_on_effect_outcome(field.value())),
        CheckedValueTemplate::List { items, .. } => {
            items.iter().any(checked_template_depends_on_effect_outcome)
        }
        CheckedValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            checked_template_depends_on_effect_outcome(entry.key())
                || checked_template_depends_on_effect_outcome(entry.value())
        }),
        CheckedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            checked_template_depends_on_effect_outcome(condition)
                || checked_template_depends_on_effect_outcome(then_value)
                || checked_template_depends_on_effect_outcome(else_value)
        }
        CheckedValueTemplate::Equality { left, right, .. }
        | CheckedValueTemplate::ScalarArithmetic { left, right, .. }
        | CheckedValueTemplate::ScalarOrdering { left, right, .. }
        | CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            checked_template_depends_on_effect_outcome(left)
                || checked_template_depends_on_effect_outcome(right)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            checked_template_depends_on_effect_outcome(operand)
        }
    }
}
