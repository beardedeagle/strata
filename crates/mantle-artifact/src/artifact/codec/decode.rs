use super::super::*;
use crate::fields::ArtifactFields;
use crate::{
    MAX_AUTHORITIES_PER_PROCESS, MAX_NEXT_STATE_IF_ELSE_DEPTH, MAX_SPAWN_SITES_PER_PROCESS,
    MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR, MAX_SUPERVISORS_PER_PROCESS,
};

mod action_decode;
mod scalar_decode;
mod value_template_decode;

use action_decode::decode_action;

impl MantleArtifact {
    pub fn decode(contents: &str) -> Result<Self> {
        let mut fields = ArtifactFields::parse(contents)?;
        let format = fields.take_required("format")?;
        let schema_version = fields.take_required("schema_version")?;
        validate_artifact_identity(format, schema_version)?;

        let process_count = fields.take_bounded_usize("process_count", 1, MAX_PROCESS_COUNT)?;
        let type_count = fields.take_bounded_usize("type_count", 1, MAX_TYPE_COUNT)?;
        let output_count = fields.take_bounded_usize("output_count", 0, MAX_OUTPUT_LITERALS)?;
        let mut types = Vec::with_capacity(type_count);
        for type_index in 0..type_count {
            types.push(decode_type(&mut fields, type_index)?);
        }
        let mut outputs = Vec::with_capacity(output_count);
        for output_index in 0..output_count {
            outputs.push(fields.take_required_string(&format!("output.{output_index}"))?);
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
                    label: fields
                        .take_required_string(&format!("{prefix}.message.{message_index}"))?,
                    payload_type: fields.take_optional_type_id(&format!(
                        "{prefix}.message.{message_index}.payload_type_id"
                    ))?,
                });
            }

            let authority_count = fields.take_bounded_usize(
                &format!("{prefix}.authority_count"),
                0,
                MAX_AUTHORITIES_PER_PROCESS,
            )?;
            let mut authorities = Vec::with_capacity(authority_count);
            for authority_index in 0..authority_count {
                let authority_prefix = format!("{prefix}.authority.{authority_index}");
                let debug_name =
                    fields.take_required_string(&format!("{authority_prefix}.debug_name"))?;
                let kind = fields.take_required(&format!("{authority_prefix}.kind"))?;
                let descriptor = match kind {
                    "spawn" => ArtifactCapabilityDescriptor::Spawn {
                        target: fields
                            .take_process_id(&format!("{authority_prefix}.target_process"))?,
                    },
                    _ => {
                        return Err(Error::new(format!(
                            "invalid {authority_prefix}.kind value {kind:?}"
                        )));
                    }
                };
                authorities.push(ArtifactAuthority {
                    debug_name,
                    descriptor,
                });
            }

            let spawn_site_count = fields.take_bounded_usize(
                &format!("{prefix}.spawn_site_count"),
                0,
                MAX_SPAWN_SITES_PER_PROCESS,
            )?;
            let mut spawn_sites = Vec::with_capacity(spawn_site_count);
            for spawn_site_index in 0..spawn_site_count {
                let spawn_site_prefix = format!("{prefix}.spawn_site.{spawn_site_index}");
                spawn_sites.push(ArtifactSpawnSite {
                    target: fields
                        .take_process_id(&format!("{spawn_site_prefix}.target_process"))?,
                    authority: fields
                        .take_optional_authority_id(&format!("{spawn_site_prefix}.authority"))?,
                    supervisor: fields
                        .take_optional_supervisor_id(&format!("{spawn_site_prefix}.supervisor"))?,
                    child: fields.take_optional_supervisor_child_id(&format!(
                        "{spawn_site_prefix}.supervisor_child"
                    ))?,
                    kind: ArtifactSpawnKind::parse(
                        fields.take_required(&format!("{spawn_site_prefix}.kind"))?,
                    )?,
                });
            }

            let supervisor_count = fields.take_bounded_usize(
                &format!("{prefix}.supervisor_count"),
                0,
                MAX_SUPERVISORS_PER_PROCESS,
            )?;
            let mut supervisor_plans = Vec::with_capacity(supervisor_count);
            for supervisor_index in 0..supervisor_count {
                let supervisor_prefix = format!("{prefix}.supervisor.{supervisor_index}");
                let child_count = fields.take_bounded_usize(
                    &format!("{supervisor_prefix}.child_count"),
                    1,
                    MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR,
                )?;
                let mut children = Vec::with_capacity(child_count);
                for child_index in 0..child_count {
                    let child_prefix = format!("{supervisor_prefix}.child.{child_index}");
                    children.push(ArtifactSupervisorChild {
                        debug_name: fields
                            .take_required_string(&format!("{child_prefix}.debug_name"))?,
                        target: fields
                            .take_process_id(&format!("{child_prefix}.target_process"))?,
                        mode: ArtifactSupervisorChildMode::parse(
                            fields.take_required(&format!("{child_prefix}.mode"))?,
                        )?,
                        spawn_site: fields
                            .take_spawn_site_id(&format!("{child_prefix}.spawn_site"))?,
                    });
                }
                supervisor_plans.push(ArtifactSupervisorPlan {
                    strategy: ArtifactSupervisorStrategy::parse(
                        fields.take_required(&format!("{supervisor_prefix}.strategy"))?,
                    )?,
                    intensity: ArtifactSupervisorRestartIntensity {
                        max_restarts: fields
                            .take_required(&format!("{supervisor_prefix}.max_restarts"))?
                            .parse::<u32>()
                            .map_err(|_| {
                                Error::new(format!(
                                    "invalid {supervisor_prefix}.max_restarts value"
                                ))
                            })?,
                        within_ms: fields
                            .take_required(&format!("{supervisor_prefix}.within_ms"))?
                            .parse::<u64>()
                            .map_err(|_| {
                                Error::new(format!("invalid {supervisor_prefix}.within_ms value"))
                            })?,
                    },
                    children,
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
                        .take_required_string(&format!("{process_ref_prefix}.debug_name"))?,
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
                        ArtifactEffect::parse(effect)
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
                debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
                state_type: fields.take_type_id(&format!("{prefix}.state_type_id"))?,
                state_values,
                message_type: fields.take_type_id(&format!("{prefix}.message_type_id"))?,
                message_variants,
                authorities,
                spawn_sites,
                supervisor_plans,
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
            format: format.to_string(),
            schema_version: schema_version.to_string(),
            source_language: fields.take_required_string("source_language")?,
            module: fields.take_required_string("module")?,
            entry_process: fields.take_process_id("entry_process")?,
            entry_message: fields.take_message_id("entry_message")?,
            types,
            outputs,
            processes,
            source_hash_fnv1a64: fields.take_required_string("source_hash_fnv1a64")?,
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
            ArtifactValue::parse_field(&payload_value_key, value)?,
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
            ArtifactValue::parse_field(&payload_value_key, value)?,
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
    let value = ArtifactValue::parse_field(&value_key, fields.take_required(&value_key)?)?;
    let label = fields.take_required_string(&format!("{prefix}.label"))?;
    let mut state_value = ArtifactStateValue::with_label(ty, value, label)?;
    state_value.payload = payload;
    Ok(state_value)
}

fn decode_type(fields: &mut ArtifactFields, type_index: usize) -> Result<ArtifactType> {
    let prefix = format!("type.{type_index}");
    let label = fields.take_required_string(&format!("{prefix}.label"))?;
    let kind_value = fields.take_required(&format!("{prefix}.kind"))?;
    let target = if kind_value == "process_ref" {
        Some(fields.take_process_id(&format!("{prefix}.target_process"))?)
    } else {
        None
    };
    let kind = ArtifactTypeKind::parse(kind_value, target)?;
    let shape = if matches!(kind, ArtifactTypeKind::Value) {
        Some(decode_type_shape(fields, &prefix)?)
    } else {
        None
    };
    Ok(ArtifactType { label, kind, shape })
}

fn decode_type_shape(fields: &mut ArtifactFields, prefix: &str) -> Result<ArtifactValueShape> {
    match fields.take_required(&format!("{prefix}.shape"))? {
        "atom" => Ok(ArtifactValueShape::Atom),
        "scalar" => scalar_decode::decode_scalar_shape(fields, prefix),
        "record" => {
            let field_count = fields.take_bounded_usize(
                &format!("{prefix}.field_count"),
                1,
                MAX_VALUE_TEMPLATE_FIELDS,
            )?;
            let mut fields_out = Vec::with_capacity(field_count);
            for field_index in 0..field_count {
                fields_out.push(ArtifactTypeField {
                    name: fields
                        .take_required_string(&format!("{prefix}.field.{field_index}.name"))?,
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
                        .take_required_string(&format!("{prefix}.enum_variant.{variant_index}"))?,
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
    match fields.take_required(&key)? {
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
    match fields.take_required(&kind_key)? {
        "literal" => {
            let value_key = format!("{prefix}.value");
            Ok(ArtifactValueTemplate::Literal {
                ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
                value: ArtifactValue::parse_field(&value_key, fields.take_required(&value_key)?)?,
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
            field: fields.take_record_field_id(&format!("{prefix}.field_id"))?,
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
            let key = ArtifactValue::parse_field(&key_field, fields.take_required(&key_field)?)?;
            let projection =
                MapProjectionMode::parse(fields.take_required(&format!("{prefix}.projection"))?)?;
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
                    fields.take_required(&expected_key_field)?,
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
                    fields.take_required(&excluded_key_field)?,
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
        "effect_outcome" => Ok(ArtifactValueTemplate::EffectOutcome {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            outcome: fields.take_effect_outcome_id(&format!("{prefix}.outcome"))?,
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
                    field: fields.take_record_field_id(&format!("{field_prefix}.field_id"))?,
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
        "if_else" => value_template_decode::decode_if_else_template(fields, prefix, depth),
        "equality" => {
            let operator_field = format!("{prefix}.operator");
            Ok(ArtifactValueTemplate::Equality {
                ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
                operand_ty: fields.take_type_id(&format!("{prefix}.operand_type_id"))?,
                operator: ArtifactValueEqualityOperator::parse(
                    &operator_field,
                    fields.take_required(&operator_field)?,
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
        "scalar_arithmetic" => {
            scalar_decode::decode_scalar_arithmetic_template(fields, prefix, depth)
        }
        "scalar_ordering" => scalar_decode::decode_scalar_ordering_template(fields, prefix, depth),
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
                    fields.take_required(&operator_field)?,
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
