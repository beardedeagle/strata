use super::super::*;
use crate::MAX_NEXT_STATE_IF_ELSE_DEPTH;
use crate::fields::ArtifactFields;

impl MantleArtifact {
    pub fn decode(contents: &str) -> Result<Self> {
        let mut fields = ArtifactFields::parse(contents)?;
        let format = fields.take_required("format")?;
        let schema_version = fields.take_required("schema_version")?;
        validate_artifact_identity(&format, &schema_version)?;

        let process_count = fields.take_bounded_usize("process_count", 1, MAX_PROCESS_COUNT)?;
        let type_count = fields.take_bounded_usize("type_count", 1, MAX_TYPE_COUNT)?;
        let output_count = fields.take_bounded_usize("output_count", 0, MAX_OUTPUT_LITERALS)?;
        let mut types = Vec::with_capacity(type_count);
        for type_index in 0..type_count {
            types.push(decode_type(&mut fields, type_index)?);
        }
        let mut outputs = Vec::with_capacity(output_count);
        for output_index in 0..output_count {
            outputs.push(fields.take_required(&format!("output.{output_index}"))?);
        }

        let mut processes = Vec::with_capacity(process_count);
        for process_index in 0..process_count {
            let prefix = format!("process.{process_index}");
            let state_value_count = fields.take_bounded_usize(
                &format!("{prefix}.state_value_count"),
                1,
                MAX_STATE_VALUES_PER_PROCESS,
            )?;
            let mut state_values = Vec::with_capacity(state_value_count);
            for value_index in 0..state_value_count {
                state_values.push(decode_state_value(
                    &mut fields,
                    &format!("{prefix}.state_value.{value_index}"),
                )?);
            }

            let message_count = fields.take_bounded_usize(
                &format!("{prefix}.message_count"),
                1,
                MAX_MESSAGE_VARIANTS_PER_PROCESS,
            )?;
            let mut message_variants = Vec::with_capacity(message_count);
            for message_index in 0..message_count {
                message_variants.push(ArtifactMessageVariant {
                    label: fields.take_required(&format!("{prefix}.message.{message_index}"))?,
                    payload_type: fields.take_optional_type_id(&format!(
                        "{prefix}.message.{message_index}.payload_type_id"
                    ))?,
                });
            }

            let process_ref_count = fields.take_bounded_usize(
                &format!("{prefix}.process_ref_count"),
                0,
                MAX_PROCESS_REFS_PER_PROCESS,
            )?;
            let mut process_refs = Vec::with_capacity(process_ref_count);
            for process_ref_index in 0..process_ref_count {
                let process_ref_prefix = format!("{prefix}.process_ref.{process_ref_index}");
                process_refs.push(ArtifactProcessRef {
                    debug_name: fields
                        .take_required(&format!("{process_ref_prefix}.debug_name"))?,
                    target: fields
                        .take_process_id(&format!("{process_ref_prefix}.target_process"))?,
                });
            }

            let transition_count = fields.take_bounded_usize(
                &format!("{prefix}.transition_count"),
                1,
                MAX_TRANSITIONS_PER_PROCESS,
            )?;
            let mut transitions = Vec::with_capacity(transition_count);
            for transition_index in 0..transition_count {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                let effect_count = fields.take_bounded_usize(
                    &format!("{transition_prefix}.effect_count"),
                    0,
                    MAX_EFFECTS_PER_TRANSITION,
                )?;
                let mut effects = Vec::with_capacity(effect_count);
                for effect_index in 0..effect_count {
                    let key = format!("{transition_prefix}.effect.{effect_index}");
                    let effect = fields.take_required(&key)?;
                    effects.push(
                        ArtifactEffect::parse(&effect)
                            .map_err(|err| Error::new(format!("{key}: {err}")))?,
                    );
                }
                let action_count = fields.take_bounded_usize(
                    &format!("{transition_prefix}.action_count"),
                    0,
                    MAX_ACTIONS_PER_PROCESS,
                )?;
                let mut actions = Vec::with_capacity(action_count);
                for action_index in 0..action_count {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    actions.push(decode_action(&mut fields, &action_prefix, 0)?);
                }

                transitions.push(ArtifactTransition {
                    current_state: fields
                        .take_optional_state_id(&format!("{transition_prefix}.current_state"))?,
                    message: fields.take_message_id(&format!("{transition_prefix}.message"))?,
                    payload_guard: decode_transition_payload_guard(
                        &mut fields,
                        &transition_prefix,
                    )?,
                    step_result: fields
                        .take_step_result(&format!("{transition_prefix}.step_result"))?,
                    next_state: decode_next_state(&mut fields, &transition_prefix, 0)?,
                    effects,
                    actions,
                });
            }

            processes.push(ArtifactProcess {
                debug_name: fields.take_required(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_type_id(&format!("{prefix}.state_type_id"))?,
                state_values,
                message_type: fields.take_type_id(&format!("{prefix}.message_type_id"))?,
                message_variants,
                process_refs,
                mailbox_bound: fields.take_bounded_usize(
                    &format!("{prefix}.mailbox_bound"),
                    1,
                    MAX_MAILBOX_BOUND,
                )?,
                init_state: fields.take_state_id(&format!("{prefix}.init_state"))?,
                transitions,
            });
        }

        let artifact = Self {
            format,
            schema_version,
            source_language: fields.take_required("source_language")?,
            module: fields.take_required("module")?,
            entry_process: fields.take_process_id("entry_process")?,
            entry_message: fields.take_message_id("entry_message")?,
            types,
            outputs,
            processes,
            source_hash_fnv1a64: fields.take_required("source_hash_fnv1a64")?,
        };

        fields.finish()?;
        artifact.validate()?;
        Ok(artifact)
    }
}

fn decode_transition_payload_guard(
    fields: &mut ArtifactFields,
    prefix: &str,
) -> Result<Option<ArtifactPayload>> {
    let payload_type_key = format!("{prefix}.payload_guard_type_id");
    let payload_value_key = format!("{prefix}.payload_guard_value");
    let payload_type = fields.take_optional_type_id(&payload_type_key)?;
    let payload_value = fields.take_optional(&payload_value_key);
    match (payload_type, payload_value) {
        (None, None) => Ok(None),
        (Some(ty), Some(value)) => Ok(Some(ArtifactPayload::value(
            ty,
            ArtifactValue::parse_field(&payload_value_key, &value)?,
        )?)),
        (Some(_), None) => Err(Error::new(format!(
            "{prefix}.payload_guard_type_id requires {prefix}.payload_guard_value"
        ))),
        (None, Some(_)) => Err(Error::new(format!(
            "{prefix}.payload_guard_value requires {prefix}.payload_guard_type_id"
        ))),
    }
}

fn decode_state_value(fields: &mut ArtifactFields, prefix: &str) -> Result<ArtifactStateValue> {
    let payload_type_key = format!("{prefix}.payload_type_id");
    let payload_value_key = format!("{prefix}.payload_value");
    let payload_type = fields.take_optional_type_id(&payload_type_key)?;
    let payload_value = fields.take_optional(&payload_value_key);
    let payload = match (payload_type, payload_value) {
        (None, None) => None,
        (Some(ty), Some(value)) => Some(ArtifactPayload::value(
            ty,
            ArtifactValue::parse_field(&payload_value_key, &value)?,
        )?),
        (Some(_), None) => {
            return Err(Error::new(format!(
                "{prefix}.payload_type_id requires {prefix}.payload_value"
            )));
        }
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "{prefix}.payload_value requires {prefix}.payload_type_id"
            )));
        }
    };
    let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
    let value_key = format!("{prefix}.value");
    let value = ArtifactValue::parse_field(&value_key, &fields.take_required(&value_key)?)?;
    let label = fields.take_required(&format!("{prefix}.label"))?;
    let mut state_value = ArtifactStateValue::with_label(ty, value, label)?;
    state_value.payload = payload;
    Ok(state_value)
}

fn decode_type(fields: &mut ArtifactFields, type_index: usize) -> Result<ArtifactType> {
    let prefix = format!("type.{type_index}");
    let label = fields.take_required(&format!("{prefix}.label"))?;
    let kind_value = fields.take_required(&format!("{prefix}.kind"))?;
    let target = if kind_value == "process_ref" {
        Some(fields.take_process_id(&format!("{prefix}.target_process"))?)
    } else {
        None
    };
    let kind = ArtifactTypeKind::parse(&kind_value, target)?;
    let shape = if matches!(kind, ArtifactTypeKind::Value) {
        Some(decode_type_shape(fields, &prefix)?)
    } else {
        None
    };
    Ok(ArtifactType { label, kind, shape })
}

fn decode_type_shape(fields: &mut ArtifactFields, prefix: &str) -> Result<ArtifactValueShape> {
    match fields.take_required(&format!("{prefix}.shape"))?.as_str() {
        "atom" => Ok(ArtifactValueShape::Atom),
        "record" => {
            let field_count = fields.take_bounded_usize(
                &format!("{prefix}.field_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut fields_out = Vec::with_capacity(field_count);
            for field_index in 0..field_count {
                fields_out.push(ArtifactTypeField {
                    name: fields.take_required(&format!("{prefix}.field.{field_index}.name"))?,
                    ty: fields.take_type_id(&format!("{prefix}.field.{field_index}.type_id"))?,
                });
            }
            Ok(ArtifactValueShape::Record { fields: fields_out })
        }
        "enum" => {
            let variant_count = fields.take_bounded_usize(
                &format!("{prefix}.enum_variant_count"),
                1,
                MAX_ENUM_VARIANTS_PER_TYPE,
            )?;
            let mut variants = Vec::with_capacity(variant_count);
            for variant_index in 0..variant_count {
                variants.push(ArtifactEnumVariant {
                    label: fields
                        .take_required(&format!("{prefix}.enum_variant.{variant_index}"))?,
                    payload_type: fields.take_optional_type_id(&format!(
                        "{prefix}.enum_variant.{variant_index}.payload_type_id"
                    ))?,
                });
            }
            Ok(ArtifactValueShape::Enum { variants })
        }
        "list" => Ok(ArtifactValueShape::List {
            element: fields.take_type_id(&format!("{prefix}.element_type_id"))?,
            capacity: fields.take_bounded_usize(
                &format!("{prefix}.capacity"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?,
        }),
        "map" => Ok(ArtifactValueShape::Map {
            key: fields.take_type_id(&format!("{prefix}.key_type_id"))?,
            value: fields.take_type_id(&format!("{prefix}.value_type_id"))?,
            capacity: fields.take_bounded_usize(
                &format!("{prefix}.capacity"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?,
        }),
        other => Err(Error::new(format!(
            "invalid artifact value type shape {other:?}"
        ))),
    }
}

fn decode_next_state(fields: &mut ArtifactFields, prefix: &str, depth: usize) -> Result<NextState> {
    let key = format!("{prefix}.next_state");
    match fields.take_required(&key)?.as_str() {
        "current" => Ok(NextState::Current),
        "value" => Ok(NextState::Value(
            fields.take_state_id(&format!("{prefix}.next_state_value"))?,
        )),
        "template" => Ok(NextState::Template(decode_value_template(
            fields,
            &format!("{prefix}.next_state_template"),
            0,
        )?)),
        "if_else" => {
            if depth >= MAX_NEXT_STATE_IF_ELSE_DEPTH {
                return Err(Error::new(format!(
                    "{prefix}.next_state runtime if nesting exceeds maximum depth of {MAX_NEXT_STATE_IF_ELSE_DEPTH}"
                )));
            }
            let branch_depth = depth + 1;
            Ok(NextState::IfElse {
                condition: decode_value_template(
                    fields,
                    &format!("{prefix}.next_state_condition"),
                    0,
                )?,
                then_state: Box::new(decode_next_state(
                    fields,
                    &format!("{prefix}.next_state_then"),
                    branch_depth,
                )?),
                else_state: Box::new(decode_next_state(
                    fields,
                    &format!("{prefix}.next_state_else"),
                    branch_depth,
                )?),
            })
        }
        value => Err(Error::new(format!(
            "invalid {key} value {value:?}; expected \"current\", \"value\", \"template\", or \"if_else\""
        ))),
    }
}

fn decode_value_template(
    fields: &mut ArtifactFields,
    prefix: &str,
    depth: usize,
) -> Result<ArtifactValueTemplate> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "{prefix} exceeds maximum value template depth of {MAX_VALUE_TEMPLATE_DEPTH}"
        )));
    }
    let kind_key = format!("{prefix}.kind");
    match fields.take_required(&kind_key)?.as_str() {
        "literal" => {
            let value_key = format!("{prefix}.value");
            Ok(ArtifactValueTemplate::Literal {
                ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
                value: ArtifactValue::parse_field(&value_key, &fields.take_required(&value_key)?)?,
            })
        }
        "received_payload" => Ok(ArtifactValueTemplate::ReceivedPayload {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
        }),
        "current_state_payload" => Ok(ArtifactValueTemplate::CurrentStatePayload {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
        }),
        "enum_payload" => Ok(ArtifactValueTemplate::EnumPayload {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            variant: fields.take_enum_variant_id(&format!("{prefix}.variant_id"))?,
            value: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.value"),
                depth + 1,
            )?),
        }),
        "record_field" => Ok(ArtifactValueTemplate::RecordField {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            field: fields.take_required(&format!("{prefix}.field_name"))?,
            record: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.record"),
                depth + 1,
            )?),
        }),
        "list_element" => Ok(ArtifactValueTemplate::ListElement {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            index: fields.take_bounded_usize(
                &format!("{prefix}.index"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS - 1,
            )?,
            len: fields.take_bounded_usize(
                &format!("{prefix}.len"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?,
            list: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.list"),
                depth + 1,
            )?),
        }),
        "list_prefix_element" => Ok(ArtifactValueTemplate::ListPrefixElement {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            index: fields.take_bounded_usize(
                &format!("{prefix}.index"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS - 1,
            )?,
            prefix_len: fields.take_bounded_usize(
                &format!("{prefix}.prefix_len"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?,
            list: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.list"),
                depth + 1,
            )?),
        }),
        "list_rest" => Ok(ArtifactValueTemplate::ListRest {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            prefix_len: fields.take_bounded_usize(
                &format!("{prefix}.prefix_len"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?,
            list: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.list"),
                depth + 1,
            )?),
        }),
        "map_value" => {
            let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
            let key_field = format!("{prefix}.key");
            let key = ArtifactValue::parse_field(&key_field, &fields.take_required(&key_field)?)?;
            let projection =
                MapProjectionMode::parse(&fields.take_required(&format!("{prefix}.projection"))?)?;
            let key_count = fields.take_bounded_usize(
                &format!("{prefix}.key_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut keys = Vec::with_capacity(key_count);
            for key_index in 0..key_count {
                let expected_key_field = format!("{prefix}.expected_key.{key_index}");
                keys.push(ArtifactValue::parse_field(
                    &expected_key_field,
                    &fields.take_required(&expected_key_field)?,
                )?);
            }
            Ok(ArtifactValueTemplate::MapValue {
                ty,
                key,
                keys,
                projection,
                map: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.map"),
                    depth + 1,
                )?),
            })
        }
        "map_rest" => {
            let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
            let key_count = fields.take_bounded_usize(
                &format!("{prefix}.key_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut excluded_keys = Vec::with_capacity(key_count);
            for key_index in 0..key_count {
                let excluded_key_field = format!("{prefix}.excluded_key.{key_index}");
                excluded_keys.push(ArtifactValue::parse_field(
                    &excluded_key_field,
                    &fields.take_required(&excluded_key_field)?,
                )?);
            }
            Ok(ArtifactValueTemplate::MapRest {
                ty,
                excluded_keys,
                map: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.map"),
                    depth + 1,
                )?),
            })
        }
        "process_ref" => Ok(ArtifactValueTemplate::ProcessRef {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            target_process: fields.take_process_id(&format!("{prefix}.target_process"))?,
            process_ref: fields.take_process_ref_id(&format!("{prefix}.process_ref"))?,
        }),
        "loop_element" => Ok(ArtifactValueTemplate::LoopElement {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            element: fields.take_loop_element_id(&format!("{prefix}.loop_element"))?,
        }),
        "enum_variant" => Ok(ArtifactValueTemplate::EnumVariant {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            variant: fields.take_enum_variant_id(&format!("{prefix}.variant_id"))?,
            payload: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.payload"),
                depth + 1,
            )?),
        }),
        "record" => {
            let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
            let field_count = fields.take_bounded_usize(
                &format!("{prefix}.field_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut record_fields = Vec::with_capacity(field_count);
            for field_index in 0..field_count {
                let field_prefix = format!("{prefix}.field.{field_index}");
                record_fields.push(ArtifactValueTemplateField {
                    name: fields.take_required(&format!("{field_prefix}.name"))?,
                    value: decode_value_template(
                        fields,
                        &format!("{field_prefix}.value"),
                        depth + 1,
                    )?,
                });
            }
            Ok(ArtifactValueTemplate::Record {
                ty,
                fields: record_fields,
            })
        }
        "list" => {
            let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
            let item_count = fields.take_bounded_usize(
                &format!("{prefix}.item_count"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut items = Vec::with_capacity(item_count);
            for item_index in 0..item_count {
                items.push(decode_value_template(
                    fields,
                    &format!("{prefix}.item.{item_index}"),
                    depth + 1,
                )?);
            }
            Ok(ArtifactValueTemplate::List { ty, items })
        }
        "map" => {
            let ty = fields.take_type_id(&format!("{prefix}.type_id"))?;
            let entry_count = fields.take_bounded_usize(
                &format!("{prefix}.entry_count"),
                0,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut entries = Vec::with_capacity(entry_count);
            for entry_index in 0..entry_count {
                let entry_prefix = format!("{prefix}.entry.{entry_index}");
                entries.push(ArtifactValueTemplateMapEntry {
                    key: decode_value_template(fields, &format!("{entry_prefix}.key"), depth + 1)?,
                    value: decode_value_template(
                        fields,
                        &format!("{entry_prefix}.value"),
                        depth + 1,
                    )?,
                });
            }
            Ok(ArtifactValueTemplate::Map { ty, entries })
        }
        "equality" => {
            let operator_field = format!("{prefix}.operator");
            Ok(ArtifactValueTemplate::Equality {
                ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
                operand_ty: fields.take_type_id(&format!("{prefix}.operand_type_id"))?,
                operator: ArtifactValueEqualityOperator::parse(
                    &operator_field,
                    &fields.take_required(&operator_field)?,
                )?,
                left: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.left"),
                    depth + 1,
                )?),
                right: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.right"),
                    depth + 1,
                )?),
            })
        }
        "boolean_not" => Ok(ArtifactValueTemplate::BooleanNot {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            operand: Box::new(decode_value_template(
                fields,
                &format!("{prefix}.operand"),
                depth + 1,
            )?),
        }),
        "boolean_binary" => {
            let operator_field = format!("{prefix}.operator");
            Ok(ArtifactValueTemplate::BooleanBinary {
                ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
                operator: ArtifactValueBooleanOperator::parse(
                    &operator_field,
                    &fields.take_required(&operator_field)?,
                )?,
                left: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.left"),
                    depth + 1,
                )?),
                right: Box::new(decode_value_template(
                    fields,
                    &format!("{prefix}.right"),
                    depth + 1,
                )?),
            })
        }
        value => Err(Error::new(format!("invalid {kind_key} value {value:?}"))),
    }
}

fn decode_send_target(
    fields: &mut ArtifactFields,
    action_prefix: &str,
) -> Result<ArtifactSendTarget> {
    let key = format!("{action_prefix}.target");
    match fields.take_required(&key)?.as_str() {
        "process_ref" => Ok(ArtifactSendTarget::ProcessRef(
            fields.take_process_ref_id(&format!("{action_prefix}.target_process_ref"))?,
        )),
        "received_payload" => Ok(ArtifactSendTarget::ReceivedPayload {
            ty: fields.take_type_id(&format!("{action_prefix}.target_payload_type_id"))?,
            target_process: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
        }),
        value => Err(Error::new(format!("invalid {key} value {value:?}"))),
    }
}

fn decode_action(
    fields: &mut ArtifactFields,
    action_prefix: &str,
    depth: usize,
) -> Result<ArtifactAction> {
    if depth > MAX_VALUE_TEMPLATE_DEPTH {
        return Err(Error::new(format!(
            "{action_prefix} exceeds maximum action nesting depth of {MAX_VALUE_TEMPLATE_DEPTH}"
        )));
    }
    let kind = fields.take_required(&format!("{action_prefix}.kind"))?;
    match kind.as_str() {
        "emit" => Ok(ArtifactAction::Emit {
            output: fields.take_output_id(&format!("{action_prefix}.output"))?,
        }),
        "spawn" => Ok(ArtifactAction::Spawn {
            target: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
            process_ref: fields.take_process_ref_id(&format!("{action_prefix}.process_ref"))?,
        }),
        "send" => {
            let target = decode_send_target(fields, action_prefix)?;
            let message = fields.take_message_id(&format!("{action_prefix}.message"))?;
            let payload_key = format!("{action_prefix}.payload");
            let payload = match fields.take_required(&payload_key)?.as_str() {
                "none" => None,
                "template" => Some(decode_value_template(
                    fields,
                    &format!("{action_prefix}.payload_template"),
                    0,
                )?),
                value => {
                    return Err(Error::new(format!(
                        "invalid {payload_key} value {value:?}; expected \"none\" or \"template\""
                    )));
                }
            };
            Ok(ArtifactAction::Send {
                target,
                message,
                payload,
            })
        }
        "if_else" => {
            let condition =
                decode_value_template(fields, &format!("{action_prefix}.condition"), 0)?;
            let then_action_count = fields.take_bounded_usize(
                &format!("{action_prefix}.then_action_count"),
                0,
                MAX_ACTIONS_PER_PROCESS,
            )?;
            let mut then_actions = Vec::with_capacity(then_action_count);
            for action_index in 0..then_action_count {
                then_actions.push(decode_action(
                    fields,
                    &format!("{action_prefix}.then_action.{action_index}"),
                    depth + 1,
                )?);
            }
            let else_action_count = fields.take_bounded_usize(
                &format!("{action_prefix}.else_action_count"),
                0,
                MAX_ACTIONS_PER_PROCESS,
            )?;
            let mut else_actions = Vec::with_capacity(else_action_count);
            for action_index in 0..else_action_count {
                else_actions.push(decode_action(
                    fields,
                    &format!("{action_prefix}.else_action.{action_index}"),
                    depth + 1,
                )?);
            }
            Ok(ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            })
        }
        "for_each" => {
            let body_action_count = fields.take_bounded_usize(
                &format!("{action_prefix}.body_action_count"),
                0,
                MAX_ACTIONS_PER_PROCESS,
            )?;
            let mut body = Vec::with_capacity(body_action_count);
            for action_index in 0..body_action_count {
                body.push(decode_action(
                    fields,
                    &format!("{action_prefix}.body_action.{action_index}"),
                    depth + 1,
                )?);
            }
            Ok(ArtifactAction::ForEach {
                element: ArtifactLoopElement {
                    id: fields.take_loop_element_id(&format!("{action_prefix}.loop_element"))?,
                    ty: fields.take_type_id(&format!("{action_prefix}.element_type_id"))?,
                },
                collection: decode_value_template(
                    fields,
                    &format!("{action_prefix}.collection"),
                    0,
                )?,
                max_items: fields.take_bounded_usize(
                    &format!("{action_prefix}.max_items"),
                    0,
                    MAX_VALUE_TEMPLATE_FIELDS,
                )?,
                body,
            })
        }
        _ => Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
    }
}
