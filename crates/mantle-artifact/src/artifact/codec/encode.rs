use super::super::*;

mod boundaries;
mod capabilities;
mod supervision;
mod target_requirements;

use boundaries::encode_boundaries;
use capabilities::encode_capability_descriptor;
use supervision::{encode_spawn_sites, encode_supervisor_plans};
use target_requirements::encode_target_requirements;

impl MantleArtifact {
    pub fn encode(&self) -> String {
        let mut encoded = String::with_capacity(
            crate::validation::encoded_artifact_len(self).unwrap_or(ARTIFACT_MAGIC.len() + 1),
        );
        encoded.push_str(&format!(
            "{ARTIFACT_MAGIC}\nformat={}\nschema_version={}\nsource_language={}\nmodule={}\nentry_process={}\nentry_message={}\ntype_count={}\noutput_count={}\nprotocol_count={}\nport_count={}\ncomponent_count={}\ncomposition_count={}\nprocess_count={}\n",
            self.format,
            self.schema_version,
            self.source_language,
            self.module,
            self.entry_process.as_u32(),
            self.entry_message.as_u32(),
            self.types.len(),
            self.outputs.len(),
            self.protocols.len(),
            self.ports.len(),
            self.components.len(),
            self.compositions.len(),
            self.processes.len()
        ));
        encode_target_requirements(&mut encoded, &self.target_requirements);
        for (type_index, ty) in self.types.iter().enumerate() {
            encode_type(&mut encoded, type_index, ty);
        }
        for (output_index, output) in self.outputs.iter().enumerate() {
            encoded.push_str(&format!("output.{output_index}={output}\n"));
        }
        encode_boundaries(&mut encoded, self);

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
                "{prefix}.authority_count={}\n",
                process.authorities.len()
            ));
            for (authority_index, authority) in process.authorities.iter().enumerate() {
                let authority_prefix = format!("{prefix}.authority.{authority_index}");
                encoded.push_str(&format!(
                    "{authority_prefix}.debug_name={}\n",
                    authority.debug_name
                ));
                encode_capability_descriptor(&mut encoded, &authority_prefix, authority.descriptor);
            }
            encode_spawn_sites(&mut encoded, &prefix, process);
            encode_supervisor_plans(&mut encoded, &prefix, process);
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
                    "{transition_prefix}.message={}\n{transition_prefix}.step_result={}\n",
                    transition.message.as_u32(),
                    transition.step_result.as_str()
                ));
                if let Some(current_state) = transition.current_state {
                    encoded.push_str(&format!(
                        "{transition_prefix}.current_state={}\n",
                        current_state.as_u32()
                    ));
                }
                if let Some(payload_guard) = &transition.payload_guard {
                    encoded.push_str(&format!(
                        "{transition_prefix}.payload_guard_type_id={}\n{transition_prefix}.payload_guard_value={}\n",
                        payload_guard.ty.as_u32(),
                        payload_guard.value.label()
                    ));
                }
                encode_next_state(&mut encoded, &transition_prefix, &transition.next_state);
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

fn encode_type(encoded: &mut String, type_index: usize, ty: &ArtifactType) {
    let prefix = format!("type.{type_index}");
    encoded.push_str(&format!(
        "{prefix}.label={}\n{prefix}.kind={}\n",
        ty.label,
        ty.kind.as_str(),
    ));
    match ty.kind {
        ArtifactTypeKind::Value => {
            if let Some(shape) = &ty.shape {
                encode_type_shape(encoded, &prefix, shape);
            }
        }
        ArtifactTypeKind::ProcessRef { target } => {
            encoded.push_str(&format!("{prefix}.target_process={}\n", target.as_u32()));
        }
    }
}

fn encode_type_shape(encoded: &mut String, prefix: &str, shape: &ArtifactValueShape) {
    match shape {
        ArtifactValueShape::Atom => {
            encoded.push_str(&format!("{prefix}.shape=atom\n"));
        }
        ArtifactValueShape::Primitive { primitive } => {
            encoded.push_str(&format!(
                "{prefix}.shape=primitive\n{prefix}.primitive_type={}\n",
                primitive.artifact_name()
            ));
        }
        ArtifactValueShape::Scalar { scalar } => {
            encoded.push_str(&format!(
                "{prefix}.shape=scalar\n{prefix}.scalar_type={}\n",
                scalar.artifact_name()
            ));
        }
        ArtifactValueShape::Record { fields } => {
            encoded.push_str(&format!(
                "{prefix}.shape=record\n{prefix}.field_count={}\n",
                fields.len()
            ));
            for (field_index, field) in fields.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.field.{field_index}.name={}\n{prefix}.field.{field_index}.type_id={}\n",
                    field.name,
                    field.ty.as_u32()
                ));
            }
        }
        ArtifactValueShape::Enum { variants } => {
            encoded.push_str(&format!(
                "{prefix}.shape=enum\n{prefix}.enum_variant_count={}\n",
                variants.len()
            ));
            for (variant_index, variant) in variants.iter().enumerate() {
                encoded.push_str(&format!(
                    "{prefix}.enum_variant.{variant_index}={}\n",
                    variant.label
                ));
                if let Some(payload_type) = variant.payload_type {
                    encoded.push_str(&format!(
                        "{prefix}.enum_variant.{variant_index}.payload_type_id={}\n",
                        payload_type.as_u32()
                    ));
                }
            }
        }
        ArtifactValueShape::List { element, capacity } => {
            encoded.push_str(&format!(
                "{prefix}.shape=list\n{prefix}.element_type_id={}\n{prefix}.capacity={capacity}\n",
                element.as_u32()
            ));
        }
        ArtifactValueShape::Map {
            key,
            value,
            capacity,
        } => {
            encoded.push_str(&format!(
                "{prefix}.shape=map\n{prefix}.key_type_id={}\n{prefix}.value_type_id={}\n{prefix}.capacity={capacity}\n",
                key.as_u32(),
                value.as_u32()
            ));
        }
    }
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
        ArtifactValueTemplate::EnumPayload { ty, value, variant } => {
            encoded.push_str(&format!(
                "{prefix}.kind=enum_payload\n{prefix}.type_id={}\n{prefix}.variant_id={}\n",
                ty.as_u32(),
                variant.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.value"), value);
        }
        ArtifactValueTemplate::RecordField { ty, record, field } => {
            encoded.push_str(&format!(
                "{prefix}.kind=record_field\n{prefix}.type_id={}\n{prefix}.field_id={}\n",
                ty.as_u32(),
                field.as_u32()
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
        ArtifactValueTemplate::LoopElement { ty, element } => {
            encoded.push_str(&format!(
                "{prefix}.kind=loop_element\n{prefix}.type_id={}\n{prefix}.loop_element={}\n",
                ty.as_u32(),
                element.as_u32()
            ));
        }
        ArtifactValueTemplate::EffectOutcome { ty, outcome } => {
            encoded.push_str(&format!(
                "{prefix}.kind=effect_outcome\n{prefix}.type_id={}\n{prefix}.outcome={}\n",
                ty.as_u32(),
                outcome.as_u32()
            ));
        }
        ArtifactValueTemplate::EnumVariant {
            ty,
            variant,
            payload,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=enum_variant\n{prefix}.type_id={}\n{prefix}.variant_id={}\n",
                ty.as_u32(),
                variant.as_u32()
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
                encoded.push_str(&format!(
                    "{field_prefix}.field_id={}\n",
                    field.field.as_u32()
                ));
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
        ArtifactValueTemplate::IfElse {
            ty,
            condition,
            then_value,
            else_value,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=if_else\n{prefix}.type_id={}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.condition"), condition);
            encode_value_template(encoded, &format!("{prefix}.then"), then_value);
            encode_value_template(encoded, &format!("{prefix}.else"), else_value);
        }
        ArtifactValueTemplate::Equality {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=equality\n{prefix}.type_id={}\n{prefix}.operand_type_id={}\n{prefix}.operator={}\n",
                ty.as_u32(),
                operand_ty.as_u32(),
                operator.as_str()
            ));
            encode_value_template(encoded, &format!("{prefix}.left"), left);
            encode_value_template(encoded, &format!("{prefix}.right"), right);
        }
        ArtifactValueTemplate::ScalarArithmetic {
            ty,
            operator,
            left,
            right,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=scalar_arithmetic\n{prefix}.type_id={}\n{prefix}.operator={}\n",
                ty.as_u32(),
                operator.as_str()
            ));
            encode_value_template(encoded, &format!("{prefix}.left"), left);
            encode_value_template(encoded, &format!("{prefix}.right"), right);
        }
        ArtifactValueTemplate::ScalarOrdering {
            ty,
            operand_ty,
            operator,
            left,
            right,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=scalar_ordering\n{prefix}.type_id={}\n{prefix}.operand_type_id={}\n{prefix}.operator={}\n",
                ty.as_u32(),
                operand_ty.as_u32(),
                operator.as_str()
            ));
            encode_value_template(encoded, &format!("{prefix}.left"), left);
            encode_value_template(encoded, &format!("{prefix}.right"), right);
        }
        ArtifactValueTemplate::BooleanNot { ty, operand } => {
            encoded.push_str(&format!(
                "{prefix}.kind=boolean_not\n{prefix}.type_id={}\n",
                ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{prefix}.operand"), operand);
        }
        ArtifactValueTemplate::BooleanBinary {
            ty,
            operator,
            left,
            right,
        } => {
            encoded.push_str(&format!(
                "{prefix}.kind=boolean_binary\n{prefix}.type_id={}\n{prefix}.operator={}\n",
                ty.as_u32(),
                operator.as_str()
            ));
            encode_value_template(encoded, &format!("{prefix}.left"), left);
            encode_value_template(encoded, &format!("{prefix}.right"), right);
        }
    }
}

fn encode_next_state(encoded: &mut String, prefix: &str, next_state: &NextState) {
    encoded.push_str(&format!("{prefix}.next_state={}\n", next_state.kind_str()));
    match next_state {
        NextState::Current => {}
        NextState::Value(state) => {
            encoded.push_str(&format!("{prefix}.next_state_value={}\n", state.as_u32()));
        }
        NextState::Template(template) => {
            encode_value_template(encoded, &format!("{prefix}.next_state_template"), template);
        }
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            encode_value_template(
                encoded,
                &format!("{prefix}.next_state_condition"),
                condition,
            );
            encode_next_state(encoded, &format!("{prefix}.next_state_then"), then_state);
            encode_next_state(encoded, &format!("{prefix}.next_state_else"), else_state);
        }
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
            spawn_site,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=spawn\n{action_prefix}.target_process={}\n{action_prefix}.process_ref={}\n{action_prefix}.spawn_site={}\n",
                target.as_u32(),
                process_ref.as_u32(),
                spawn_site.as_u32()
            ));
        }
        ArtifactAction::SpawnOutcome {
            outcome,
            outcome_ty,
            target,
            spawn_site,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=spawn_outcome\n{action_prefix}.outcome={}\n{action_prefix}.outcome_type_id={}\n{action_prefix}.target_process={}\n{action_prefix}.spawn_site={}\n",
                outcome.as_u32(),
                outcome_ty.as_u32(),
                target.as_u32(),
                spawn_site.as_u32()
            ));
        }
        ArtifactAction::Send {
            target,
            port,
            message,
            payload,
        } => {
            encoded.push_str(&format!("{action_prefix}.kind=send\n"));
            encode_send_target(encoded, action_prefix, target);
            if let Some(port) = port {
                encoded.push_str(&format!(
                    "{action_prefix}.boundary_port={}\n",
                    port.as_u32()
                ));
            }
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
        ArtifactAction::SendOutcome {
            outcome,
            outcome_ty,
            target,
            port,
            message,
            payload,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=send_outcome\n{action_prefix}.outcome={}\n{action_prefix}.outcome_type_id={}\n",
                outcome.as_u32(),
                outcome_ty.as_u32()
            ));
            encode_send_target(encoded, action_prefix, target);
            if let Some(port) = port {
                encoded.push_str(&format!(
                    "{action_prefix}.boundary_port={}\n",
                    port.as_u32()
                ));
            }
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
        ArtifactAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            encoded.push_str(&format!("{action_prefix}.kind=if_else\n"));
            encode_value_template(encoded, &format!("{action_prefix}.condition"), condition);
            encoded.push_str(&format!(
                "{action_prefix}.then_action_count={}\n",
                then_actions.len()
            ));
            for (action_index, action) in then_actions.iter().enumerate() {
                encode_action(
                    encoded,
                    &format!("{action_prefix}.then_action.{action_index}"),
                    action,
                );
            }
            encoded.push_str(&format!(
                "{action_prefix}.else_action_count={}\n",
                else_actions.len()
            ));
            for (action_index, action) in else_actions.iter().enumerate() {
                encode_action(
                    encoded,
                    &format!("{action_prefix}.else_action.{action_index}"),
                    action,
                );
            }
        }
        ArtifactAction::ForEach {
            element,
            collection,
            max_items,
            body,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.kind=for_each\n{action_prefix}.loop_element={}\n{action_prefix}.element_type_id={}\n{action_prefix}.max_items={max_items}\n",
                element.id.as_u32(),
                element.ty.as_u32()
            ));
            encode_value_template(encoded, &format!("{action_prefix}.collection"), collection);
            encoded.push_str(&format!(
                "{action_prefix}.body_action_count={}\n",
                body.len()
            ));
            for (action_index, action) in body.iter().enumerate() {
                encode_action(
                    encoded,
                    &format!("{action_prefix}.body_action.{action_index}"),
                    action,
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
        ArtifactSendTarget::SupervisorChild {
            supervisor,
            child,
            target_process,
        } => {
            encoded.push_str(&format!(
                "{action_prefix}.target=supervisor_child\n{action_prefix}.target_supervisor={}\n{action_prefix}.target_supervisor_child={}\n{action_prefix}.target_process={}\n",
                supervisor.as_u32(),
                child.as_u32(),
                target_process.as_u32()
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
