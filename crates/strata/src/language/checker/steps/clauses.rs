use super::discovery::{check_step_pattern, check_step_shape};
use super::returns::{
    StepReturnInput, StepReturnPreadmitContext, preadmit_step_return_state_value,
};
use super::*;
use crate::language::checked::CheckedTypeId;

mod policy;
mod preadmit;
mod state_match;

use policy::{
    matching_step_body_clauses, reject_unreachable_payload_guarded_clauses,
    reject_unreachable_wildcard, step_dispatch_pattern_label, transition_payload_guard_for_case,
    validate_process_wildcard_compatibility, wildcard_payload_guard_for_case,
};
pub(in crate::language::checker) use preadmit::collect_concrete_state_payload_domains;
use preadmit::preadmit_concrete_step_state_values;
use state_match::expand_state_match_step_clause_group;

pub(super) fn check_step_clauses<'a, 'state>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'state>,
    types: &mut CheckedTypeInterner<'state>,
) -> Result<Vec<StepClause<'a>>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut explicit_clauses = vec![Vec::new(); msg_enum.variants.len()];
    let mut wildcard_clause = None;
    let mut dispatch_style = None;
    let mut match_body_seen = false;

    preadmit_concrete_step_state_values(
        module,
        process,
        process_id,
        semantic_index,
        state_space,
        types,
    )?;

    for step in &process.steps {
        let Some(body) = &step.body else {
            return Err(Error::new("step must have a body for buildable source"));
        };
        match check_step_shape(module, process, process_id, semantic_index, step)? {
            StepDispatchForm::ParameterPattern(pattern) => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::ParameterPattern,
                )?;
                let FunctionBody::Block(body) = body else {
                    return Err(Error::new("step parameter pattern must use a block body"));
                };
                insert_step_body_clause(
                    module,
                    semantic_index,
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::Block(body),
                        payload_params: Vec::new(),
                        payload_guard: None,
                    },
                    StepClauseInsertMode::Signature,
                )?;
            }
            StepDispatchForm::BodyMatch => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::BodyMatch,
                )?;
                if match_body_seen {
                    return Err(Error::new(format!(
                        "process {} declares duplicate match step body",
                        process.name
                    )));
                }
                match_body_seen = true;
                let FunctionBody::Match(match_body) = body else {
                    return Err(Error::new("match step must use a match body"));
                };
                for arm in &match_body.arms {
                    let pattern = check_step_pattern(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        &arm.pattern,
                    )?;
                    insert_step_body_clause(
                        module,
                        semantic_index,
                        process,
                        &msg_enum.variants,
                        &mut explicit_clauses,
                        &mut wildcard_clause,
                        pattern,
                        StepBodyClause {
                            step,
                            body: StepBodySource::Block(&arm.body),
                            payload_params: Vec::new(),
                            payload_guard: None,
                        },
                        StepClauseInsertMode::MatchBody,
                    )?;
                }
            }
            StepDispatchForm::StateMatch(pattern) => {
                set_step_dispatch_style(
                    process,
                    &mut dispatch_style,
                    StepDispatchStyle::ParameterPattern,
                )?;
                let FunctionBody::Match(match_body) = body else {
                    return Err(Error::new("state match step must use a match body"));
                };
                insert_step_body_clause(
                    module,
                    semantic_index,
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::StateMatch(match_body),
                        payload_params: Vec::new(),
                        payload_guard: None,
                    },
                    StepClauseInsertMode::StateMatch,
                )?;
            }
        }
    }

    validate_process_wildcard_compatibility(
        process,
        &msg_enum.variants,
        &explicit_clauses,
        wildcard_clause.as_ref(),
    )?;
    let concrete_message_cases = concrete_step_message_cases(
        process_id,
        &msg_enum.variants,
        message_cases,
        &explicit_clauses,
    )?;
    reject_unreachable_wildcard(
        module,
        semantic_index,
        process,
        wildcard_clause.as_ref(),
        &explicit_clauses,
        &concrete_message_cases,
    )?;
    let pattern_label = step_dispatch_pattern_label(dispatch_style);
    let mut clauses = Vec::with_capacity(concrete_message_cases.len());
    let mut state_match_cases = Vec::new();
    for concrete_case in &concrete_message_cases {
        let variant_id = concrete_case.variant;
        let message_variant = &msg_enum.variants[variant_id.index()];
        let matching_explicit = matching_step_body_clauses(
            module,
            semantic_index,
            &explicit_clauses[variant_id.index()],
            concrete_case.payload.as_ref(),
        )?;
        if matching_explicit.count > 1 {
            return Err(Error::new(format!(
                "process {} has overlapping step patterns for message {} payload {}",
                process.name,
                message_variant.name,
                concrete_case
                    .payload
                    .as_ref()
                    .map(CheckedPayloadValue::label)
                    .unwrap_or("<unknown>")
            )));
        }
        let (clause, payload_guard) = if let Some(clause) = matching_explicit.first {
            let payload_guard =
                transition_payload_guard_for_case(clause, &explicit_clauses, concrete_case);
            (clause, payload_guard)
        } else if let Some(clause) = wildcard_clause.as_ref() {
            let payload_guard = wildcard_payload_guard_for_case(
                process,
                &explicit_clauses,
                clause,
                concrete_case,
                message_variant,
                pattern_label,
            )?;
            (clause, payload_guard)
        } else {
            return Err(Error::new(format!(
                "process {} must declare step pattern for message {}{}",
                process.name,
                message_variant.name,
                concrete_case
                    .payload
                    .as_ref()
                    .map(|payload| format!(" payload {}", payload.label()))
                    .unwrap_or_default()
            )));
        };
        let payload_bindings = clause
            .payload_params
            .iter()
            .map(|param| {
                let payload_ty = message_variant.payload_type.as_ref().ok_or_else(|| {
                    Error::new(format!(
                        "process {} step pattern for message {} binds payload {}, but the message has no payload",
                        process.name, message_variant.name, param.name
                    ))
                })?;
                Ok(StepPayloadBinding {
                    name: param.name.clone(),
                    payload_ty: payload_ty.clone(),
                    ty: param.ty.clone(),
                    checked_payload_ty: types.intern(payload_ty)?,
                    checked_ty: types.intern(&param.ty)?,
                    path: param.path.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let message = message_cases.message_id(process_id, variant_id)?;
        match &clause.body {
            StepBodySource::Block(body) => {
                let mut preadmit_context = StepReturnPreadmitContext {
                    module,
                    process,
                    process_id,
                    semantic_index,
                    message_cases,
                    state_space,
                    types,
                };
                preadmit_step_return_state_value(
                    &mut preadmit_context,
                    &StepReturnInput {
                        variant: variant_id,
                        payload_guard: payload_guard.as_ref(),
                        payload_bindings: &payload_bindings,
                        state_payload_bindings: &[],
                        body,
                    },
                )?;
                clauses.push(StepClause {
                    step: clause.step,
                    variant: variant_id,
                    message,
                    payload_guard,
                    payload_bindings,
                    current_state: None,
                    state_payload_bindings: Vec::new(),
                    body,
                });
            }
            StepBodySource::StateMatch(match_body) => {
                state_match_cases.push(StateMatchStepExpansion {
                    step: clause.step,
                    variant: variant_id,
                    message,
                    payload_guard,
                    payload_bindings,
                    match_body,
                })
            }
        }
    }
    expand_state_match_step_clause_group(
        module,
        process,
        process_id,
        semantic_index,
        message_cases,
        state_space,
        types,
        &state_match_cases,
        &mut clauses,
    )?;
    if explicit_clauses
        .iter()
        .flatten()
        .any(|clause| clause.payload_guard.is_some())
    {
        reject_unreachable_payload_guarded_clauses(
            module,
            semantic_index,
            process,
            &msg_enum.variants,
            &explicit_clauses,
            &concrete_message_cases,
            pattern_label,
        )?;
    }

    Ok(clauses)
}

#[derive(Debug, Clone)]
struct StateMatchStepExpansion<'a> {
    step: &'a Function,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_guard: Option<CheckedPayloadValue>,
    payload_bindings: Vec<StepPayloadBinding>,
    match_body: &'a crate::language::ast::Match,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StateMatchTransitionKey {
    message: CheckedMessageId,
    current_state: CheckedStateId,
    payload_guard: Option<CheckedPayloadGuardKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CheckedPayloadGuardKey {
    ty: CheckedTypeId,
    payload: PayloadDomainKey,
}

impl CheckedPayloadGuardKey {
    fn from_payload(payload: &CheckedPayloadValue) -> Result<Self> {
        Ok(Self {
            ty: payload.ty().id(),
            payload: PayloadDomainKey::from_payload(payload)?,
        })
    }
}

#[derive(Debug, Clone)]
struct StepConcreteMessageCase {
    variant: CheckedMessageVariantId,
    payload: Option<CheckedPayloadValue>,
}

fn concrete_step_message_cases(
    process_id: CheckedProcessId,
    message_variants: &[EnumVariant],
    message_cases: &MessageCaseTable,
    explicit_clauses: &[Vec<StepBodyClause<'_>>],
) -> Result<Vec<StepConcreteMessageCase>> {
    let mut concrete_cases = Vec::new();
    for (variant_index, message_variant) in message_variants.iter().enumerate() {
        let variant = CheckedMessageVariantId::from_index(variant_index)?;
        let needs_payload_sensitive_cases = explicit_clauses[variant_index]
            .iter()
            .any(|clause| clause.payload_guard.is_some());
        if message_variant.payload_type.is_some() && needs_payload_sensitive_cases {
            let payload_values = message_cases.payload_values(process_id, variant)?;
            if payload_values.is_empty() {
                concrete_cases.push(StepConcreteMessageCase {
                    variant,
                    payload: None,
                });
            } else {
                concrete_cases.extend(payload_values.iter().cloned().map(|payload| {
                    StepConcreteMessageCase {
                        variant,
                        payload: Some(payload),
                    }
                }));
            }
        } else {
            concrete_cases.push(StepConcreteMessageCase {
                variant,
                payload: None,
            });
        }
    }
    Ok(concrete_cases)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepClauseInsertMode {
    MatchBody,
    StateMatch,
    Signature,
}

impl StepClauseInsertMode {
    fn pattern_label(self) -> &'static str {
        match self {
            StepClauseInsertMode::MatchBody => "match msg pattern",
            StepClauseInsertMode::StateMatch => "state match step pattern",
            StepClauseInsertMode::Signature => "step pattern",
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_step_body_clause<'a>(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    message_variants: &[EnumVariant],
    explicit_clauses: &mut [Vec<StepBodyClause<'a>>],
    wildcard_clause: &mut Option<StepBodyClause<'a>>,
    pattern: StepPattern,
    mut clause: StepBodyClause<'a>,
    mode: StepClauseInsertMode,
) -> Result<()> {
    match pattern {
        StepPattern::Variant {
            message,
            bindings,
            payload_guard,
        } => {
            clause.payload_params = bindings;
            clause.payload_guard = payload_guard;
            let clauses = &mut explicit_clauses[message.index()];
            if mode == StepClauseInsertMode::Signature
                && clause.payload_guard.is_none()
                && clauses
                    .iter()
                    .any(|existing| existing.payload_guard.is_none())
            {
                return Err(Error::new(format!(
                    "process {} declares duplicate step pattern for message {}",
                    process.name,
                    message_variants[message.index()].name
                )));
            }
            for existing in clauses.iter() {
                if payload_patterns_overlap(
                    semantic_index,
                    clause.payload_guard.as_ref(),
                    existing.payload_guard.as_ref(),
                )? {
                    return Err(Error::new(format!(
                        "process {} {} {} overlaps an earlier pattern for message {}",
                        process.name,
                        mode.pattern_label(),
                        step_pattern_payload_label(
                            module,
                            semantic_index,
                            &message_variants[message.index()],
                            clause.payload_guard.as_ref(),
                        )?,
                        message_variants[message.index()].name
                    )));
                }
            }
            clauses.push(clause);
        }
        StepPattern::Wildcard => {
            if wildcard_clause.replace(clause).is_some() {
                return Err(Error::new(format!(
                    "process {} declares duplicate wildcard step pattern",
                    process.name
                )));
            }
        }
    }
    Ok(())
}

fn step_pattern_payload_label(
    module: &Module,
    semantic_index: &SemanticIndex,
    message_variant: &EnumVariant,
    payload_guard: Option<&PatternPayloadGuard>,
) -> Result<String> {
    match payload_guard {
        Some(payload_guard) => Ok(format!(
            "{}({})",
            message_variant.name,
            payload_guard_label(module, semantic_index, payload_guard)?
        )),
        None => Ok(message_variant.name.to_string()),
    }
}

fn set_step_dispatch_style(
    process: &Process,
    dispatch_style: &mut Option<StepDispatchStyle>,
    next: StepDispatchStyle,
) -> Result<()> {
    if let Some(existing) = dispatch_style {
        if *existing != next {
            return Err(Error::new(format!(
                "process {} cannot mix match step bodies with step parameter patterns",
                process.name
            )));
        }
    } else {
        *dispatch_style = Some(next);
    }
    Ok(())
}
