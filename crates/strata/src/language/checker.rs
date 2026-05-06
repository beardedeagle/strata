mod outputs;
mod state_space;
mod static_validation;
mod symbols;

use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
};

use super::ast::{
    Determinism, Effect, EnumVariant, Function, FunctionBlock, FunctionParam, Identifier, Module,
    Param, Process, ReturnExpr, SignaturePattern, Statement, TypeRef, ValueExpr,
};
use super::checked::{
    CheckedAction, CheckedMessageCase, CheckedMessageId, CheckedMessageVariantId, CheckedNextState,
    CheckedPayloadValue, CheckedProcess, CheckedProcessId, CheckedProcessParts, CheckedProcessRef,
    CheckedProcessRefId, CheckedProgram, CheckedProgramParts, CheckedStateId, CheckedStepResult,
    CheckedTransition, CheckedTransitionParts, CheckedValueTemplate,
};
use super::diagnostic::{Error, Result};
use super::{PROCESS_REF_TYPE, PROC_RESULT_TYPE};
use outputs::OutputPool;
use state_space::{
    canonical_source_value_with_bindings, checked_value_template_with_binding,
    source_value_uses_binding, StateSpace, ValueBinding, ValueTemplateBinding,
};
use static_validation::validate_action_references;
use symbols::SemanticIndex;

const STEP_STATE_PARAMETER_NAME: &str = "state";

#[derive(Debug, Clone, Copy)]
struct ProcessRefBinding {
    id: CheckedProcessRefId,
    target: CheckedProcessId,
}

struct ProcessCheckContext<'a> {
    module: &'a Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &'a SemanticIndex,
    message_cases: &'a MessageCaseTable,
}

struct StepCheckContext<'a> {
    module: &'a Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &'a SemanticIndex,
    process_ref_index: &'a BTreeMap<Identifier, ProcessRefBinding>,
    message_cases: &'a MessageCaseTable,
}

struct ProcessRefCollectionContext<'a> {
    process: &'a Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &'a SemanticIndex,
}

#[derive(Debug, Clone)]
struct StepBodyClause<'a> {
    step: &'a Function,
    body: &'a FunctionBlock,
    payload_param: Option<StepPayloadParam>,
}

#[derive(Debug, Clone)]
struct StepClause<'a> {
    step: &'a Function,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_binding: Option<StepPayloadBinding>,
    body: &'a FunctionBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepSignaturePattern {
    Variant {
        message: CheckedMessageVariantId,
        binding: Option<StepPayloadParam>,
    },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepPayloadParam {
    name: Identifier,
    ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepPayloadBinding {
    name: Identifier,
    ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredMessageCase {
    variant: CheckedMessageVariantId,
    payload: Option<CheckedPayloadValue>,
}

impl DiscoveredMessageCase {
    fn new(variant: CheckedMessageVariantId, payload: Option<CheckedPayloadValue>) -> Self {
        Self { variant, payload }
    }

    fn variant(&self) -> CheckedMessageVariantId {
        self.variant
    }

    fn payload(&self) -> Option<&CheckedPayloadValue> {
        self.payload.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MessageCaseKey {
    process: CheckedProcessId,
    variant: CheckedMessageVariantId,
}

#[derive(Debug, Clone)]
struct MessageCaseTable {
    cases_by_process: Vec<Vec<CheckedMessageCase>>,
    payloads_by_process: Vec<Vec<Vec<CheckedPayloadValue>>>,
    ids_by_key: BTreeMap<MessageCaseKey, CheckedMessageId>,
}

impl MessageCaseTable {
    fn build(
        module: &Module,
        entry_process: CheckedProcessId,
        semantic_index: &SemanticIndex,
    ) -> Result<Self> {
        reject_payload_entry_message(module, entry_process, semantic_index)?;
        let mut builders = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                MessageCaseBuilder::new(module, semantic_index, process, process_id)
            })
            .collect::<Result<Vec<_>>>()?;
        let process_ref_targets = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_message_case_process_refs(process, process_id, semantic_index)
            })
            .collect::<Result<Vec<_>>>()?;
        let explicit_step_variants = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_explicit_step_variants(module, process, process_id, semantic_index)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut iteration_count = 0usize;
        let max_iterations = MAX_MESSAGE_VARIANTS_PER_PROCESS.saturating_mul(builders.len());
        loop {
            let case_snapshots = builders
                .iter()
                .map(MessageCaseBuilder::current_cases)
                .collect::<Result<Vec<_>>>()?;
            let mut changed = false;
            for (process_index, process) in module.processes.iter().enumerate() {
                let process_id = CheckedProcessId::from_index(process_index)?;
                let sender_cases = &case_snapshots[process_index];
                for step in &process.steps {
                    let Some(body) = &step.body else {
                        continue;
                    };
                    let Some(pattern) = resolve_step_message_pattern(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        step,
                    )?
                    else {
                        continue;
                    };
                    for sender_case in matching_message_cases(
                        sender_cases,
                        &pattern,
                        &explicit_step_variants[process_index],
                    ) {
                        let bindings = payload_value_bindings(&pattern, sender_case);
                        for statement in &body.statements {
                            let Statement::Send {
                                target,
                                message,
                                payload,
                            } = statement
                            else {
                                continue;
                            };
                            let target_process_id = process_ref_targets[process_index]
                                .get(target)
                                .ok_or_else(|| {
                                    Error::new(format!(
                                        "process {} sends to undeclared process reference {}",
                                        process.name, target
                                    ))
                                })?;
                            let target_variant = semantic_index.message_id_for_process(
                                module,
                                process.name.as_str(),
                                *target_process_id,
                                message,
                            )?;
                            let builder =
                                builders.get_mut(target_process_id.index()).ok_or_else(|| {
                                    Error::new(format!(
                                        "process id {} is not declared",
                                        target_process_id.as_u32()
                                    ))
                                })?;
                            changed |= builder.add_payload_case(
                                module,
                                semantic_index,
                                *target_process_id,
                                target_variant,
                                payload.as_ref(),
                                &bindings,
                            )?;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
            iteration_count = iteration_count
                .checked_add(1)
                .ok_or_else(|| Error::new("message case discovery iteration count overflowed"))?;
            if iteration_count > max_iterations {
                return Err(Error::new(
                    "message case discovery did not converge within the message variant limit",
                ));
            }
        }

        let mut cases_by_process = Vec::with_capacity(builders.len());
        let mut payloads_by_process = Vec::with_capacity(builders.len());
        let mut ids_by_key = BTreeMap::new();
        for builder in builders {
            let process_id = builder.process_id;
            let cases = builder.logical_cases()?;
            for (message_index, case) in cases.iter().enumerate() {
                let key = MessageCaseKey {
                    process: process_id,
                    variant: case.variant(),
                };
                if ids_by_key
                    .insert(key, CheckedMessageId::from_index(message_index)?)
                    .is_some()
                {
                    return Err(Error::new(format!(
                        "process id {} declares duplicate message case",
                        process_id.as_u32()
                    )));
                }
            }
            payloads_by_process.push(builder.payload_domains()?);
            cases_by_process.push(cases);
        }

        Ok(Self {
            cases_by_process,
            payloads_by_process,
            ids_by_key,
        })
    }

    fn cases_for(&self, process: CheckedProcessId) -> Result<&[CheckedMessageCase]> {
        self.cases_by_process
            .get(process.index())
            .map(Vec::as_slice)
            .ok_or_else(|| Error::new(format!("process id {} is not declared", process.as_u32())))
    }

    fn message_id(
        &self,
        process: CheckedProcessId,
        variant: CheckedMessageVariantId,
    ) -> Result<CheckedMessageId> {
        let key = MessageCaseKey { process, variant };
        self.ids_by_key.get(&key).copied().ok_or_else(|| {
            Error::new(format!(
                "process id {} has no message case for message id {}",
                process.as_u32(),
                variant.as_u32()
            ))
        })
    }

    fn payload_values(
        &self,
        process: CheckedProcessId,
        variant: CheckedMessageVariantId,
    ) -> Result<&[CheckedPayloadValue]> {
        self.payloads_by_process
            .get(process.index())
            .and_then(|process_payloads| process_payloads.get(variant.index()))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                Error::new(format!(
                    "process id {} has no payload domain for message id {}",
                    process.as_u32(),
                    variant.as_u32()
                ))
            })
    }
}

struct MessageCaseBuilder<'a> {
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    process: &'a Process,
    process_id: CheckedProcessId,
    payload_cases: BTreeMap<CheckedMessageVariantId, BTreeMap<String, CheckedPayloadValue>>,
}

impl<'a> MessageCaseBuilder<'a> {
    fn new(
        module: &'a Module,
        semantic_index: &'a SemanticIndex,
        process: &'a Process,
        process_id: CheckedProcessId,
    ) -> Result<Self> {
        let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
        let mut payload_cases = BTreeMap::new();
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if variant.payload_type.is_some() {
                payload_cases.insert(variant_id, BTreeMap::new());
            }
        }
        Ok(Self {
            module,
            semantic_index,
            process,
            process_id,
            payload_cases,
        })
    }

    fn add_payload_case(
        &mut self,
        module: &Module,
        semantic_index: &SemanticIndex,
        process_id: CheckedProcessId,
        variant_id: CheckedMessageVariantId,
        payload: Option<&ValueExpr>,
        bindings: &[ValueBinding<'_>],
    ) -> Result<bool> {
        let variant = semantic_index.message_variant(module, process_id, variant_id)?;
        match (&variant.payload_type, payload) {
            (None, None) => Ok(false),
            (None, Some(_)) => Err(Error::new(format!(
                "message {} does not accept a payload",
                variant.name
            ))),
            (Some(_), None) => Err(Error::new(format!(
                "message {} requires a payload",
                variant.name
            ))),
            (Some(payload_type), Some(payload)) => {
                let label = canonical_source_value_with_bindings(
                    module,
                    semantic_index,
                    payload_type,
                    payload,
                    bindings,
                )?;
                Ok(self.insert_payload_case(variant_id, payload_type, label))
            }
        }
    }

    fn insert_payload_case(
        &mut self,
        variant_id: CheckedMessageVariantId,
        payload_type: &TypeRef,
        label: String,
    ) -> bool {
        let payloads = self.payload_cases.entry(variant_id).or_default();
        if payloads.contains_key(&label) {
            return false;
        }
        payloads.insert(
            label.clone(),
            CheckedPayloadValue::new(payload_type.clone(), label),
        );
        true
    }

    fn current_cases(&self) -> Result<Vec<DiscoveredMessageCase>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        let mut cases = Vec::new();
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if variant.payload_type.is_some() {
                if let Some(payloads) = self.payload_cases.get(&variant_id) {
                    for payload in payloads.values() {
                        cases.push(DiscoveredMessageCase::new(
                            variant_id,
                            Some(payload.clone()),
                        ));
                    }
                }
            } else {
                cases.push(DiscoveredMessageCase::new(variant_id, None));
            }
        }
        Ok(cases)
    }

    fn logical_cases(&self) -> Result<Vec<CheckedMessageCase>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        msg_enum
            .variants
            .iter()
            .enumerate()
            .map(|(variant_index, variant)| {
                CheckedMessageCase::new(
                    variant.name.to_string(),
                    CheckedMessageVariantId::from_index(variant_index)?,
                    variant.payload_type.clone(),
                )
            })
            .collect()
    }

    fn payload_domains(&self) -> Result<Vec<Vec<CheckedPayloadValue>>> {
        let msg_enum = self
            .semantic_index
            .enum_decl(self.module, &self.process.msg_type)?;
        let mut payloads_by_variant = vec![Vec::new(); msg_enum.variants.len()];
        for (variant_index, variant) in msg_enum.variants.iter().enumerate() {
            if variant.payload_type.is_none() {
                continue;
            }
            let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
            if let Some(payloads) = self.payload_cases.get(&variant_id) {
                payloads_by_variant[variant_index] = payloads.values().cloned().collect();
            }
        }
        Ok(payloads_by_variant)
    }
}

pub fn check_module(module: Module) -> Result<CheckedProgram> {
    if module.records.is_empty() {
        return Err(Error::new("expected at least one record declaration"));
    }
    if module.enums.is_empty() {
        return Err(Error::new("expected at least one enum declaration"));
    }
    if module.processes.is_empty() {
        return Err(Error::new("expected at least one process declaration"));
    }
    validate_count(
        "process_count",
        module.processes.len(),
        1,
        MAX_PROCESS_COUNT,
    )?;

    let semantic_index = SemanticIndex::build(&module)?;
    let entry_process = semantic_index
        .process_id_by_name("Main")
        .map_err(|_| Error::new("entry process Main is not declared"))?;
    validate_process_declarations_before_message_cases(&module, &semantic_index)?;
    let message_cases = MessageCaseTable::build(&module, entry_process, &semantic_index)?;
    let mut outputs = OutputPool::new();
    let mut checked_processes = Vec::with_capacity(module.processes.len());
    for (index, process) in module.processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(index)?;
        checked_processes.push(check_process(
            &module,
            process,
            process_id,
            entry_process,
            &semantic_index,
            &message_cases,
            &mut outputs,
        )?);
    }

    let entry_message = CheckedMessageId::from_index(0)?;
    validate_action_references(&checked_processes, &entry_process, &entry_message)?;

    let entry_process_definition = checked_processes
        .get(entry_process.index())
        .ok_or_else(|| Error::new("entry process id is not defined"))?;
    if entry_process_definition.message_cases().is_empty() {
        return Err(Error::new(format!(
            "entry process {} has no messages",
            entry_process_definition.debug_name()
        )));
    }

    Ok(CheckedProgram::new(CheckedProgramParts {
        module,
        entry_process,
        entry_message,
        outputs: outputs.into_values(),
        processes: checked_processes,
    }))
}

fn validate_process_declarations_before_message_cases(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    for (process_index, process) in module.processes.iter().enumerate() {
        validate_count(
            &format!("process {} mailbox_bound", process.name),
            process.mailbox_bound,
            1,
            MAX_MAILBOX_BOUND,
        )?;
        let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
        if msg_enum.variants.is_empty() {
            return Err(Error::new(format!(
                "enum {} must declare at least one variant",
                msg_enum.name
            )));
        }
        validate_count(
            &format!("process {} message_count", process.name),
            msg_enum.variants.len(),
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        let _ = StateSpace::new(module, semantic_index, process)?;
        let process_id = CheckedProcessId::from_index(process_index)?;
        for step in &process.steps {
            check_step_signature(module, process, process_id, semantic_index, step)?;
        }
    }
    Ok(())
}

fn check_process(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    outputs: &mut OutputPool,
) -> Result<CheckedProcess> {
    validate_count(
        &format!("process {} mailbox_bound", process.name),
        process.mailbox_bound,
        1,
        MAX_MAILBOX_BOUND,
    )?;

    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    if msg_enum.variants.is_empty() {
        return Err(Error::new(format!(
            "enum {} must declare at least one variant",
            msg_enum.name
        )));
    }
    validate_count(
        &format!("process {} message_count", process.name),
        msg_enum.variants.len(),
        1,
        MAX_MESSAGE_VARIANTS_PER_PROCESS,
    )?;
    validate_count(
        &format!("process {} message_case_count", process.name),
        message_cases.cases_for(process_id)?.len(),
        1,
        MAX_MESSAGE_VARIANTS_PER_PROCESS,
    )?;

    let mut state_space = StateSpace::new(module, semantic_index, process)?;
    let init_state = check_init(semantic_index, process, &mut state_space)?;
    let process_context = ProcessCheckContext {
        module,
        process,
        process_id,
        entry_process,
        semantic_index,
        message_cases,
    };
    let (process_refs, transitions) = check_step(&process_context, &mut state_space, outputs)?;
    let state_values = state_space.into_values()?;

    Ok(CheckedProcess::new(CheckedProcessParts {
        debug_name: process.name.clone(),
        state_type: process.state_type.clone(),
        state_values,
        message_type: process.msg_type.clone(),
        message_cases: message_cases.cases_for(process_id)?.to_vec(),
        process_refs,
        mailbox_bound: process.mailbox_bound,
        init_state,
        transitions,
    }))
}

fn check_init(
    semantic_index: &SemanticIndex,
    process: &Process,
    state_space: &mut StateSpace<'_>,
) -> Result<CheckedStateId> {
    let init = &process.init;
    if !init.params.is_empty() {
        return Err(Error::new("init must declare no parameters"));
    }
    if !semantic_index.same_type(&init.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "init returns {}, expected {}",
            init.return_type, process.state_type
        )));
    }
    if !init.may.is_empty() {
        return Err(Error::new("init may-behaviors must be empty"));
    }
    if init.determinism != Determinism::Det {
        return Err(Error::new("init must be deterministic"));
    }

    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    if !body.statements.is_empty() {
        return Err(Error::new(
            "init body must not perform statements in this slice",
        ));
    }
    validate_effects("init", &init.effects, BTreeSet::new())?;

    let ReturnExpr::Value(value) = &body.returns else {
        return Err(Error::new(format!(
            "init body must return a value of {}",
            process.state_type
        )));
    };
    state_space.resolve_state_value(semantic_index, value)
}

fn check_step(
    context: &ProcessCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
) -> Result<(Vec<CheckedProcessRef>, Vec<CheckedTransition>)> {
    let step_clauses = check_step_clauses(
        context.module,
        context.process,
        context.process_id,
        context.semantic_index,
        context.message_cases,
    )?;
    let (process_refs, process_ref_index) = collect_process_refs(
        context.process,
        context.process_id,
        context.entry_process,
        context.semantic_index,
        &step_clauses,
    )?;
    let step_context = StepCheckContext {
        module: context.module,
        process: context.process,
        process_id: context.process_id,
        semantic_index: context.semantic_index,
        process_ref_index: &process_ref_index,
        message_cases: context.message_cases,
    };

    let mut transitions = Vec::with_capacity(step_clauses.len());
    for clause in step_clauses {
        let transition = check_step_transition(
            &step_context,
            state_space,
            outputs,
            clause.variant,
            clause.message,
            clause.payload_binding.as_ref(),
            clause.body,
        )?;
        let used_effects =
            transition
                .actions()
                .iter()
                .fold(BTreeSet::new(), |mut effects, action| {
                    effects.insert(action.effect());
                    effects
                });
        validate_effects("step", &clause.step.effects, used_effects)?;
        transitions.push(transition);
    }

    let action_count = total_action_count(&transitions)?;
    validate_count(
        &format!("process {} action_count", context.process.name),
        action_count,
        0,
        MAX_ACTIONS_PER_PROCESS,
    )?;

    Ok((process_refs, transitions))
}

fn check_step_clauses<'a>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
) -> Result<Vec<StepClause<'a>>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut explicit_clauses = vec![None; msg_enum.variants.len()];
    let mut wildcard_clause = None;

    for step in &process.steps {
        let pattern = check_step_signature(module, process, process_id, semantic_index, step)?;
        let Some(body) = &step.body else {
            return Err(Error::new("step must have a body for buildable source"));
        };
        match pattern {
            StepSignaturePattern::Variant { message, binding } => {
                let clause = StepBodyClause {
                    step,
                    body,
                    payload_param: binding,
                };
                if explicit_clauses[message.index()].replace(clause).is_some() {
                    return Err(Error::new(format!(
                        "process {} declares duplicate step pattern for message {}",
                        process.name,
                        msg_enum.variants[message.index()].name
                    )));
                }
            }
            StepSignaturePattern::Wildcard => {
                let clause = StepBodyClause {
                    step,
                    body,
                    payload_param: None,
                };
                if wildcard_clause.replace(clause).is_some() {
                    return Err(Error::new(format!(
                        "process {} declares duplicate wildcard step pattern",
                        process.name
                    )));
                }
            }
        }
    }

    if wildcard_clause.is_some() && explicit_clauses.iter().all(Option::is_some) {
        return Err(Error::new(format!(
            "process {} wildcard step pattern is unreachable",
            process.name
        )));
    }

    let message_cases_for_process = message_cases.cases_for(process_id)?;
    let mut clauses = Vec::with_capacity(message_cases_for_process.len());
    for (index, message_variant) in msg_enum.variants.iter().enumerate() {
        let Some(clause) = explicit_clauses[index]
            .as_ref()
            .or(wildcard_clause.as_ref())
        else {
            return Err(Error::new(format!(
                "process {} must declare step pattern for message {}",
                process.name, message_variant.name
            )));
        };
        let variant_id = CheckedMessageVariantId::from_index(index)?;
        let case = message_cases_for_process
            .iter()
            .find(|case| case.variant() == variant_id)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} has no checked message case for message {}",
                    process.name, message_variant.name
                ))
            })?;
        let payload_binding = match (&clause.payload_param, case.payload_type()) {
            (Some(param), Some(_)) => Some(StepPayloadBinding {
                name: param.name.clone(),
                ty: param.ty.clone(),
            }),
            _ => None,
        };
        let message = message_cases.message_id(process_id, variant_id)?;
        clauses.push(StepClause {
            step: clause.step,
            variant: variant_id,
            message,
            payload_binding,
            body: clause.body,
        });
    }

    Ok(clauses)
}

fn collect_explicit_step_variants(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeSet<CheckedMessageVariantId>> {
    let mut variants = BTreeSet::new();
    for step in &process.steps {
        if step.body.is_none() {
            continue;
        }
        let Some(pattern) =
            resolve_step_message_pattern(module, process, process_id, semantic_index, step)?
        else {
            continue;
        };
        if let StepSignaturePattern::Variant { message, .. } = pattern {
            variants.insert(message);
        }
    }
    Ok(variants)
}

fn matching_message_cases<'a>(
    cases: &'a [DiscoveredMessageCase],
    pattern: &StepSignaturePattern,
    explicit_variants: &BTreeSet<CheckedMessageVariantId>,
) -> Vec<&'a DiscoveredMessageCase> {
    cases
        .iter()
        .filter(|case| match pattern {
            StepSignaturePattern::Variant { message, .. } => case.variant() == *message,
            StepSignaturePattern::Wildcard => !explicit_variants.contains(&case.variant()),
        })
        .collect()
}

fn payload_value_bindings<'a>(
    pattern: &'a StepSignaturePattern,
    case: &'a DiscoveredMessageCase,
) -> Vec<ValueBinding<'a>> {
    match (pattern, case.payload()) {
        (
            StepSignaturePattern::Variant {
                binding: Some(param),
                ..
            },
            Some(payload),
        ) => vec![ValueBinding {
            name: &param.name,
            ty: &param.ty,
            label: payload.label(),
        }],
        _ => Vec::new(),
    }
}

fn resolve_step_message_pattern(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &Function,
) -> Result<Option<StepSignaturePattern>> {
    let Some(FunctionParam::Pattern(message_pattern)) = step.params.get(1) else {
        return Ok(None);
    };
    match message_pattern {
        SignaturePattern::Variant { name, binding } => {
            let message = semantic_index.message_id_for_step_pattern(module, process_id, name)?;
            let variant = semantic_index.message_variant(module, process_id, message)?;
            let payload_param =
                check_step_payload_pattern(process, semantic_index, variant, binding.as_ref())?;
            Ok(Some(StepSignaturePattern::Variant {
                message,
                binding: payload_param,
            }))
        }
        SignaturePattern::Wildcard => Ok(Some(StepSignaturePattern::Wildcard)),
    }
}

fn check_step_signature(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &Function,
) -> Result<StepSignaturePattern> {
    if step.params.len() != 2 {
        return Err(Error::new(
            "step must declare state parameter and message pattern",
        ));
    }
    let FunctionParam::Binding(state_param) = &step.params[0] else {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    };
    if state_param.name.as_str() != STEP_STATE_PARAMETER_NAME
        || !semantic_index.same_type(&state_param.ty, &process.state_type)
    {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    if !matches!(&step.params[1], FunctionParam::Pattern(_)) {
        return Err(Error::new(
            "step second parameter must be a message variant pattern or wildcard pattern",
        ));
    }

    if !semantic_index.is_proc_result_of(&step.return_type, &process.state_type) {
        return Err(Error::new(format!(
            "step returns {}, expected {}",
            step.return_type,
            format_args!("{PROC_RESULT_TYPE}<{}>", process.state_type)
        )));
    }
    if !step.may.is_empty() {
        return Err(Error::new("step may-behaviors must be empty"));
    }
    if step.determinism != Determinism::Det {
        return Err(Error::new("step must be deterministic"));
    }

    resolve_step_message_pattern(module, process, process_id, semantic_index, step)?.ok_or_else(
        || {
            Error::new(
                "step second parameter must be a message variant pattern or wildcard pattern",
            )
        },
    )
}

fn check_step_payload_pattern(
    process: &Process,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    binding: Option<&Param>,
) -> Result<Option<StepPayloadParam>> {
    match (&variant.payload_type, binding) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(Error::new(format!(
            "process {} step pattern message {} does not carry a payload",
            process.name, variant.name
        ))),
        (Some(_), None) => Ok(None),
        (Some(payload_type), Some(binding)) => {
            validate_payload_binding_name(process, semantic_index, &binding.name)?;
            if !semantic_index.same_type(&binding.ty, payload_type) {
                return Err(Error::new(format!(
                    "process {} step pattern payload {} has type {}, expected {}",
                    process.name, binding.name, binding.ty, payload_type
                )));
            }
            Ok(Some(StepPayloadParam {
                name: binding.name.clone(),
                ty: binding.ty.clone(),
            }))
        }
    }
}

fn validate_payload_binding_name(
    process: &Process,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} payload binding {} conflicts with a step parameter name",
            process.name, binding
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "process {} payload binding {} conflicts with a process declaration",
            process.name, binding
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "process {} payload binding {} conflicts with a declared type or value constructor",
            process.name, binding
        )));
    }
    Ok(())
}

fn collect_process_refs(
    process: &Process,
    process_id: CheckedProcessId,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step_clauses: &[StepClause<'_>],
) -> Result<(
    Vec<CheckedProcessRef>,
    BTreeMap<Identifier, ProcessRefBinding>,
)> {
    let mut process_refs = Vec::new();
    let mut process_ref_index = BTreeMap::new();
    let context = ProcessRefCollectionContext {
        process,
        process_id,
        entry_process,
        semantic_index,
    };
    for clause in step_clauses {
        collect_process_refs_from_block(
            &context,
            clause.body,
            clause.payload_binding.as_ref(),
            &mut process_refs,
            &mut process_ref_index,
        )?;
    }
    Ok((process_refs, process_ref_index))
}

fn collect_message_case_process_refs(
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeMap<Identifier, CheckedProcessId>> {
    let mut refs = BTreeMap::new();
    for step in &process.steps {
        let Some(body) = &step.body else {
            continue;
        };
        for statement in &body.statements {
            let Statement::LetProcessRef { name, ty, target } = statement else {
                continue;
            };
            validate_process_ref_name(process, semantic_index, name)?;
            let annotated_target = process_ref_type_target(process, semantic_index, name, ty)?;
            let target_id = semantic_index.process_id(target)?;
            if annotated_target != target_id {
                return Err(Error::new(format!(
                    "process {} process reference {} has type {ty} but spawns {}",
                    process.name, name, target
                )));
            }
            if target_id == process_id {
                return Err(Error::new(format!(
                    "process {} spawns itself, which is not supported",
                    process.name
                )));
            }
            let existing = refs.insert(name.clone(), target_id);
            if existing.is_some_and(|existing| existing != target_id) {
                return Err(Error::new(format!(
                    "process {} process reference {} is bound to multiple process definitions",
                    process.name, name
                )));
            }
        }
    }
    Ok(refs)
}

fn reject_payload_entry_message(
    module: &Module,
    entry_process: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let entry = module.processes.get(entry_process.index()).ok_or_else(|| {
        Error::new(format!(
            "entry process id {} is not declared",
            entry_process.as_u32()
        ))
    })?;
    let msg_enum = semantic_index.enum_decl(module, &entry.msg_type)?;
    let Some(first_message) = msg_enum.variants.first() else {
        return Ok(());
    };
    if first_message.payload_type.is_some() {
        return Err(Error::new(format!(
            "entry message {} must not require a payload in this source slice",
            first_message.name
        )));
    }
    Ok(())
}

fn collect_process_refs_from_block(
    context: &ProcessRefCollectionContext<'_>,
    block: &FunctionBlock,
    payload_binding: Option<&StepPayloadBinding>,
    process_refs: &mut Vec<CheckedProcessRef>,
    process_ref_index: &mut BTreeMap<Identifier, ProcessRefBinding>,
) -> Result<()> {
    for statement in &block.statements {
        let Statement::LetProcessRef { name, ty, target } = statement else {
            continue;
        };
        if let Some(binding) = payload_binding {
            if binding.name == *name {
                return Err(Error::new(format!(
                    "process {} process reference {} conflicts with payload binding",
                    context.process.name, name
                )));
            }
        }
        validate_process_ref_name(context.process, context.semantic_index, name)?;
        let annotated_target =
            process_ref_type_target(context.process, context.semantic_index, name, ty)?;
        let target_id = context.semantic_index.process_id(target)?;
        if annotated_target != target_id {
            return Err(Error::new(format!(
                "process {} process reference {} has type {ty} but spawns {}",
                context.process.name, name, target
            )));
        }
        if target_id == context.entry_process {
            return Err(Error::new(format!(
                "process {} spawns entry process {}, which is already started",
                context.process.name, target
            )));
        }
        if target_id == context.process_id {
            return Err(Error::new(format!(
                "process {} spawns itself, which is not supported",
                context.process.name
            )));
        }
        if let Some(existing) = process_ref_index.get(name) {
            if existing.target != target_id {
                return Err(Error::new(format!(
                    "process {} process reference {} is bound to multiple process definitions",
                    context.process.name, name
                )));
            }
            continue;
        }
        let process_ref_id = CheckedProcessRefId::from_index(process_refs.len())?;
        process_refs.push(CheckedProcessRef::new(name.clone(), target_id));
        process_ref_index.insert(
            name.clone(),
            ProcessRefBinding {
                id: process_ref_id,
                target: target_id,
            },
        );
    }
    Ok(())
}

fn check_step_transition(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_binding: Option<&StepPayloadBinding>,
    block: &FunctionBlock,
) -> Result<CheckedTransition> {
    let payload_template_binding = payload_binding.map(|binding| ValueTemplateBinding {
        name: &binding.name,
        ty: &binding.ty,
    });
    let mut actions = Vec::with_capacity(block.statements.len());
    for statement in &block.statements {
        match statement {
            Statement::Emit(text) => {
                actions.push(CheckedAction::Emit {
                    output: outputs.intern(text.as_str())?,
                });
            }
            Statement::LetProcessRef { name, target, .. } => {
                let binding = context.process_ref_index.get(name).ok_or_else(|| {
                    Error::new(format!(
                        "process {} process reference {} was not resolved",
                        context.process.name, name
                    ))
                })?;
                actions.push(CheckedAction::Spawn {
                    target: context.semantic_index.process_id(target)?,
                    process_ref: binding.id,
                });
            }
            Statement::Send {
                target,
                message,
                payload,
            } => {
                let binding = context.process_ref_index.get(target).ok_or_else(|| {
                    Error::new(format!(
                        "process {} sends to undeclared process reference {}",
                        context.process.name, target
                    ))
                })?;
                let message_id = resolve_send_message_case(
                    context,
                    binding.target,
                    message,
                    payload.as_ref(),
                    payload_template_binding.as_ref(),
                )?;
                actions.push(CheckedAction::Send {
                    target: binding.id,
                    message: message_id.message,
                    payload: message_id.payload,
                });
            }
        }
    }

    let (step_result, state_arg) = match &block.returns {
        ReturnExpr::Call { name, arg } if name.as_str() == "Stop" => (CheckedStepResult::Stop, arg),
        ReturnExpr::Call { name, arg } if name.as_str() == "Continue" => {
            (CheckedStepResult::Continue, arg)
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(<state value>) or Continue(<state value>)",
            ))
        }
    };
    let next_state = if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        CheckedNextState::Current
    } else if let Some(binding) = payload_binding {
        if source_value_uses_binding(state_arg, &binding.name) {
            let template = checked_value_template_with_binding(
                context.module,
                context.semantic_index,
                &context.process.state_type,
                state_arg,
                payload_template_binding.as_ref(),
            )?;
            populate_payload_template_state_values(
                context,
                state_space,
                variant,
                state_arg,
                binding,
            )?;
            CheckedNextState::Template(template)
        } else {
            CheckedNextState::Value(
                state_space.resolve_state_value(context.semantic_index, state_arg)?,
            )
        }
    } else {
        CheckedNextState::Value(state_space.resolve_state_value(context.semantic_index, state_arg)?)
    };

    Ok(CheckedTransition::new(CheckedTransitionParts {
        message,
        step_result,
        next_state,
        actions,
    }))
}

struct CheckedSendMessage {
    message: CheckedMessageId,
    payload: Option<CheckedValueTemplate>,
}

fn resolve_send_message_case(
    context: &StepCheckContext<'_>,
    target_process: CheckedProcessId,
    message: &Identifier,
    payload: Option<&ValueExpr>,
    binding: Option<&ValueTemplateBinding<'_>>,
) -> Result<CheckedSendMessage> {
    let variant = context.semantic_index.message_id_for_process(
        context.module,
        context.process.name.as_str(),
        target_process,
        message,
    )?;
    let variant_decl =
        context
            .semantic_index
            .message_variant(context.module, target_process, variant)?;
    let payload = match (&variant_decl.payload_type, payload) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "process {} sends payload to message {}, which does not accept one",
                context.process.name, variant_decl.name
            )))
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "process {} sends message {} without required payload",
                context.process.name, variant_decl.name
            )))
        }
        (Some(payload_type), Some(payload)) => Some(checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            payload_type,
            payload,
            binding,
        )?),
    };
    Ok(CheckedSendMessage {
        message: context.message_cases.message_id(target_process, variant)?,
        payload,
    })
}

fn populate_payload_template_state_values(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    variant: CheckedMessageVariantId,
    state_arg: &ValueExpr,
    binding: &StepPayloadBinding,
) -> Result<()> {
    for payload in context
        .message_cases
        .payload_values(context.process_id, variant)?
    {
        state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            state_arg,
            &[ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: payload.label(),
            }],
        )?;
    }
    Ok(())
}

fn validate_process_ref_name(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_ref: &Identifier,
) -> Result<()> {
    if process_ref.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} process reference {} conflicts with a step parameter name",
            process.name, process_ref
        )));
    }
    if semantic_index.process_id(process_ref).is_ok() {
        return Err(Error::new(format!(
            "process {} process reference {} conflicts with a process declaration",
            process.name, process_ref
        )));
    }
    Ok(())
}

fn process_ref_type_target(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_ref: &Identifier,
    ty: &TypeRef,
) -> Result<CheckedProcessId> {
    let TypeRef::Applied { constructor, args } = ty else {
        return Err(Error::new(format!(
            "process {} process reference {} must be typed as {PROCESS_REF_TYPE}<ProcessName>",
            process.name, process_ref
        )));
    };
    if constructor.as_str() != PROCESS_REF_TYPE || args.len() != 1 {
        return Err(Error::new(format!(
            "process {} process reference {} must be typed as {PROCESS_REF_TYPE}<ProcessName>",
            process.name, process_ref
        )));
    }
    let TypeRef::Named(target) = &args[0] else {
        return Err(Error::new(format!(
            "process {} process reference {} has nested process reference target type {}",
            process.name, process_ref, args[0]
        )));
    };
    semantic_index.process_id(target).map_err(|_| {
        Error::new(format!(
            "process {} process reference {} targets undeclared process {}",
            process.name, process_ref, target
        ))
    })
}

impl CheckedAction {
    fn effect(&self) -> Effect {
        match self {
            Self::Emit { .. } => Effect::Emit,
            Self::Spawn { .. } => Effect::Spawn,
            Self::Send { .. } => Effect::Send,
        }
    }
}

fn total_action_count(transitions: &[CheckedTransition]) -> Result<usize> {
    transitions.iter().try_fold(0usize, |total, transition| {
        total
            .checked_add(transition.actions().len())
            .ok_or_else(|| Error::new("process action_count overflowed"))
    })
}

fn validate_count(field: &str, value: usize, min: usize, max: usize) -> Result<()> {
    if value < min {
        if min == 1 {
            return Err(Error::new(format!("{field} must be greater than zero")));
        }
        return Err(Error::new(format!("{field} must be at least {min}")));
    }
    if value > max {
        return Err(Error::new(format!("{field} must be no greater than {max}")));
    }
    Ok(())
}

fn validate_effects(
    function_name: &str,
    declared_effects: &[Effect],
    used_effects: BTreeSet<Effect>,
) -> Result<()> {
    let mut declared = BTreeSet::new();
    for &effect in declared_effects {
        if !declared.insert(effect) {
            return Err(Error::new(format!(
                "{function_name} declares duplicate effect {effect}"
            )));
        }
    }

    for used in &used_effects {
        if !declared.contains(used) {
            return Err(Error::new(format!(
                "{function_name} uses effect {used} but does not declare it"
            )));
        }
    }
    for declared_effect in &declared {
        if !used_effects.contains(declared_effect) {
            return Err(Error::new(format!(
                "{function_name} declares effect {declared_effect} but does not use it"
            )));
        }
    }
    Ok(())
}
