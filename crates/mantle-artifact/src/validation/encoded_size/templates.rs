use super::*;
use crate::ArtifactValueTemplate;

pub(super) fn add_value_template_bytes(
    total: &mut usize,
    prefix: &str,
    template: &ArtifactValueTemplate,
) -> Result<()> {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "literal")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.value"), &value.label())?;
        }
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "received_payload")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
        }
        ArtifactValueTemplate::CurrentStatePayload { ty } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "current_state_payload")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
        }
        ArtifactValueTemplate::EnumPayload { ty, value, variant } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "enum_payload")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.variant_id"),
                &variant.as_u32().to_string(),
            )?;
            add_value_template_bytes(total, &format!("{prefix}.value"), value)?;
        }
        ArtifactValueTemplate::RecordField { ty, record, field } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "record_field")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.field_name"), field)?;
            add_value_template_bytes(total, &format!("{prefix}.record"), record)?;
        }
        ArtifactValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "list_element")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.index"), &index.to_string())?;
            add_field_bytes(total, &format!("{prefix}.len"), &len.to_string())?;
            add_value_template_bytes(total, &format!("{prefix}.list"), list)?;
        }
        ArtifactValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "list_prefix_element")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.index"), &index.to_string())?;
            add_field_bytes(
                total,
                &format!("{prefix}.prefix_len"),
                &prefix_len.to_string(),
            )?;
            add_value_template_bytes(total, &format!("{prefix}.list"), list)?;
        }
        ArtifactValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "list_rest")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.prefix_len"),
                &prefix_len.to_string(),
            )?;
            add_value_template_bytes(total, &format!("{prefix}.list"), list)?;
        }
        ArtifactValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "map_value")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.key"), &key.label())?;
            add_field_bytes(total, &format!("{prefix}.projection"), projection.as_str())?;
            add_field_bytes(
                total,
                &format!("{prefix}.key_count"),
                &keys.len().to_string(),
            )?;
            for (key_index, expected_key) in keys.iter().enumerate() {
                add_field_bytes(
                    total,
                    &format!("{prefix}.expected_key.{key_index}"),
                    &expected_key.label(),
                )?;
            }
            add_value_template_bytes(total, &format!("{prefix}.map"), map)?;
        }
        ArtifactValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "map_rest")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.key_count"),
                &excluded_keys.len().to_string(),
            )?;
            for (key_index, excluded_key) in excluded_keys.iter().enumerate() {
                add_field_bytes(
                    total,
                    &format!("{prefix}.excluded_key.{key_index}"),
                    &excluded_key.label(),
                )?;
            }
            add_value_template_bytes(total, &format!("{prefix}.map"), map)?;
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "process_ref")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.target_process"),
                &target_process.as_u32().to_string(),
            )?;
            add_field_bytes(
                total,
                &format!("{prefix}.process_ref"),
                &process_ref.as_u32().to_string(),
            )?;
        }
        ArtifactValueTemplate::LoopElement { ty, element } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "loop_element")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.loop_element"),
                &element.as_u32().to_string(),
            )?;
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "enum_variant")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.variant_id"),
                &variant.as_u32().to_string(),
            )?;
            add_value_template_bytes(total, &format!("{prefix}.payload"), payload)?;
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "record")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.field_count"),
                &fields.len().to_string(),
            )?;
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = format!("{prefix}.field.{field_index}");
                add_field_bytes(total, &format!("{field_prefix}.name"), &field.name)?;
                add_value_template_bytes(total, &format!("{field_prefix}.value"), &field.value)?;
            }
        }
        ArtifactValueTemplate::List { ty, items } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "list")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.item_count"),
                &items.len().to_string(),
            )?;
            for (item_index, item) in items.iter().enumerate() {
                add_value_template_bytes(total, &format!("{prefix}.item.{item_index}"), item)?;
            }
        }
        ArtifactValueTemplate::Map { ty, entries } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "map")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.entry_count"),
                &entries.len().to_string(),
            )?;
            for (entry_index, entry) in entries.iter().enumerate() {
                let entry_prefix = format!("{prefix}.entry.{entry_index}");
                add_value_template_bytes(total, &format!("{entry_prefix}.key"), &entry.key)?;
                add_value_template_bytes(total, &format!("{entry_prefix}.value"), &entry.value)?;
            }
        }
        ArtifactValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "if_else")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_value_template_bytes(total, &format!("{prefix}.condition"), condition)?;
            add_value_template_bytes(total, &format!("{prefix}.then"), then_value)?;
            add_value_template_bytes(total, &format!("{prefix}.else"), else_value)?;
        }
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "equality")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.operand_type_id"),
                &type_id_string(*operand_ty),
            )?;
            add_field_bytes(total, &format!("{prefix}.operator"), operator.as_str())?;
            add_value_template_bytes(total, &format!("{prefix}.left"), left)?;
            add_value_template_bytes(total, &format!("{prefix}.right"), right)?;
        }
        ArtifactValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "scalar_arithmetic")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.operator"), operator.as_str())?;
            add_value_template_bytes(total, &format!("{prefix}.left"), left)?;
            add_value_template_bytes(total, &format!("{prefix}.right"), right)?;
        }
        ArtifactValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "scalar_ordering")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(
                total,
                &format!("{prefix}.operand_type_id"),
                &type_id_string(*operand_ty),
            )?;
            add_field_bytes(total, &format!("{prefix}.operator"), operator.as_str())?;
            add_value_template_bytes(total, &format!("{prefix}.left"), left)?;
            add_value_template_bytes(total, &format!("{prefix}.right"), right)?;
        }
        ArtifactValueTemplate::BooleanNot { ty, operand } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "boolean_not")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_value_template_bytes(total, &format!("{prefix}.operand"), operand)?;
        }
        ArtifactValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, &format!("{prefix}.kind"), "boolean_binary")?;
            add_field_bytes(total, &format!("{prefix}.type_id"), &type_id_string(*ty))?;
            add_field_bytes(total, &format!("{prefix}.operator"), operator.as_str())?;
            add_value_template_bytes(total, &format!("{prefix}.left"), left)?;
            add_value_template_bytes(total, &format!("{prefix}.right"), right)?;
        }
    }
    Ok(())
}
