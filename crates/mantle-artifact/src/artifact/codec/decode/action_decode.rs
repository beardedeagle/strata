use super::*;
use crate::fields::ArtifactFields;

pub(super) fn decode_action(
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
    match kind {
        "emit" => Ok(ArtifactAction::Emit {
            output: fields.take_output_id(&format!("{action_prefix}.output"))?,
        }),
        "spawn" => Ok(ArtifactAction::Spawn {
            target: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
            process_ref: fields.take_process_ref_id(&format!("{action_prefix}.process_ref"))?,
            spawn_site: fields.take_spawn_site_id(&format!("{action_prefix}.spawn_site"))?,
        }),
        "spawn_outcome" => Ok(ArtifactAction::SpawnOutcome {
            outcome: fields.take_effect_outcome_id(&format!("{action_prefix}.outcome"))?,
            outcome_ty: fields.take_type_id(&format!("{action_prefix}.outcome_type_id"))?,
            target: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
            spawn_site: fields.take_spawn_site_id(&format!("{action_prefix}.spawn_site"))?,
        }),
        "send" => {
            let target = decode_send_target(fields, action_prefix)?;
            let message = fields.take_message_id(&format!("{action_prefix}.message"))?;
            let payload = decode_optional_payload_template(fields, action_prefix)?;
            Ok(ArtifactAction::Send {
                target,
                message,
                payload,
            })
        }
        "send_outcome" => {
            let outcome = fields.take_effect_outcome_id(&format!("{action_prefix}.outcome"))?;
            let outcome_ty = fields.take_type_id(&format!("{action_prefix}.outcome_type_id"))?;
            let target = decode_send_target(fields, action_prefix)?;
            let message = fields.take_message_id(&format!("{action_prefix}.message"))?;
            let payload = decode_optional_payload_template(fields, action_prefix)?;
            Ok(ArtifactAction::SendOutcome {
                outcome,
                outcome_ty,
                target,
                message,
                payload,
            })
        }
        "if_else" => {
            let condition =
                decode_value_template(fields, &format!("{action_prefix}.condition"), 0)?;
            let then_actions = decode_action_list(fields, action_prefix, "then", depth)?;
            let else_actions = decode_action_list(fields, action_prefix, "else", depth)?;
            Ok(ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            })
        }
        "for_each" => {
            let body = decode_action_list(fields, action_prefix, "body", depth)?;
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

fn decode_send_target(
    fields: &mut ArtifactFields,
    action_prefix: &str,
) -> Result<ArtifactSendTarget> {
    let key = format!("{action_prefix}.target");
    match fields.take_required(&key)? {
        "process_ref" => Ok(ArtifactSendTarget::ProcessRef(
            fields.take_process_ref_id(&format!("{action_prefix}.target_process_ref"))?,
        )),
        "supervisor_child" => Ok(ArtifactSendTarget::SupervisorChild {
            supervisor: fields.take_supervisor_id(&format!("{action_prefix}.target_supervisor"))?,
            child: fields
                .take_supervisor_child_id(&format!("{action_prefix}.target_supervisor_child"))?,
            target_process: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
        }),
        "received_payload" => Ok(ArtifactSendTarget::ReceivedPayload {
            ty: fields.take_type_id(&format!("{action_prefix}.target_payload_type_id"))?,
            target_process: fields.take_process_id(&format!("{action_prefix}.target_process"))?,
        }),
        value => Err(Error::new(format!("invalid {key} value {value:?}"))),
    }
}

fn decode_optional_payload_template(
    fields: &mut ArtifactFields,
    action_prefix: &str,
) -> Result<Option<ArtifactValueTemplate>> {
    let payload_key = format!("{action_prefix}.payload");
    match fields.take_required(&payload_key)? {
        "none" => Ok(None),
        "template" => Ok(Some(decode_value_template(
            fields,
            &format!("{action_prefix}.payload_template"),
            0,
        )?)),
        value => Err(Error::new(format!(
            "invalid {payload_key} value {value:?}; expected \"none\" or \"template\""
        ))),
    }
}

fn decode_action_list(
    fields: &mut ArtifactFields,
    action_prefix: &str,
    label: &str,
    depth: usize,
) -> Result<Vec<ArtifactAction>> {
    let action_count = fields.take_bounded_usize(
        &format!("{action_prefix}.{label}_action_count"),
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;
    let mut actions = Vec::with_capacity(action_count);
    for action_index in 0..action_count {
        actions.push(decode_action(
            fields,
            &format!("{action_prefix}.{label}_action.{action_index}"),
            depth + 1,
        )?);
    }
    Ok(actions)
}
