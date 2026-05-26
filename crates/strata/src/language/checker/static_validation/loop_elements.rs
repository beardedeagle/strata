use super::*;

pub(super) fn validate_value_template_loop_elements(
    template: &CheckedValueTemplate,
    active_loop_elements: &[ActiveCheckedLoopElement],
) -> Result<()> {
    match template {
        CheckedValueTemplate::Literal(_)
        | CheckedValueTemplate::ReceivedPayload { .. }
        | CheckedValueTemplate::CurrentStatePayload { .. }
        | CheckedValueTemplate::ProcessRef { .. }
        | CheckedValueTemplate::EffectOutcome { .. } => Ok(()),
        CheckedValueTemplate::LoopElement { ty, element } => {
            let Some(active) = active_loop_elements
                .iter()
                .find(|active| active.id == *element)
            else {
                return Err(Error::new(format!(
                    "references inactive loop element id {}",
                    element.as_u32()
                )));
            };
            if active.ty != *ty {
                return Err(Error::new(format!(
                    "loop element id {} has type {}, expected {}",
                    element.as_u32(),
                    active.ty,
                    ty
                )));
            }
            Ok(())
        }
        CheckedValueTemplate::EnumPayload { value, .. } => {
            validate_value_template_loop_elements(value, active_loop_elements)
        }
        CheckedValueTemplate::RecordField { record, .. } => {
            validate_value_template_loop_elements(record, active_loop_elements)
        }
        CheckedValueTemplate::ListElement { list, .. }
        | CheckedValueTemplate::ListPrefixElement { list, .. }
        | CheckedValueTemplate::ListRest { list, .. } => {
            validate_value_template_loop_elements(list, active_loop_elements)
        }
        CheckedValueTemplate::MapValue { map, .. } | CheckedValueTemplate::MapRest { map, .. } => {
            validate_value_template_loop_elements(map, active_loop_elements)
        }
        CheckedValueTemplate::Equality { left, right, .. }
        | CheckedValueTemplate::ScalarArithmetic { left, right, .. }
        | CheckedValueTemplate::ScalarOrdering { left, right, .. }
        | CheckedValueTemplate::BooleanBinary { left, right, .. } => {
            validate_value_template_loop_elements(left, active_loop_elements)?;
            validate_value_template_loop_elements(right, active_loop_elements)
        }
        CheckedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            validate_value_template_loop_elements(condition, active_loop_elements)?;
            validate_value_template_loop_elements(then_value, active_loop_elements)?;
            validate_value_template_loop_elements(else_value, active_loop_elements)
        }
        CheckedValueTemplate::BooleanNot { operand, .. } => {
            validate_value_template_loop_elements(operand, active_loop_elements)
        }
        CheckedValueTemplate::EnumVariant { payload, .. } => {
            validate_value_template_loop_elements(payload, active_loop_elements)
        }
        CheckedValueTemplate::Record { fields, .. } => {
            for field in fields {
                validate_value_template_loop_elements(field.value(), active_loop_elements)?;
            }
            Ok(())
        }
        CheckedValueTemplate::List { items, .. } => {
            for item in items {
                validate_value_template_loop_elements(item, active_loop_elements)?;
            }
            Ok(())
        }
        CheckedValueTemplate::Map { entries, .. } => {
            for entry in entries {
                validate_value_template_loop_elements(entry.key(), active_loop_elements)?;
                validate_value_template_loop_elements(entry.value(), active_loop_elements)?;
            }
            Ok(())
        }
    }
}
