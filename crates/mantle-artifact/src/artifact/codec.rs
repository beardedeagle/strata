use super::*;
use crate::fields::ArtifactFields;

impl MantleArtifact {
    pub fn encode(&self) -> String {
        let mut encoded = format!(
            "{ARTIFACT_MAGIC}\nformat={}\nschema_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\ntype_count={}\noutput_count={}\nprocess_count={}\n",
            self.format,
            self.schema_version,
            self.source_language,
            self.module,
            self.entry_process.as_u32(),
            self.entry_message.as_u32(),
            self.types.len(),
            self.outputs.len(),
            self.processes.len()
        );
        for (type_index, ty) in self.types.iter().enumerate() {
            encode_type(&mut encoded, type_index, ty);
        }
        for (output_index, output) in self.outputs.iter().enumerate() {
            encoded.push_str(&format!("output.{output_index}={output}\n"));
        }

        for (process_index, process) in self.processes.iter().enumerate() {
            let prefix = format!("process.{process_index}");
            encoded.push_str(&format!(
                "{prefix}.debug_name={}\n{prefix}.state_type_id={}\n{prefix}.state_value_count={}\n",
                process.debug_name,
                process.state_type.as_u32(),
                process.state_values.len()
            ));
            for (value_index, value) in process.state_values.iter().enumerate() {
                encode_state_value(
                    &mut encoded,
                    &format!("{prefix}.state_value.{value_index}"),
                    value,
                );
            }
            encoded.push_str(&format!(
                "{prefix}.message_type_id={}\n{prefix}.message_count={}\n",
                process.message_type.as_u32(),
                process.message_variants.len()
            ));
            for (message_index, message) in process.message_variants.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.message.{message_index}={}\n",
                    message.label
                ));
                if let Some(payload_type) = message.payload_type {
                    encoded.push_str(&format!(
                        "{prefix}.message.{message_index}.payload_type_id={}\n",
                        payload_type.as_u32()
                    ));
                }
            }
            encoded.push_str(&format!(
                "{prefix}.process_ref_count={}\n",
                process.process_refs.len()
            ));
            for (process_ref_index, process_ref) in process.process_refs.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.process_ref.{process_ref_index}.debug_name={}\n{prefix}.process_ref.{process_ref_index}.target_process={}\n",
                    process_ref.debug_name,
                    process_ref.target.as_u32()
                ));
            }
            encoded.push_str(&format!(
                "{prefix}.mailbox_bound={}\n{prefix}.init_state={}\n{prefix}.transition_count={}\n",
                process.mailbox_bound,
                process.init_state.as_u32(),
                process.transitions.len()
            ));
            for (transition_index, transition) in process.transitions.iter().enumerate() {
                let transition_prefix = format!("{prefix}.transition.{transition_index}");
                encoded.push_str(&format!(
                    "{transition_prefix}.message={}\n{transition_prefix}.step_result={}\n{transition_prefix}.next_state={}\n",
                    transition.message.as_u32(),
                    transition.step_result.as_str(),
                    transition.next_state.kind_str()
                ));
                if let Some(current_state) = transition.current_state {
                    encoded.push_str(&format!(
                        "{transition_prefix}.current_state={}\n",
                        current_state.as_u32()
                    ));
                }
                if let NextState::Value(state) = &transition.next_state {
                    encoded.push_str(&format!(
                        "{transition_prefix}.next_state_value={}\n",
                        state.as_u32()
                    ));
                }
                if let NextState::Template(template) = &transition.next_state {
                    encode_value_template(
                        &mut encoded,
                        &format!("{transition_prefix}.next_state_template"),
                        template,
                    );
                }
                encoded.push_str(&format!(
                    "{transition_prefix}.effect_count={}\n",
                    transition.effects.len()
                ));
                for (effect_index, effect) in transition.effects.iter().enumerate() {
                    encoded.push_str(&format!(
                        "{transition_prefix}.effect.{effect_index}={}\n",
                        effect.as_str()
                    ));
                }
                encoded.push_str(&format!(
                    "{transition_prefix}.action_count={}\n",
                    transition.actions.len()
                ));
                for (action_index, action) in transition.actions.iter().enumerate() {
                    let action_prefix = format!("{transition_prefix}.action.{action_index}");
                    encode_action(&mut encoded, &action_prefix, action);
                }
            }
        }

        encoded.push_str(&format!(
            "source_hash_fnv1a64={}\n",
            self.source_hash_fnv1a64
        ));
        encoded
    }

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
                    actions.push(decode_action(&mut fields, &action_prefix)?);
                }

                transitions.push(ArtifactTransition {
                    current_state: fields
                        .take_optional_state_id(&format!("{transition_prefix}.current_state"))?,
                    message: fields.take_message_id(&format!("{transition_prefix}.message"))?,
                    step_result: fields
                        .take_step_result(&format!("{transition_prefix}.step_result"))?,
                    next_state: decode_next_state(&mut fields, &transition_prefix)?,
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

fn encode_state_value(encoded: &mut String, prefix: &str, state_value: &ArtifactStateValue) {
    encoded.push_str(&format!(
        "{prefix}.type_id={}\n{prefix}.value={}\n{prefix}.label={}\n",
        state_value.ty.as_u32(),
        state_value.value.label(),
        state_value.label
    ));
    if let Some(payload) = &state_value.payload {
        encoded.push_str(&format!(
            "{prefix}.payload_type_id={}\n{prefix}.payload_value={}\n",
            payload.ty.as_u32(),
            payload.value.label()
        ));
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

fn encode_type(encoded: &mut String, type_index: usize, ty: &ArtifactType) {
    let prefix = format!("type.{type_index}");
    encoded.push_str(&format!(
        "{prefix}.label={}\n{prefix}.kind={}\n",
        ty.label,
        ty.kind.as_str()
    ));
    if let ArtifactTypeKind::ProcessRef { target } = ty.kind {
        encoded.push_str(&format!("{prefix}.target_process={}\n", target.as_u32()));
    }
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
    Ok(ArtifactType {
        label,
        kind: ArtifactTypeKind::parse(&kind_value, target)?,
    })
}

fn encode_value_template(encoded: &mut String, prefix: &str, template: &ArtifactValueTemplate) {
    match template {
        ArtifactValueTemplate::Literal { ty, value } => {
            let value = value.label();
            encoded.push_str(&format!(
                "{prefix}.kind=literal\n{prefix}.type_id={}\n{prefix}.value={value}\n",
                ty.as_u32()
            ));
        }
        ArtifactValueTemplate::ReceivedPayload { ty } => {
            encoded.push_str(&format!(
                "{prefix}.kind=received_payload\n{prefix}.type_id={}\n",
                ty.as_u32()
            ));
        }
        ArtifactValueTemplate::CurrentStatePayload { ty } => {
            encoded.push_str(&format!(
                "{prefix}.kind=current_state_payload\n{prefix}.type_id={}\n",
                ty.as_u32()
            ));
        }
        ArtifactValueTemplate::RecordField { ty, record, field } => {
            encoded.push_str(&format!(
                "{prefix}.kind=record_field\n{prefix}.type_id={}\n{prefix}.field_name={field}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.record"), record);
        }
        ArtifactValueTemplate::ListElement {
            ty,
            list,
            index,
            len,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=list_element\n{prefix}.type_id={}\n{prefix}.index={index}\n{prefix}.len={len}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.list"), list);
        }
        ArtifactValueTemplate::ListPrefixElement {
            ty,
            list,
            index,
            prefix_len,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=list_prefix_element\n{prefix}.type_id={}\n{prefix}.index={index}\n{prefix}.prefix_len={prefix_len}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.list"), list);
        }
        ArtifactValueTemplate::ListRest {
            ty,
            list,
            prefix_len,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=list_rest\n{prefix}.type_id={}\n{prefix}.prefix_len={prefix_len}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.list"), list);
        }
        ArtifactValueTemplate::MapValue {
            ty,
            map,
            key,
            keys,
            projection,
        } => {
            let key = key.label();
            encoded.push_str(&format!(
                "{prefix}.kind=map_value\n{prefix}.type_id={}\n{prefix}.key={key}\n{prefix}.projection={}\n{prefix}.key_count={}\n",
                ty.as_u32(),
                projection.as_str(),
                keys.len()
            ));
            for (key_index, expected_key) in keys.iter().enumerate() {
                let expected_key = expected_key.label();
                encoded.push_str(&format!(
                    "{prefix}.expected_key.{key_index}={expected_key}\n"
                ));
            }
            encode_value_template(encoded, &format!("{prefix}.map"), map);
        }
        ArtifactValueTemplate::MapRest {
            ty,
            map,
            excluded_keys,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=map_rest\n{prefix}.type_id={}\n{prefix}.key_count={}\n",
                ty.as_u32(),
                excluded_keys.len()
            ));
            for (key_index, excluded_key) in excluded_keys.iter().enumerate() {
                let excluded_key = excluded_key.label();
                encoded.push_str(&format!(
                    "{prefix}.excluded_key.{key_index}={excluded_key}\n"
                ));
            }
            encode_value_template(encoded, &format!("{prefix}.map"), map);
        }
        ArtifactValueTemplate::ProcessRef {
            ty,
            target_process,
            process_ref,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=process_ref\n{prefix}.type_id={}\n{prefix}.target_process={}\n{prefix}.process_ref={}\n",
                ty.as_u32(),
                target_process.as_u32(),
                process_ref.as_u32()
            ));
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=enum_variant\n{prefix}.type_id={}\n{prefix}.variant={variant}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.payload"), payload);
        }
        ArtifactValueTemplate::Record { ty, fields } => {
            encoded.push_str(&format!(
                "{prefix}.kind=record\n{prefix}.type_id={}\n{prefix}.field_count={}\n",
                ty.as_u32(),
                fields.len()
            ));
            for (field_index, field) in fields.iter().enumerate() {
                let field_prefix = format!("{prefix}.field.{field_index}");
                encoded.push_str(&format!("{field_prefix}.name={}\n", field.name));
                encode_value_template(encoded, &format!("{field_prefix}.value"), &field.value);
            }
        }
        ArtifactValueTemplate::List { ty, items } => {
            encoded.push_str(&format!(
                "{prefix}.kind=list\n{prefix}.type_id={}\n{prefix}.item_count={}\n",
                ty.as_u32(),
                items.len()
            ));
            for (item_index, item) in items.iter().enumerate() {
                encode_value_template(encoded, &format!("{prefix}.item.{item_index}"), item);
            }
        }
        ArtifactValueTemplate::Map { ty, entries } => {
            encoded.push_str(&format!(
                "{prefix}.kind=map\n{prefix}.type_id={}\n{prefix}.entry_count={}\n",
                ty.as_u32(),
                entries.len()
            ));
            for (entry_index, entry) in entries.iter().enumerate() {
                let entry_prefix = format!("{prefix}.entry.{entry_index}");
                encode_value_template(encoded, &format!("{entry_prefix}.key"), &entry.key);
                encode_value_template(encoded, &format!("{entry_prefix}.value"), &entry.value);
            }
        }
    }
}

fn decode_next_state(fields: &mut ArtifactFields, prefix: &str) -> Result<NextState> {
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
        value => Err(Error::new(format!(
            "invalid {key} value {value:?}; expected \"current\", \"value\", or \"template\""
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
        "enum_variant" => Ok(ArtifactValueTemplate::EnumVariant {
            ty: fields.take_type_id(&format!("{prefix}.type_id"))?,
            variant: fields.take_required(&format!("{prefix}.variant"))?,
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
        value => Err(Error::new(format!("invalid {kind_key} value {value:?}"))),
    }
}

fn encode_action(encoded: &mut String, action_prefix: &str, action: &ArtifactAction) {
    match action {
        ArtifactAction::Emit { output } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=emit\n{action_prefix}.output={}\n",
                output.as_u32()
            ));
        }
        ArtifactAction::Spawn {
            target,
            process_ref,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n{action_prefix}.process_ref={}\n",
                target.as_u32(),
                process_ref.as_u32()
            ));
        }
        ArtifactAction::Send {
            target,
            message,
            payload,
        } => {
            encoded.push_str(&format!("{action_prefix}.kind=send\n"));
            encode_send_target(encoded, action_prefix, target);
            encoded.push_str(&format!(
                "{action_prefix}.message={}\n{action_prefix}.payload={}\n",
                message.as_u32(),
                if payload.is_some() {
                    "template"
                } else {
                    "none"
                }
            ));
            if let Some(payload) = payload {
                encode_value_template(
                    encoded,
                    &format!("{action_prefix}.payload_template"),
                    payload,
                );
            }
        }
    }
}

fn encode_send_target(encoded: &mut String, action_prefix: &str, target: &ArtifactSendTarget) {
    match target {
        ArtifactSendTarget::ProcessRef(process_ref) => {
            encoded.push_str(&format!(
                "{action_prefix}.target=process_ref\n{action_prefix}.target_process_ref={}\n",
                process_ref.as_u32()
            ));
        }
        ArtifactSendTarget::ReceivedPayload { ty, target_process } => {
            encoded.push_str(&format!(
                "{action_prefix}.target=received_payload\n{action_prefix}.target_payload_type_id={}\n{action_prefix}.target_process={}\n",
                ty.as_u32(),
                target_process.as_u32()
            ));
        }
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

fn decode_action(fields: &mut ArtifactFields, action_prefix: &str) -> Result<ArtifactAction> {
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
        _ => Err(Error::new(format!("invalid artifact action kind {kind:?}"))),
    }
}
