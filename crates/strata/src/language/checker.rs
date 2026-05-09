mod init;
mod message_cases;
mod outputs;
mod source_functions;
mod state_space;
mod static_validation;
mod steps;
mod symbols;

use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{
    MAX_ACTIONS_PER_PROCESS, MAX_IDENTIFIER_BYTES, MAX_MAILBOX_BOUND,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT, MAX_TYPE_COUNT,
};

use super::ast::{
    Determinism, Effect, Enum, EnumVariant, Function, FunctionBlock, FunctionBody, FunctionParam,
    Identifier, MatchArm, Module, Param, Pattern, Process, RecordValue, RecordValueField,
    ReturnExpr, Statement, TypeRef, ValueExpr,
};
use super::checked::{
    CheckedAction, CheckedMessageCase, CheckedMessageId, CheckedMessageVariantId, CheckedNextState,
    CheckedPayloadValue, CheckedProcess, CheckedProcessId, CheckedProcessParts, CheckedProcessRef,
    CheckedProcessRefId, CheckedProgram, CheckedProgramParts, CheckedSendTarget, CheckedStateId,
    CheckedStepResult, CheckedTransition, CheckedTransitionParts, CheckedTypeId, CheckedTypeKind,
    CheckedTypeRef, CheckedValueTemplate,
};
use super::diagnostic::{Error, Result};
use super::{MAX_VALUE_NESTING, PROC_RESULT_TYPE, PROCESS_REF_TYPE};
use init::check_init;
use message_cases::{DiscoveredMessageCase, MessageCaseTable};
use outputs::OutputPool;
use source_functions::{
    check_source_value_type, resolve_source_value_expr, validate_source_function_declarations,
};
use state_space::{
    StateSpace, ValueBinding, ValueTemplateBinding, ValueTemplateSource,
    canonical_source_value_with_bindings, checked_value_template_with_binding,
    source_value_uses_binding,
};
use static_validation::validate_action_references;
use steps::{check_step, check_step_shape, pattern_binding_subject, validate_pattern_binding_name};
use symbols::SemanticIndex;

const STEP_STATE_PARAMETER_NAME: &str = "state";
pub(super) const CHECKED_TYPE_LABEL_PREFIX: &str = "__strata_checked_";
const CHECKED_PROCESS_REF_TYPE_LABEL_PREFIX: &str = "__strata_checked_process_ref_";

#[derive(Debug, Clone, Copy)]
struct ProcessRefBinding {
    id: CheckedProcessRefId,
    target: CheckedProcessId,
}

struct ModuleCheckContext<'a> {
    module: &'a Module,
    entry_process: CheckedProcessId,
    semantic_index: &'a SemanticIndex,
    message_cases: &'a MessageCaseTable,
}

struct CheckedTypeInterner<'a> {
    semantic_index: &'a SemanticIndex,
    entries: Vec<(TypeRef, CheckedTypeRef)>,
}

impl<'a> CheckedTypeInterner<'a> {
    fn new(semantic_index: &'a SemanticIndex) -> Self {
        Self {
            semantic_index,
            entries: Vec::new(),
        }
    }

    fn intern(&mut self, ty: &TypeRef) -> Result<CheckedTypeRef> {
        if let Some((_, checked)) = self
            .entries
            .iter()
            .find(|(existing, _)| self.semantic_index.same_type(existing, ty))
        {
            return Ok(checked.clone());
        }

        if self.entries.len() >= MAX_TYPE_COUNT {
            return Err(Error::new(format!(
                "checked type_count exceeds Mantle artifact limit of {MAX_TYPE_COUNT} types"
            )));
        }
        let id = CheckedTypeId::from_index(self.entries.len())?;
        let process_ref_target = self.semantic_index.process_ref_target_type(ty)?;
        let kind = process_ref_target.map_or(CheckedTypeKind::Value, |target| {
            CheckedTypeKind::ProcessRef { target }
        });
        let checked = CheckedTypeRef::new(id, checked_type_label(ty, process_ref_target)?, kind);
        self.entries.push((ty.clone(), checked.clone()));
        Ok(checked)
    }

    fn into_types(self) -> Vec<CheckedTypeRef> {
        self.entries
            .into_iter()
            .map(|(_, checked)| checked)
            .collect()
    }
}

fn checked_type_label(
    ty: &TypeRef,
    process_ref_target: Option<CheckedProcessId>,
) -> Result<String> {
    if let Some(target) = process_ref_target {
        return checked_process_ref_type_label(target);
    }
    match ty {
        TypeRef::Named(name) => Ok(name.to_string()),
        TypeRef::Applied { constructor, .. } => Ok(constructor.to_string()),
    }
}

fn checked_process_ref_type_label(target: CheckedProcessId) -> Result<String> {
    let label = format!("{CHECKED_PROCESS_REF_TYPE_LABEL_PREFIX}{}", target.as_u32());
    if label.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "checked process reference type label exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    Ok(label)
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
    body: StepBodySource<'a>,
    payload_param: Option<PatternPayloadParam>,
}

#[derive(Debug, Clone)]
enum StepBodySource<'a> {
    Block(&'a FunctionBlock),
    StateMatch(&'a super::ast::Match),
}

struct StepDiscoveryClause<'a> {
    pattern: StepPattern,
    body: &'a FunctionBlock,
    state_payload_bindings: Vec<StatePayloadDiscoveryBinding>,
}

#[derive(Debug, Clone)]
struct StepClause<'a> {
    step: &'a Function,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_binding: Option<StepPayloadBinding>,
    current_state: Option<CheckedStateId>,
    state_payload_binding: Option<StepStatePayloadBinding>,
    body: &'a FunctionBlock,
}

struct StepTransitionInput<'a> {
    current_state: Option<CheckedStateId>,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_binding: Option<&'a StepPayloadBinding>,
    state_payload_binding: Option<&'a StepStatePayloadBinding>,
    body: &'a FunctionBlock,
    declared_effects: &'a [Effect],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepPattern {
    Variant {
        message: CheckedMessageVariantId,
        binding: Option<PatternPayloadParam>,
    },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepDispatchForm {
    ParameterPattern(StepPattern),
    BodyMatch,
    StateMatch(StepPattern),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepDispatchStyle {
    ParameterPattern,
    BodyMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternPayloadParam {
    name: Identifier,
    ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatePayloadDiscoveryBinding {
    name: Identifier,
    ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveryValueBinding {
    name: Identifier,
    ty: TypeRef,
    label: String,
}

struct SendPayloadDiscoveryContext<'a, 'types, 'semantic> {
    sender_cases: &'a [DiscoveredMessageCase],
    concrete_state_payloads: &'a BTreeMap<String, BTreeSet<String>>,
    process_refs: &'a BTreeMap<Identifier, CheckedProcessId>,
    types: &'types mut CheckedTypeInterner<'semantic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepPayloadBinding {
    name: Identifier,
    ty: TypeRef,
    checked_ty: CheckedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepStatePayloadBinding {
    name: Identifier,
    ty: TypeRef,
    checked_ty: CheckedTypeRef,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypedMatchPattern {
    Variant {
        variant: usize,
        binding: Option<PatternPayloadParam>,
    },
    Wildcard,
}

struct TypedMatchArm<'a> {
    pattern: TypedMatchPattern,
    body: &'a FunctionBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternPayloadContext {
    StepPattern,
    SourceValue,
}

#[derive(Debug, Clone, Copy)]
enum PatternBindingContext<'a> {
    Step { process: &'a Process },
    Source { owner: &'a str },
}

struct PatternCheckContext<'a> {
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    enum_decl: &'a Enum,
    enum_type: &'a TypeRef,
    subject: &'a str,
    label: &'a str,
    payload_context: PatternPayloadContext,
    binding_context: PatternBindingContext<'a>,
}

#[derive(Debug, Clone, Copy)]
struct SourceFunctionScope<'a> {
    module: &'a Module,
    process_name: Option<&'a Identifier>,
    process_functions: &'a [Function],
    semantic_index: &'a SemanticIndex,
}

#[derive(Debug, Clone, Copy)]
struct SourceValueBinding<'a> {
    name: &'a Identifier,
    ty: &'a TypeRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFunctionParamKind {
    Binding,
    Pattern,
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
    let mut types = CheckedTypeInterner::new(&semantic_index);
    let entry_process = semantic_index
        .process_id_by_name("Main")
        .map_err(|_| Error::new("entry process Main is not declared"))?;
    validate_source_function_declarations(&module, &semantic_index)?;
    validate_process_declarations_before_message_cases(&module, &semantic_index)?;
    let message_cases =
        MessageCaseTable::build(&module, entry_process, &semantic_index, &mut types)?;
    let mut outputs = OutputPool::new();
    let check_context = ModuleCheckContext {
        module: &module,
        entry_process,
        semantic_index: &semantic_index,
        message_cases: &message_cases,
    };
    let mut checked_processes = Vec::with_capacity(module.processes.len());
    for (index, process) in module.processes.iter().enumerate() {
        let process_id = CheckedProcessId::from_index(index)?;
        checked_processes.push(check_process(
            &check_context,
            process,
            process_id,
            &mut types,
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
        types: types.into_types(),
        outputs: outputs.into_values(),
        processes: checked_processes,
    }))
}

fn validate_process_declarations_before_message_cases(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let mut validation_types = CheckedTypeInterner::new(semantic_index);
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
        let _ = StateSpace::new(module, semantic_index, process, &mut validation_types)?;
        let process_id = CheckedProcessId::from_index(process_index)?;
        for step in &process.steps {
            check_step_shape(module, process, process_id, semantic_index, step)?;
        }
    }
    Ok(())
}

fn check_process(
    context: &ModuleCheckContext<'_>,
    process: &Process,
    process_id: CheckedProcessId,
    types: &mut CheckedTypeInterner<'_>,
    outputs: &mut OutputPool,
) -> Result<CheckedProcess> {
    validate_count(
        &format!("process {} mailbox_bound", process.name),
        process.mailbox_bound,
        1,
        MAX_MAILBOX_BOUND,
    )?;

    let msg_enum = context
        .semantic_index
        .enum_decl(context.module, &process.msg_type)?;
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
        context.message_cases.cases_for(process_id)?.len(),
        1,
        MAX_MESSAGE_VARIANTS_PER_PROCESS,
    )?;

    let mut state_space = StateSpace::new(context.module, context.semantic_index, process, types)?;
    let init_state = check_init(
        context.module,
        context.semantic_index,
        process,
        &mut state_space,
        types,
    )?;
    let process_context = ProcessCheckContext {
        module: context.module,
        process,
        process_id,
        entry_process: context.entry_process,
        semantic_index: context.semantic_index,
        message_cases: context.message_cases,
    };
    let (process_refs, transitions) =
        check_step(&process_context, &mut state_space, outputs, types)?;
    let state_values = state_space.into_values()?;

    Ok(CheckedProcess::new(CheckedProcessParts {
        debug_name: process.name.clone(),
        state_type: types.intern(&process.state_type)?,
        state_values,
        message_type: types.intern(&process.msg_type)?,
        message_cases: context.message_cases.cases_for(process_id)?.to_vec(),
        process_refs,
        mailbox_bound: process.mailbox_bound,
        init_state,
        transitions,
    }))
}

fn check_typed_match_arms<'a>(
    context: &PatternCheckContext<'_>,
    arms: &'a [MatchArm],
) -> Result<Vec<TypedMatchArm<'a>>> {
    let mut explicit_arms = vec![false; context.enum_decl.variants.len()];
    let mut wildcard_seen = false;
    let mut checked_arms = Vec::with_capacity(arms.len());
    let label = context.label;

    for arm in arms {
        let pattern = check_typed_match_pattern(context, &arm.pattern)?;
        match pattern {
            TypedMatchPattern::Variant { variant, .. } => {
                if explicit_arms[variant] {
                    return Err(Error::new(format!(
                        "{} {label} declares duplicate pattern for variant {}",
                        context.subject, context.enum_decl.variants[variant].name,
                    )));
                }
                explicit_arms[variant] = true;
            }
            TypedMatchPattern::Wildcard => {
                if wildcard_seen {
                    return Err(Error::new(format!(
                        "{} {label} declares duplicate wildcard pattern",
                        context.subject
                    )));
                }
                wildcard_seen = true;
            }
        }
        checked_arms.push(TypedMatchArm {
            pattern,
            body: &arm.body,
        });
    }

    if wildcard_seen && explicit_arms.iter().all(|is_present| *is_present) {
        return Err(Error::new(format!(
            "{} {label} wildcard pattern is unreachable",
            context.subject
        )));
    }
    if !wildcard_seen {
        for (index, variant) in context.enum_decl.variants.iter().enumerate() {
            if !explicit_arms[index] {
                return Err(Error::new(format!(
                    "{} {label} must handle variant {}",
                    context.subject, variant.name,
                )));
            }
        }
    }

    Ok(checked_arms)
}

fn check_typed_match_pattern(
    context: &PatternCheckContext<'_>,
    pattern: &Pattern,
) -> Result<TypedMatchPattern> {
    match pattern {
        Pattern::Constructor { name, binding } => {
            let variant_index = context.semantic_index.enum_variant_index(
                context.module,
                context.enum_type,
                name,
            )?;
            let variant = &context.enum_decl.variants[variant_index];
            let binding = check_pattern_payload_binding(
                context.semantic_index,
                variant,
                binding.as_ref(),
                context.label,
                context.payload_context,
                context.binding_context,
            )?;
            Ok(TypedMatchPattern::Variant {
                variant: variant_index,
                binding,
            })
        }
        Pattern::Wildcard => Ok(TypedMatchPattern::Wildcard),
    }
}

fn check_pattern_payload_binding(
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    binding: Option<&Param>,
    context: &str,
    payload_context: PatternPayloadContext,
    binding_context: PatternBindingContext<'_>,
) -> Result<Option<PatternPayloadParam>> {
    match (&variant.payload_type, binding) {
        (None, None) => Ok(None),
        (None, Some(_)) => {
            let noun = match payload_context {
                PatternPayloadContext::StepPattern => "message",
                PatternPayloadContext::SourceValue => "pattern",
            };
            let subject = pattern_binding_subject(binding_context);
            Err(Error::new(format!(
                "{subject} {context} {noun} {} does not carry a payload",
                variant.name
            )))
        }
        (Some(_), None) => Ok(None),
        (Some(payload_type), Some(binding)) => {
            validate_pattern_binding_name(binding_context, semantic_index, &binding.name)?;
            if !semantic_index.same_type(&binding.ty, payload_type) {
                let subject = pattern_binding_subject(binding_context);
                return Err(Error::new(format!(
                    "{subject} {context} payload {} has type {}, expected {}",
                    binding.name, binding.ty, payload_type
                )));
            }
            Ok(Some(PatternPayloadParam {
                name: binding.name.clone(),
                ty: binding.ty.clone(),
            }))
        }
    }
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
