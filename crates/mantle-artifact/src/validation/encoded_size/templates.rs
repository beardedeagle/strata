use super::*;
use crate::ArtifactValueTemplate;

pub(super) fn add_value_template_bytes(
    total: &mut usize,
    prefix: KeyLen,
    template: &ArtifactValueTemplate,
) -> Result<()> {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => {
            add_field_bytes(total, prefix.child("kind"), "literal")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_value_label_len(total, prefix.child("value"), value)?;
        }
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            add_field_bytes(total, prefix.child("kind"), "received_payload")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
        }
        ArtifactValueTemplate::CurrentStatePayload { ty } => {
            add_field_bytes(total, prefix.child("kind"), "current_state_payload")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
        }
        ArtifactValueTemplate::EffectOutcome { ty, outcome } => {
            add_field_bytes(total, prefix.child("kind"), "effect_outcome")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("outcome"), outcome.as_u32())?;
        }
        ArtifactValueTemplate::EnumPayload { ty, value, variant } => {
            add_field_bytes(total, prefix.child("kind"), "enum_payload")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("variant_id"), variant.as_u32())?;
            add_value_template_bytes(total, prefix.child("value"), value)?;
        }
        ArtifactValueTemplate::RecordField { ty, record, field } => {
            add_field_bytes(total, prefix.child("kind"), "record_field")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_bytes(total, prefix.child("field_name"), field)?;
            add_value_template_bytes(total, prefix.child("record"), record)?;
        }
        ArtifactValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            add_field_bytes(total, prefix.child("kind"), "list_element")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("index"), *index)?;
            add_field_usize(total, prefix.child("len"), *len)?;
            add_value_template_bytes(total, prefix.child("list"), list)?;
        }
        ArtifactValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            add_field_bytes(total, prefix.child("kind"), "list_prefix_element")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("index"), *index)?;
            add_field_usize(total, prefix.child("prefix_len"), *prefix_len)?;
            add_value_template_bytes(total, prefix.child("list"), list)?;
        }
        ArtifactValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            add_field_bytes(total, prefix.child("kind"), "list_rest")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("prefix_len"), *prefix_len)?;
            add_value_template_bytes(total, prefix.child("list"), list)?;
        }
        ArtifactValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            add_field_bytes(total, prefix.child("kind"), "map_value")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_value_label_len(total, prefix.child("key"), key)?;
            add_field_bytes(total, prefix.child("projection"), projection.as_str())?;
            add_field_usize(total, prefix.child("key_count"), keys.len())?;
            for (key_index, expected_key) in keys.iter().enumerate() {
                add_field_value_label_len(
                    total,
                    prefix.indexed_child("expected_key", key_index),
                    expected_key,
                )?;
            }
            add_value_template_bytes(total, prefix.child("map"), map)?;
        }
        ArtifactValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            add_field_bytes(total, prefix.child("kind"), "map_rest")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("key_count"), excluded_keys.len())?;
            for (key_index, excluded_key) in excluded_keys.iter().enumerate() {
                add_field_value_label_len(
                    total,
                    prefix.indexed_child("excluded_key", key_index),
                    excluded_key,
                )?;
            }
            add_value_template_bytes(total, prefix.child("map"), map)?;
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            add_field_bytes(total, prefix.child("kind"), "process_ref")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(
                total,
                prefix.child("target_process"),
                target_process.as_u32(),
            )?;
            add_field_u32(total, prefix.child("process_ref"), process_ref.as_u32())?;
        }
        ArtifactValueTemplate::LoopElement { ty, element } => {
            add_field_bytes(total, prefix.child("kind"), "loop_element")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("loop_element"), element.as_u32())?;
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            add_field_bytes(total, prefix.child("kind"), "enum_variant")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("variant_id"), variant.as_u32())?;
            add_value_template_bytes(total, prefix.child("payload"), payload)?;
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            add_field_bytes(total, prefix.child("kind"), "record")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("field_count"), fields.len())?;
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = prefix.indexed_child("field", field_index);
                add_field_bytes(total, field_prefix.child("name"), &field.name)?;
                add_value_template_bytes(total, field_prefix.child("value"), &field.value)?;
            }
        }
        ArtifactValueTemplate::List { ty, items } => {
            add_field_bytes(total, prefix.child("kind"), "list")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("item_count"), items.len())?;
            for (item_index, item) in items.iter().enumerate() {
                add_value_template_bytes(total, prefix.indexed_child("item", item_index), item)?;
            }
        }
        ArtifactValueTemplate::Map { ty, entries } => {
            add_field_bytes(total, prefix.child("kind"), "map")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_usize(total, prefix.child("entry_count"), entries.len())?;
            for (entry_index, entry) in entries.iter().enumerate() {
                let entry_prefix = prefix.indexed_child("entry", entry_index);
                add_value_template_bytes(total, entry_prefix.child("key"), &entry.key)?;
                add_value_template_bytes(total, entry_prefix.child("value"), &entry.value)?;
            }
        }
        ArtifactValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            add_field_bytes(total, prefix.child("kind"), "if_else")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_value_template_bytes(total, prefix.child("condition"), condition)?;
            add_value_template_bytes(total, prefix.child("then"), then_value)?;
            add_value_template_bytes(total, prefix.child("else"), else_value)?;
        }
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, prefix.child("kind"), "equality")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("operand_type_id"), operand_ty.as_u32())?;
            add_field_bytes(total, prefix.child("operator"), operator.as_str())?;
            add_value_template_bytes(total, prefix.child("left"), left)?;
            add_value_template_bytes(total, prefix.child("right"), right)?;
        }
        ArtifactValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, prefix.child("kind"), "scalar_arithmetic")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_bytes(total, prefix.child("operator"), operator.as_str())?;
            add_value_template_bytes(total, prefix.child("left"), left)?;
            add_value_template_bytes(total, prefix.child("right"), right)?;
        }
        ArtifactValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, prefix.child("kind"), "scalar_ordering")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_u32(total, prefix.child("operand_type_id"), operand_ty.as_u32())?;
            add_field_bytes(total, prefix.child("operator"), operator.as_str())?;
            add_value_template_bytes(total, prefix.child("left"), left)?;
            add_value_template_bytes(total, prefix.child("right"), right)?;
        }
        ArtifactValueTemplate::BooleanNot { ty, operand } => {
            add_field_bytes(total, prefix.child("kind"), "boolean_not")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_value_template_bytes(total, prefix.child("operand"), operand)?;
        }
        ArtifactValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            add_field_bytes(total, prefix.child("kind"), "boolean_binary")?;
            add_field_u32(total, prefix.child("type_id"), ty.as_u32())?;
            add_field_bytes(total, prefix.child("operator"), operator.as_str())?;
            add_value_template_bytes(total, prefix.child("left"), left)?;
            add_value_template_bytes(total, prefix.child("right"), right)?;
        }
    }
    Ok(())
}
