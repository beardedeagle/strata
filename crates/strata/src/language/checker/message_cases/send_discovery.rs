use super::*;

pub(super) fn discover_send_statements(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    statements: &[Statement],
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let mut changed = false;
    for statement in statements {
        changed |= discover_send_statement(
            builders,
            process,
            pattern,
            statement,
            bindings,
            state_payload_bindings,
            context,
        )?;
    }
    Ok(changed)
}

fn discover_send_statement(
    builders: &mut [MessageCaseBuilder<'_>],
    process: &Process,
    pattern: &StepPattern,
    statement: &Statement,
    bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    match statement {
        Statement::Send {
            target,
            message,
            payload,
        } => {
            let target_process_id = resolve_send_target_process_for_discovery(
                process,
                context.semantic_index,
                context.process_refs,
                pattern,
                target,
            )?;
            let target_variant = context.semantic_index.message_id_for_process(
                context.module,
                process.name.as_str(),
                target_process_id,
                message,
            )?;
            let builder = builders.get_mut(target_process_id.index()).ok_or_else(|| {
                Error::new(format!(
                    "process id {} is not declared",
                    target_process_id.as_u32()
                ))
            })?;
            add_discovered_send_payload_cases(
                builder,
                target_variant,
                payload.as_ref(),
                bindings,
                state_payload_bindings,
                context,
            )
        }
        Statement::IfElse {
            then_body,
            else_body,
            ..
        } => {
            let then_changed = discover_send_statements(
                builders,
                process,
                pattern,
                then_body,
                bindings,
                state_payload_bindings,
                context,
            )?;
            let else_changed = discover_send_statements(
                builders,
                process,
                pattern,
                else_body,
                bindings,
                state_payload_bindings,
                context,
            )?;
            Ok(then_changed || else_changed)
        }
        Statement::Emit(_) | Statement::LetProcessRef { .. } | Statement::ForEach { .. } => {
            Ok(false)
        }
    }
}
