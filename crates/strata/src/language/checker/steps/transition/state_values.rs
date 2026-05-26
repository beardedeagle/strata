use crate::language::UNIT_TYPE;
use mantle_artifact::{ArtifactRecordField, ArtifactValue, MAX_STATE_VALUES_PER_PROCESS};

use super::*;

pub(super) fn populate_template_state_values<'a>(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    env: StepTransitionEnv<'_, '_, 'a, '_, 'a>,
    state_arg: &ValueExpr,
) -> Result<()> {
    let mut binding_sets = initial_binding_sets(context, env)?;
    for outcome in env
        .outcome_bindings
        .iter()
        .filter(|binding| source_value_uses_binding(state_arg, binding.name))
    {
        let values = finite_outcome_values(context, outcome.ty)?.ok_or_else(|| {
            Error::new(format!(
                "process {} effect outcome binding {} cannot be used as a next-state value because type {} has non-finite payload values",
                context.process.name, outcome.name, outcome.ty
            ))
        })?;
        let expanded_len = binding_sets
            .len()
            .checked_mul(values.len())
            .ok_or_else(|| Error::new("effect outcome state expansion overflowed"))?;
        if expanded_len > MAX_STATE_VALUES_PER_PROCESS {
            return Err(Error::new(format!(
                "process {} effect outcome binding {} would expand next-state candidates to {}, exceeding maximum state_value_count {MAX_STATE_VALUES_PER_PROCESS}",
                context.process.name, outcome.name, expanded_len
            )));
        }
        let mut expanded = Vec::with_capacity(expanded_len);
        for bindings in &binding_sets {
            for value in &values {
                let mut next = bindings.clone();
                next.push(ValueBinding {
                    name: outcome.name,
                    ty: outcome.ty,
                    label: value.label(),
                    value: Some(value.clone()),
                });
                expanded.push(next);
            }
        }
        binding_sets = expanded;
    }

    for bindings in binding_sets {
        state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            types,
            state_arg,
            &bindings,
        )?;
    }
    Ok(())
}

fn initial_binding_sets<'a>(
    context: &StepCheckContext<'_>,
    env: StepTransitionEnv<'_, '_, 'a, '_, 'a>,
) -> Result<Vec<Vec<ValueBinding<'a>>>> {
    if !env.input.payload_bindings.is_empty() {
        return payload_binding_sets(context, env);
    }
    if !env.input.state_payload_bindings.is_empty() {
        return Ok(vec![state_payload_bindings(env)]);
    }
    Ok(vec![Vec::new()])
}

fn payload_binding_sets<'a>(
    context: &StepCheckContext<'_>,
    env: StepTransitionEnv<'_, '_, 'a, '_, 'a>,
) -> Result<Vec<Vec<ValueBinding<'a>>>> {
    let payloads = match env.input.payload_guard {
        Some(payload) => vec![payload],
        None => context
            .message_cases
            .payload_values(context.process_id, env.input.variant)?
            .iter()
            .collect::<Vec<_>>(),
    };
    let mut sets = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let payload_values = env
            .input
            .payload_bindings
            .iter()
            .map(|binding| {
                checked_payload_binding(
                    context.module,
                    context.semantic_index,
                    payload,
                    &PatternPayloadParam {
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                        path: binding.path.clone(),
                    },
                )?
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} message payload {} does not match step pattern binding {}",
                        context.process.name,
                        payload.label(),
                        binding.name
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut bindings = Vec::new();
        for (binding, (label, value)) in env.input.payload_bindings.iter().zip(&payload_values) {
            bindings.push(ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: label.clone(),
                value: value.clone(),
            });
        }
        bindings.extend(state_payload_bindings(env));
        sets.push(bindings);
    }
    Ok(sets)
}

fn state_payload_bindings<'a>(env: StepTransitionEnv<'_, '_, 'a, '_, 'a>) -> Vec<ValueBinding<'a>> {
    env.input
        .state_payload_bindings
        .iter()
        .map(|binding| ValueBinding {
            name: &binding.name,
            ty: &binding.ty,
            label: binding.label.clone(),
            value: Some(binding.value.clone()),
        })
        .collect()
}

fn finite_outcome_values(
    context: &StepCheckContext<'_>,
    ty: &TypeRef,
) -> Result<Option<Vec<ArtifactValue>>> {
    let Some(values) = finite_values_for_type(
        context.module,
        context.semantic_index,
        ty,
        MAX_STATE_VALUES_PER_PROCESS,
    )?
    else {
        return Ok(None);
    };
    if values.len() > MAX_STATE_VALUES_PER_PROCESS {
        return Err(Error::new(format!(
            "process {} effect outcome type {} expands beyond maximum state_value_count {MAX_STATE_VALUES_PER_PROCESS}",
            context.process.name, ty
        )));
    }
    Ok(Some(values))
}

fn finite_values_for_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    ty: &TypeRef,
    limit: usize,
) -> Result<Option<Vec<ArtifactValue>>> {
    if limit == 0 {
        return Ok(None);
    }
    if semantic_index.is_unit_type(ty)? {
        return Ok(Some(vec![ArtifactValue::Atom(UNIT_TYPE.to_string())]));
    }
    if semantic_index.collection_type(ty)?.is_some() {
        return Ok(None);
    }
    if let Ok(record) = semantic_index.record_decl(module, ty) {
        return finite_record_values(module, semantic_index, record, limit);
    }
    let Ok(value_enum) = semantic_index.value_enum(module, ty) else {
        return Ok(None);
    };
    let mut values = Vec::new();
    for variant in value_enum.variants {
        match variant.payload_type {
            Some(payload_ty) => {
                let Some(payload_values) =
                    finite_values_for_type(module, semantic_index, &payload_ty, limit)?
                else {
                    return Ok(None);
                };
                for payload in payload_values {
                    values.push(ArtifactValue::EnumVariant {
                        variant: variant.name.to_string(),
                        payload: Box::new(payload),
                    });
                    if values.len() > limit {
                        return Ok(None);
                    }
                }
            }
            None => {
                values.push(ArtifactValue::Atom(variant.name.to_string()));
                if values.len() > limit {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(values))
}

fn finite_record_values(
    module: &Module,
    semantic_index: &SemanticIndex,
    record: &Record,
    limit: usize,
) -> Result<Option<Vec<ArtifactValue>>> {
    if record.fields.is_empty() {
        return Ok(Some(vec![ArtifactValue::Atom(record.name.to_string())]));
    }
    let mut rows: Vec<Vec<ArtifactRecordField>> = vec![Vec::new()];
    for field in &record.fields {
        let Some(field_values) = finite_values_for_type(module, semantic_index, &field.ty, limit)?
        else {
            return Ok(None);
        };
        let Some(expanded) =
            append_finite_record_field(rows, field.name.to_string(), &field_values, limit)?
        else {
            return Ok(None);
        };
        rows = expanded;
    }
    Ok(Some(
        rows.into_iter()
            .map(|fields| ArtifactValue::Record {
                constructor: record.name.to_string(),
                fields,
            })
            .collect(),
    ))
}

fn append_finite_record_field(
    rows: Vec<Vec<ArtifactRecordField>>,
    name: String,
    values: &[ArtifactValue],
    limit: usize,
) -> Result<Option<Vec<Vec<ArtifactRecordField>>>> {
    let capacity = rows
        .len()
        .checked_mul(values.len())
        .ok_or_else(|| Error::new("finite state value expansion overflowed"))?;
    if capacity > limit {
        return Ok(None);
    }
    let mut expanded = Vec::with_capacity(capacity);
    for row in rows {
        for value in values {
            let mut next = row.clone();
            next.push(ArtifactRecordField {
                name: name.clone(),
                value: value.clone(),
            });
            expanded.push(next);
        }
    }
    Ok(Some(expanded))
}
