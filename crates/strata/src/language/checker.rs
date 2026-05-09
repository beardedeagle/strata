mod outputs;
mod state_space;
mod static_validation;
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
use outputs::OutputPool;
use state_space::{
    StateSpace, ValueBinding, ValueTemplateBinding, ValueTemplateSource,
    canonical_source_value_with_bindings, checked_value_template_with_binding,
    source_value_uses_binding,
};
use static_validation::validate_action_references;
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
        types: &mut CheckedTypeInterner<'_>,
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
        let concrete_state_payload_domains = module
            .processes
            .iter()
            .enumerate()
            .map(|(process_index, process)| {
                let process_id = CheckedProcessId::from_index(process_index)?;
                collect_concrete_state_payload_domains(module, process, process_id, semantic_index)
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
                    for clause in
                        step_discovery_clauses(module, process, process_id, semantic_index, step)?
                    {
                        for sender_case in matching_message_cases(
                            sender_cases,
                            &clause.pattern,
                            &explicit_step_variants[process_index],
                        ) {
                            let bindings = payload_value_bindings(&clause.pattern, sender_case);
                            for statement in &clause.body.statements {
                                let Statement::Send {
                                    target,
                                    message,
                                    payload,
                                } = statement
                                else {
                                    continue;
                                };
                                let target_process_id = resolve_send_target_process_for_discovery(
                                    process,
                                    semantic_index,
                                    &process_ref_targets[process_index],
                                    &clause.pattern,
                                    target,
                                )?;
                                let target_variant = semantic_index.message_id_for_process(
                                    module,
                                    process.name.as_str(),
                                    target_process_id,
                                    message,
                                )?;
                                let builder = builders
                                    .get_mut(target_process_id.index())
                                    .ok_or_else(|| {
                                        Error::new(format!(
                                            "process id {} is not declared",
                                            target_process_id.as_u32()
                                        ))
                                    })?;
                                let mut discovery_context = SendPayloadDiscoveryContext {
                                    sender_cases,
                                    concrete_state_payloads: &concrete_state_payload_domains
                                        [process_index],
                                    process_refs: &process_ref_targets[process_index],
                                    types,
                                };
                                changed |= add_discovered_send_payload_cases(
                                    builder,
                                    target_variant,
                                    payload.as_ref(),
                                    &bindings,
                                    &clause.state_payload_bindings,
                                    &mut discovery_context,
                                )?;
                            }
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
            let cases = builder.logical_cases(types)?;
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
            payloads_by_process.push(builder.payload_domains(types)?);
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
        variant_id: CheckedMessageVariantId,
        payload: Option<&ValueExpr>,
        bindings: &[ValueBinding<'_>],
        process_refs: &BTreeMap<Identifier, CheckedProcessId>,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<bool> {
        let variant =
            self.semantic_index
                .message_variant(self.module, self.process_id, variant_id)?;
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
                let function_scope = SourceFunctionScope {
                    module: self.module,
                    process_name: Some(&self.process.name),
                    process_functions: &self.process.functions,
                    semantic_index: self.semantic_index,
                };
                let source_bindings = bindings
                    .iter()
                    .map(|binding| SourceValueBinding {
                        name: binding.name,
                        ty: binding.ty,
                    })
                    .collect::<Vec<_>>();
                let payload = resolve_source_value_expr(
                    &function_scope,
                    payload_type,
                    payload,
                    &source_bindings,
                    0,
                )?;
                let label = if let Some(target) =
                    self.semantic_index.process_ref_target_type(payload_type)?
                {
                    canonical_process_ref_payload_label(
                        payload_type,
                        target,
                        &payload,
                        bindings,
                        process_refs,
                    )?
                } else {
                    canonical_source_value_with_bindings(
                        self.module,
                        self.semantic_index,
                        payload_type,
                        &payload,
                        bindings,
                    )?
                };
                let checked_type = types.intern(payload_type)?;
                Ok(self.insert_payload_case(variant_id, checked_type, label))
            }
        }
    }

    fn insert_payload_case(
        &mut self,
        variant_id: CheckedMessageVariantId,
        payload_type: CheckedTypeRef,
        label: String,
    ) -> bool {
        let payloads = self.payload_cases.entry(variant_id).or_default();
        if payloads.contains_key(&label) {
            return false;
        }
        payloads.insert(label.clone(), CheckedPayloadValue::new(payload_type, label));
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

    fn logical_cases(
        &self,
        types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Vec<CheckedMessageCase>> {
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
                    variant
                        .payload_type
                        .as_ref()
                        .map(|payload_type| types.intern(payload_type))
                        .transpose()?,
                )
            })
            .collect()
    }

    fn payload_domains(
        &self,
        _types: &mut CheckedTypeInterner<'_>,
    ) -> Result<Vec<Vec<CheckedPayloadValue>>> {
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

fn add_discovered_send_payload_cases(
    builder: &mut MessageCaseBuilder<'_>,
    variant_id: CheckedMessageVariantId,
    payload: Option<&ValueExpr>,
    message_bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    context: &mut SendPayloadDiscoveryContext<'_, '_, '_>,
) -> Result<bool> {
    let mut changed = false;
    for bindings in discovery_value_binding_sets(
        payload,
        message_bindings,
        state_payload_bindings,
        context.sender_cases,
        context.concrete_state_payloads,
        context.types,
    )? {
        let value_bindings = value_bindings_from_discovery(&bindings);
        changed |= builder.add_payload_case(
            variant_id,
            payload,
            &value_bindings,
            context.process_refs,
            context.types,
        )?;
    }
    Ok(changed)
}

fn discovery_value_binding_sets(
    payload: Option<&ValueExpr>,
    message_bindings: &[DiscoveryValueBinding],
    state_payload_bindings: &[StatePayloadDiscoveryBinding],
    sender_cases: &[DiscoveredMessageCase],
    concrete_state_payloads: &BTreeMap<String, BTreeSet<String>>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<Vec<Vec<DiscoveryValueBinding>>> {
    let Some(payload) = payload else {
        return Ok(vec![message_bindings.to_vec()]);
    };
    let mut binding_sets = vec![message_bindings.to_vec()];
    for binding in state_payload_bindings {
        if !source_value_uses_binding(payload, &binding.name) {
            continue;
        }
        let payloads =
            state_payload_discovery_values(binding, sender_cases, concrete_state_payloads, types)?;
        if payloads.is_empty() {
            return Ok(Vec::new());
        }
        let mut expanded = Vec::with_capacity(binding_sets.len().saturating_mul(payloads.len()));
        for base in &binding_sets {
            for payload in &payloads {
                let mut next = base.clone();
                next.push(DiscoveryValueBinding {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    label: payload.label().to_string(),
                });
                expanded.push(next);
            }
        }
        binding_sets = expanded;
    }
    Ok(binding_sets)
}

fn state_payload_discovery_values(
    binding: &StatePayloadDiscoveryBinding,
    sender_cases: &[DiscoveredMessageCase],
    concrete_state_payloads: &BTreeMap<String, BTreeSet<String>>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<Vec<CheckedPayloadValue>> {
    let checked_ty = types.intern(&binding.ty)?;
    let mut payloads = BTreeMap::new();
    if let Some(concrete_payloads) = concrete_state_payloads.get(checked_ty.label()) {
        for label in concrete_payloads {
            payloads.insert(
                label.clone(),
                CheckedPayloadValue::new(checked_ty.clone(), label.clone()),
            );
        }
    }
    for case in sender_cases {
        let Some(payload) = case.payload() else {
            continue;
        };
        if payload.ty().label() == checked_ty.label() && payload.process_ref_payload().is_none() {
            payloads.insert(payload.label().to_string(), payload.clone());
        }
    }
    Ok(payloads.into_values().collect())
}

fn value_bindings_from_discovery(bindings: &[DiscoveryValueBinding]) -> Vec<ValueBinding<'_>> {
    bindings
        .iter()
        .map(|binding| ValueBinding {
            name: &binding.name,
            ty: &binding.ty,
            label: &binding.label,
        })
        .collect()
}

fn canonical_process_ref_payload_label(
    expected_type: &TypeRef,
    expected_target: CheckedProcessId,
    payload: &ValueExpr,
    bindings: &[ValueBinding<'_>],
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
) -> Result<String> {
    let ValueExpr::Identifier(name) = payload else {
        return Err(Error::new(format!(
            "process reference payload type {expected_type} must be passed as an immutable process reference value"
        )));
    };
    if let Some(binding) = bindings.iter().find(|binding| binding.name == name) {
        if binding.ty == expected_type {
            return Ok(binding.label.to_string());
        }
        return Err(Error::new(format!(
            "value binding {} has type {}, expected {}",
            binding.name, binding.ty, expected_type
        )));
    }
    let Some(actual_target) = process_refs.get(name) else {
        return Err(Error::new(format!(
            "process reference payload {name} is not a bound process reference"
        )));
    };
    if *actual_target != expected_target {
        return Err(Error::new(format!(
            "process reference payload {name} targets process id {}, expected {}",
            actual_target.as_u32(),
            expected_target.as_u32()
        )));
    }
    Ok(expected_type.to_string())
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

fn check_init(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
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

    validate_effects("init", &init.effects, BTreeSet::new())?;

    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        semantic_index,
    };
    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    match body {
        FunctionBody::Block(body) => check_init_return_block(
            process,
            &function_scope,
            state_space,
            types,
            body,
            None,
            "init body",
        ),
        FunctionBody::Match(match_body) => {
            check_init_match(process, &function_scope, state_space, types, match_body)
        }
    }
}

fn check_init_match(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    match_body: &super::ast::Match,
) -> Result<CheckedStateId> {
    let scrutinee_type = scope
        .semantic_index
        .fieldless_enum_variant_type(scope.module, &match_body.scrutinee)?;
    let enum_decl = scope
        .semantic_index
        .enum_decl(scope.module, &scrutinee_type)?;
    let selected_variant = scope.semantic_index.enum_variant_index(
        scope.module,
        &scrutinee_type,
        &match_body.scrutinee,
    )?;
    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module: scope.module,
        semantic_index: scope.semantic_index,
        enum_decl,
        enum_type: &scrutinee_type,
        subject: &subject,
        label: "init match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;

    let mut selected_state = None;
    let mut wildcard_state = None;
    for arm in arms {
        let payload_binding = match &arm.pattern {
            TypedMatchPattern::Variant { binding, .. } => binding.as_ref(),
            TypedMatchPattern::Wildcard => None,
        };
        let state = resolve_init_return_block_value(
            process,
            scope,
            arm.body,
            payload_binding,
            "init match arm",
        )?;
        match arm.pattern {
            TypedMatchPattern::Variant { variant, .. } if variant == selected_variant => {
                selected_state = Some(state);
            }
            TypedMatchPattern::Wildcard => {
                wildcard_state = Some(state);
            }
            _ => {}
        }
    }

    let state = selected_state.or(wildcard_state).ok_or_else(|| {
        Error::new(format!(
            "process {} init match has no arm for scrutinee {}",
            process.name, match_body.scrutinee
        ))
    })?;
    state_space.resolve_state_value(scope.semantic_index, types, &state)
}

fn check_init_return_block(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    body: &FunctionBlock,
    payload_binding: Option<&PatternPayloadParam>,
    context: &str,
) -> Result<CheckedStateId> {
    let value = resolve_init_return_block_value(process, scope, body, payload_binding, context)?;
    state_space.resolve_state_value(scope.semantic_index, types, &value)
}

fn resolve_init_return_block_value(
    process: &Process,
    scope: &SourceFunctionScope<'_>,
    body: &FunctionBlock,
    payload_binding: Option<&PatternPayloadParam>,
    context: &str,
) -> Result<ValueExpr> {
    if !body.statements.is_empty() {
        return Err(Error::new(format!(
            "{context} must not perform statements in this slice"
        )));
    }
    let value = match &body.returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
    };
    let bindings = payload_binding
        .map(|binding| {
            vec![SourceValueBinding {
                name: &binding.name,
                ty: &binding.ty,
            }]
        })
        .unwrap_or_default();
    let value = resolve_source_value_expr(scope, &process.state_type, &value, &bindings, 0)?;
    if let Some(binding) = payload_binding {
        if source_value_uses_binding(&value, &binding.name) {
            return Err(Error::new(format!(
                "process {} {context} cannot use payload binding {} in returned state",
                process.name, binding.name
            )));
        }
    }
    check_source_value_type(scope, &process.state_type, &value, &bindings)?;
    Ok(value)
}

fn validate_source_function_declarations(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let mut module_function_names = BTreeSet::new();
    validate_source_function_groups(module, semantic_index, "module", None, &module.functions)?;
    for function in &module.functions {
        module_function_names.insert(function.name.as_str());
    }

    for process in &module.processes {
        for function in &process.functions {
            if module_function_names.contains(function.name.as_str()) {
                return Err(Error::new(format!(
                    "process {} function {} conflicts with module function {}",
                    process.name, function.name, function.name
                )));
            }
        }
        let owner = format!("process {}", process.name);
        validate_source_function_groups(
            module,
            semantic_index,
            &owner,
            Some(process),
            &process.functions,
        )?;
    }

    Ok(())
}

fn validate_source_function_groups(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[Function],
) -> Result<()> {
    let mut groups: BTreeMap<&str, Vec<&Function>> = BTreeMap::new();
    for function in functions {
        validate_source_function_name(semantic_index, owner, &function.name)?;
        groups
            .entry(function.name.as_str())
            .or_default()
            .push(function);
    }

    for group in groups.values() {
        validate_source_function_group(module, semantic_index, owner, process, group)?;
    }

    validate_source_function_call_cycles(owner, functions)?;

    Ok(())
}

fn validate_source_function_name(
    semantic_index: &SemanticIndex,
    owner: &str,
    name: &Identifier,
) -> Result<()> {
    if matches!(
        name.as_str(),
        "init" | "step" | "Stop" | "Continue" | "Panic"
    ) {
        return Err(Error::new(format!(
            "{owner} function {name} uses a reserved function name"
        )));
    }
    if semantic_index.process_id(name).is_ok() {
        return Err(Error::new(format!(
            "{owner} function {name} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(name) {
        return Err(Error::new(format!(
            "{owner} function {name} conflicts with a declared type or value constructor"
        )));
    }
    Ok(())
}

fn validate_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let first_kind = source_function_param_kind(first)?;

    for function in functions {
        validate_source_function_contract(semantic_index, owner, function)?;
        if !semantic_index.same_type(&function.return_type, &first.return_type) {
            return Err(Error::new(format!(
                "{owner} function {} clauses must return {}, found {}",
                first.name, first.return_type, function.return_type
            )));
        }
        let kind = source_function_param_kind(function)?;
        if kind != first_kind {
            return Err(Error::new(format!(
                "{owner} function {} cannot mix binding parameters with pattern parameters",
                first.name
            )));
        }
    }

    match first_kind {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "{owner} function {} declares duplicate binding clauses",
                    first.name
                )));
            }
            validate_binding_source_function_body(module, semantic_index, owner, process, first)
        }
        SourceFunctionParamKind::Pattern => validate_pattern_source_function_group(
            module,
            semantic_index,
            owner,
            process,
            functions,
        ),
    }
}

fn validate_source_function_contract(
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
) -> Result<()> {
    if function.params.len() != 1 {
        return Err(Error::new(format!(
            "{owner} function {} must declare exactly one parameter in this source slice",
            function.name
        )));
    }
    if !function.effects.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} must not declare effects",
            function.name
        )));
    }
    if !function.may.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} may-behaviors must be empty",
            function.name
        )));
    }
    if function.determinism != Determinism::Det {
        return Err(Error::new(format!(
            "{owner} function {} must be deterministic",
            function.name
        )));
    }
    if function.body.is_none() {
        return Err(Error::new(format!(
            "{owner} function {} must have a body for buildable source",
            function.name
        )));
    }
    validate_source_function_declared_value_type(
        semantic_index,
        owner,
        function,
        "return type",
        &function.return_type,
    )?;
    if let [FunctionParam::Binding(param)] = function.params.as_slice() {
        validate_source_function_declared_value_type(
            semantic_index,
            owner,
            function,
            &format!("parameter {}", param.name),
            &param.ty,
        )?;
    }
    Ok(())
}

fn validate_source_function_declared_value_type(
    semantic_index: &SemanticIndex,
    owner: &str,
    function: &Function,
    position: &str,
    ty: &TypeRef,
) -> Result<()> {
    if semantic_index.is_source_value_type(ty) {
        return Ok(());
    }
    Err(Error::new(format!(
        "{owner} function {} {position} must use a declared record or enum type, found {ty}",
        function.name
    )))
}

fn validate_source_function_call_cycles(owner: &str, functions: &[Function]) -> Result<()> {
    let function_names = functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut graph = function_names
        .iter()
        .map(|name| (*name, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();

    for function in functions {
        let mut calls = BTreeSet::new();
        collect_source_function_calls(function, &mut calls);
        let caller = function.name.as_str();
        let Some(callees) = graph.get_mut(caller) else {
            return Err(Error::new(format!(
                "{owner} function {} is not registered for cycle validation",
                function.name
            )));
        };
        for call in calls {
            if function_names.contains(call) {
                callees.insert(call);
            }
        }
    }

    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    for name in function_names {
        validate_source_function_call_cycle_from(owner, name, &graph, &mut visited, &mut stack)?;
    }
    Ok(())
}

fn validate_source_function_call_cycle_from<'a>(
    owner: &str,
    name: &'a str,
    graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    visited: &mut BTreeSet<&'a str>,
    stack: &mut Vec<&'a str>,
) -> Result<()> {
    if visited.contains(name) {
        return Ok(());
    }

    stack.push(name);
    if let Some(callees) = graph.get(name) {
        for &callee in callees {
            if let Some(position) = stack.iter().position(|candidate| *candidate == callee) {
                let mut cycle = stack[position..].to_vec();
                cycle.push(callee);
                return Err(Error::new(format!(
                    "{owner} source function call cycle {} is not supported in this source slice",
                    cycle.join(" -> ")
                )));
            }
            validate_source_function_call_cycle_from(owner, callee, graph, visited, stack)?;
        }
    }
    stack.pop();
    visited.insert(name);
    Ok(())
}

fn collect_source_function_calls<'a>(function: &'a Function, calls: &mut BTreeSet<&'a str>) {
    let Some(body) = &function.body else {
        return;
    };
    match body {
        FunctionBody::Block(body) => collect_source_return_expr_calls(&body.returns, calls),
        FunctionBody::Match(match_body) => {
            for arm in &match_body.arms {
                collect_source_return_expr_calls(&arm.body.returns, calls);
            }
        }
    }
}

fn collect_source_return_expr_calls<'a>(returns: &'a ReturnExpr, calls: &mut BTreeSet<&'a str>) {
    match returns {
        ReturnExpr::Value(value) => collect_source_value_expr_calls(value, calls),
        ReturnExpr::Call { name, arg } => {
            calls.insert(name.as_str());
            collect_source_value_expr_calls(arg, calls);
        }
    }
}

fn collect_source_value_expr_calls<'a>(value: &'a ValueExpr, calls: &mut BTreeSet<&'a str>) {
    match value {
        ValueExpr::Identifier(_) => {}
        ValueExpr::Call { name, arg } => {
            calls.insert(name.as_str());
            collect_source_value_expr_calls(arg, calls);
        }
        ValueExpr::EnumVariant { payload, .. } => {
            collect_source_value_expr_calls(payload, calls);
        }
        ValueExpr::Record(record) => {
            for field in &record.fields {
                collect_source_value_expr_calls(&field.value, calls);
            }
        }
    }
}

fn source_function_param_kind(function: &Function) -> Result<SourceFunctionParamKind> {
    match function.params.as_slice() {
        [FunctionParam::Binding(_)] => Ok(SourceFunctionParamKind::Binding),
        [FunctionParam::Pattern(_)] => Ok(SourceFunctionParamKind::Pattern),
        _ => Err(Error::new(format!(
            "function {} must declare exactly one parameter in this source slice",
            function.name
        ))),
    }
}

fn validate_binding_source_function_body(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    function: &Function,
) -> Result<()> {
    let FunctionParam::Binding(param) = &function.params[0] else {
        return Err(Error::new(format!(
            "{owner} function {} must declare a binding parameter",
            function.name
        )));
    };

    match function.body.as_ref().expect("validated function body") {
        FunctionBody::Block(body) => validate_pure_source_function_block(owner, function, body),
        FunctionBody::Match(match_body) => {
            if match_body.scrutinee != param.name {
                return Err(Error::new(format!(
                    "{owner} function {} match scrutinee {} must be parameter {}",
                    function.name, match_body.scrutinee, param.name
                )));
            }
            let enum_decl = semantic_index.enum_decl(module, &param.ty)?;
            let subject = format!("{owner} function {}", function.name);
            let pattern_context = PatternCheckContext {
                module,
                semantic_index,
                enum_decl,
                enum_type: &param.ty,
                subject: &subject,
                label: "match",
                payload_context: PatternPayloadContext::SourceValue,
                binding_context: PatternBindingContext::Source { owner: &subject },
            };
            for arm in check_typed_match_arms(&pattern_context, &match_body.arms)? {
                validate_pure_source_function_block(owner, function, arm.body)?;
            }
            Ok(())
        }
    }?;

    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        semantic_index,
    };
    validate_source_function_body_values(
        &scope,
        function,
        &[SourceValueBinding {
            name: &param.name,
            ty: &param.ty,
        }],
    )
}

fn validate_pattern_source_function_group(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    process: Option<&Process>,
    functions: &[&Function],
) -> Result<()> {
    let Some(first) = functions.first() else {
        return Ok(());
    };
    let enum_type = infer_pattern_function_enum_type(module, semantic_index, owner, functions)?;
    let enum_decl = semantic_index.enum_decl(module, &enum_type)?;
    let process_functions = process
        .map(|process| process.functions.as_slice())
        .unwrap_or(&[]);
    let scope = SourceFunctionScope {
        module,
        process_name: process.map(|process| &process.name),
        process_functions,
        semantic_index,
    };
    let mut explicit_arms = vec![false; enum_decl.variants.len()];
    let mut wildcard_seen = false;

    for function in functions {
        validate_pure_source_function_block(owner, function, source_function_block(function)?)?;
        match &function.params[0] {
            FunctionParam::Pattern(pattern) => {
                let subject = format!("{owner} function {}", function.name);
                let pattern_context = PatternCheckContext {
                    module,
                    semantic_index,
                    enum_decl,
                    enum_type: &enum_type,
                    subject: &subject,
                    label: "signature",
                    payload_context: PatternPayloadContext::SourceValue,
                    binding_context: PatternBindingContext::Source { owner: &subject },
                };
                let checked_pattern = check_typed_match_pattern(&pattern_context, pattern)?;
                match checked_pattern {
                    TypedMatchPattern::Variant { variant, .. } => {
                        if explicit_arms[variant] {
                            return Err(Error::new(format!(
                                "{owner} function {} declares duplicate pattern for variant {}",
                                function.name, enum_decl.variants[variant].name
                            )));
                        }
                        explicit_arms[variant] = true;
                    }
                    TypedMatchPattern::Wildcard => {
                        if wildcard_seen {
                            return Err(Error::new(format!(
                                "{owner} function {} declares duplicate wildcard pattern",
                                function.name
                            )));
                        }
                        wildcard_seen = true;
                    }
                }
            }
            FunctionParam::Binding(_) => {
                return Err(Error::new(format!(
                    "{owner} function {} cannot mix binding and pattern clauses",
                    function.name
                )));
            }
        }
        let body_bindings = match &function.params[0] {
            FunctionParam::Pattern(Pattern::Constructor {
                binding: Some(payload),
                ..
            }) => vec![SourceValueBinding {
                name: &payload.name,
                ty: &payload.ty,
            }],
            _ => Vec::new(),
        };
        validate_source_function_body_values(&scope, function, &body_bindings)?;
    }

    if wildcard_seen && explicit_arms.iter().all(|is_present| *is_present) {
        return Err(Error::new(format!(
            "{owner} function {} wildcard pattern is unreachable",
            first.name
        )));
    }
    if !wildcard_seen {
        for (index, variant) in enum_decl.variants.iter().enumerate() {
            if !explicit_arms[index] {
                return Err(Error::new(format!(
                    "{owner} function {} must handle variant {}",
                    first.name, variant.name
                )));
            }
        }
    }
    Ok(())
}

fn infer_pattern_function_enum_type(
    module: &Module,
    semantic_index: &SemanticIndex,
    owner: &str,
    functions: &[&Function],
) -> Result<TypeRef> {
    let mut inferred = None;
    for function in functions {
        let FunctionParam::Pattern(Pattern::Constructor { name, .. }) = &function.params[0] else {
            continue;
        };
        let next = semantic_index.enum_variant_type(module, name)?;
        if let Some(existing) = &inferred {
            if !semantic_index.same_type(existing, &next) {
                return Err(Error::new(format!(
                    "{owner} function {} pattern {} belongs to {}, expected {}",
                    function.name, name, next, existing
                )));
            }
        } else {
            inferred = Some(next);
        }
    }
    inferred.ok_or_else(|| {
        Error::new(format!(
            "{owner} function {} wildcard pattern cannot infer a matched enum type",
            functions
                .first()
                .map(|function| function.name.to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ))
    })
}

fn validate_pure_source_function_block(
    owner: &str,
    function: &Function,
    body: &FunctionBlock,
) -> Result<()> {
    if !body.statements.is_empty() {
        return Err(Error::new(format!(
            "{owner} function {} must not perform statements",
            function.name
        )));
    }
    Ok(())
}

fn source_function_block(function: &Function) -> Result<&FunctionBlock> {
    match function.body.as_ref().expect("validated function body") {
        FunctionBody::Block(body) => Ok(body),
        FunctionBody::Match(_) => Err(Error::new(format!(
            "function {} pattern signature clauses must use block bodies",
            function.name
        ))),
    }
}

fn source_function_body_scope<'a>(
    scope: &SourceFunctionScope<'a>,
    function: &Function,
) -> SourceFunctionScope<'a> {
    if scope
        .module
        .functions
        .iter()
        .any(|candidate| std::ptr::eq(candidate, function))
    {
        SourceFunctionScope {
            module: scope.module,
            process_name: None,
            process_functions: &[],
            semantic_index: scope.semantic_index,
        }
    } else {
        *scope
    }
}

fn validate_source_function_body_values(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match function.body.as_ref().expect("validated function body") {
        FunctionBody::Block(body) => validate_source_function_return_expr(
            scope,
            &function.return_type,
            &body.returns,
            bindings,
        ),
        FunctionBody::Match(match_body) => {
            let FunctionParam::Binding(param) = &function.params[0] else {
                return Err(Error::new(format!(
                    "function {} match body requires a binding parameter",
                    function.name
                )));
            };
            if match_body.scrutinee != param.name {
                return Err(Error::new(format!(
                    "function {} match scrutinee {} must be parameter {}",
                    function.name, match_body.scrutinee, param.name
                )));
            }
            for arm in &match_body.arms {
                let mut arm_bindings = bindings.to_vec();
                if let Pattern::Constructor {
                    binding: Some(payload),
                    ..
                } = &arm.pattern
                {
                    if bindings.iter().any(|binding| binding.name == &payload.name) {
                        return Err(Error::new(format!(
                            "function {} match payload binding {} conflicts with an existing source value binding",
                            function.name, payload.name
                        )));
                    }
                    arm_bindings.push(SourceValueBinding {
                        name: &payload.name,
                        ty: &payload.ty,
                    });
                }
                validate_source_function_return_expr(
                    scope,
                    &function.return_type,
                    &arm.body.returns,
                    &arm_bindings,
                )?;
            }
            Ok(())
        }
    }
}

fn validate_source_function_return_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    returns: &ReturnExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let value = match returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
    };
    validate_source_function_value_expr(scope, expected_type, &value, bindings)
}

fn validate_source_function_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    match value {
        ValueExpr::Identifier(_) | ValueExpr::EnumVariant { .. } => {
            check_source_value_type(scope, expected_type, value, bindings)
        }
        ValueExpr::Call { name, arg } => {
            validate_source_function_call_or_constructor(scope, expected_type, name, arg, bindings)
        }
        ValueExpr::Record(record) => {
            let record_decl = scope
                .semantic_index
                .record_decl(scope.module, expected_type)?;
            if record.name != record_decl.name {
                return Err(Error::new(format!(
                    "expected record value {}, found {}",
                    record_decl.name, record.name
                )));
            }
            let mut seen = BTreeSet::new();
            for field in &record.fields {
                let Some(field_decl) = record_decl
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field.name)
                else {
                    return Err(Error::new(format!(
                        "record {} has no field {}",
                        record.name, field.name
                    )));
                };
                if !seen.insert(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} field {} is assigned more than once",
                        record.name, field.name
                    )));
                }
                validate_source_function_value_expr(scope, &field_decl.ty, &field.value, bindings)?;
            }
            for field in &record_decl.fields {
                if !seen.contains(field.name.as_str()) {
                    return Err(Error::new(format!(
                        "record {} value is missing field {}",
                        record_decl.name, field.name
                    )));
                }
            }
            Ok(())
        }
    }
}

fn validate_source_function_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return validate_source_enum_payload_value(scope, expected_type, name, arg, bindings);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    validate_source_function_call(scope, expected_type, name, arg, bindings, &functions)
}

fn validate_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    functions: &[&Function],
) -> Result<()> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }
    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            let FunctionParam::Binding(param) = &first.params[0] else {
                return Err(Error::new(format!(
                    "function {name} must declare a binding parameter"
                )));
            };
            validate_source_function_value_expr(scope, &param.ty, arg, bindings)
        }
        SourceFunctionParamKind::Pattern => {
            let enum_type = infer_pattern_function_enum_type(
                scope.module,
                scope.semantic_index,
                "source",
                functions,
            )?;
            validate_source_function_value_expr(scope, &enum_type, arg, bindings)
        }
    }
}

fn validate_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    validate_source_function_value_expr(scope, payload_type, payload, bindings)
}

fn resolve_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    if depth > MAX_VALUE_NESTING {
        return Err(Error::new(format!(
            "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
        )));
    }

    match value {
        ValueExpr::Identifier(_) => Ok(value.clone()),
        ValueExpr::Call { name, arg } => {
            resolve_source_call_or_constructor(scope, expected_type, name, arg, bindings, depth + 1)
        }
        ValueExpr::EnumVariant { name, payload } => resolve_source_enum_payload_value(
            scope,
            expected_type,
            name,
            payload,
            bindings,
            depth + 1,
        ),
        ValueExpr::Record(record) => {
            resolve_record_source_value_expr(scope, expected_type, record, bindings, depth + 1)
        }
    }
}

fn resolve_record_source_value_expr(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    record: &RecordValue,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let Ok(record_decl) = scope
        .semantic_index
        .record_decl(scope.module, expected_type)
    else {
        return Ok(ValueExpr::Record(record.clone()));
    };
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in &record.fields {
        let Some(field_decl) = record_decl
            .fields
            .iter()
            .find(|candidate| candidate.name == field.name)
        else {
            fields.push(field.clone());
            continue;
        };
        fields.push(RecordValueField {
            name: field.name.clone(),
            value: resolve_source_value_expr(
                scope,
                &field_decl.ty,
                &field.value,
                bindings,
                depth + 1,
            )?,
        });
    }
    Ok(ValueExpr::Record(RecordValue {
        name: record.name.clone(),
        fields,
    }))
}

fn enum_variant_for_expected_type<'module>(
    scope: &SourceFunctionScope<'module>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Result<Option<&'module EnumVariant>> {
    let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type) else {
        return Ok(None);
    };
    Ok(enum_decl
        .variants
        .iter()
        .find(|variant| variant.name == *name))
}

fn enum_value_error(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
) -> Error {
    match scope.semantic_index.enum_decl(scope.module, expected_type) {
        Ok(enum_decl) => Error::new(format!(
            "value {name} is not a variant of enum {}",
            enum_decl.name
        )),
        Err(_) => Error::new(format!(
            "value {name} cannot construct non-enum value of type {expected_type}"
        )),
    }
}

fn identifier_starts_uppercase(name: &Identifier) -> bool {
    name.as_str()
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn resolve_source_call_or_constructor(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let constructor = enum_variant_for_expected_type(scope, expected_type, name)?;
    let functions = source_function_group_option(scope, name)?;
    if constructor.is_some() && functions.is_some() {
        return Err(Error::new(format!(
            "value expression {name}(...) is ambiguous between an enum constructor and source function"
        )));
    }
    if constructor.is_some() {
        return resolve_source_enum_payload_value(scope, expected_type, name, arg, bindings, depth);
    }
    let Some(functions) = functions else {
        if identifier_starts_uppercase(name)
            && let Ok(enum_decl) = scope.semantic_index.enum_decl(scope.module, expected_type)
        {
            return Err(Error::new(format!(
                "value {name} is not a variant of enum {}",
                enum_decl.name
            )));
        }
        return Err(Error::new(format!("function {name} is not declared")));
    };
    resolve_source_function_call(scope, expected_type, name, arg, bindings, depth, &functions)
}

fn resolve_source_enum_payload_value(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    payload: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let variant = enum_variant_for_expected_type(scope, expected_type, name)?
        .ok_or_else(|| enum_value_error(scope, expected_type, name))?;
    let Some(payload_type) = &variant.payload_type else {
        return Err(Error::new(format!(
            "enum variant {name} does not accept a payload"
        )));
    };
    let payload = resolve_source_value_expr(scope, payload_type, payload, bindings, depth + 1)?;
    if scope
        .semantic_index
        .process_ref_target_type(payload_type)?
        .is_none()
    {
        check_source_value_type(scope, payload_type, &payload, bindings)?;
    }
    Ok(ValueExpr::EnumVariant {
        name: name.clone(),
        payload: Box::new(payload),
    })
}

fn resolve_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    name: &Identifier,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
    functions: &[&Function],
) -> Result<ValueExpr> {
    let first = functions
        .first()
        .ok_or_else(|| Error::new(format!("function {name} is not declared")))?;
    if !scope
        .semantic_index
        .same_type(&first.return_type, expected_type)
    {
        return Err(Error::new(format!(
            "function {name} returns {}, expected {}",
            first.return_type, expected_type
        )));
    }

    match source_function_param_kind(first)? {
        SourceFunctionParamKind::Binding => {
            if functions.len() != 1 {
                return Err(Error::new(format!(
                    "function {name} declares duplicate binding clauses"
                )));
            }
            resolve_binding_source_function_call(
                scope,
                expected_type,
                first,
                arg,
                bindings,
                depth + 1,
            )
        }
        SourceFunctionParamKind::Pattern => resolve_pattern_source_function_call(
            scope,
            expected_type,
            functions,
            arg,
            bindings,
            depth + 1,
        ),
    }
}

fn source_function_group_option<'a>(
    scope: &SourceFunctionScope<'a>,
    name: &Identifier,
) -> Result<Option<Vec<&'a Function>>> {
    let local: Vec<_> = scope
        .process_functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();
    let module: Vec<_> = scope
        .module
        .functions
        .iter()
        .filter(|function| function.name == *name)
        .collect();

    match (local.is_empty(), module.is_empty()) {
        (false, false) => Err(Error::new(format!(
            "{} function {name} conflicts with module function {name}",
            source_function_scope_label(scope)
        ))),
        (false, true) => Ok(Some(local)),
        (true, false) => Ok(Some(module)),
        (true, true) => Ok(None),
    }
}

fn source_function_scope_label(scope: &SourceFunctionScope<'_>) -> String {
    scope
        .process_name
        .map(|name| format!("process {name}"))
        .unwrap_or_else(|| "module".to_string())
}

fn resolve_binding_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    function: &Function,
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let FunctionParam::Binding(param) = &function.params[0] else {
        return Err(Error::new(format!(
            "function {} must declare a binding parameter",
            function.name
        )));
    };
    let resolved_arg = resolve_source_value_expr(scope, &param.ty, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &param.ty, &resolved_arg, bindings)?;
    let returned = resolve_source_function_body_value(
        scope,
        function,
        &[(&param.name, &resolved_arg)],
        bindings,
        depth + 1,
    )?;
    resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1)
}

fn resolve_pattern_source_function_call(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    functions: &[&Function],
    arg: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let enum_type =
        infer_pattern_function_enum_type(scope.module, scope.semantic_index, "source", functions)?;
    let resolved_arg = resolve_source_value_expr(scope, &enum_type, arg, bindings, depth + 1)?;
    check_source_value_type(scope, &enum_type, &resolved_arg, bindings)?;
    let (variant_name, selected_payload) = concrete_source_enum_value(
        functions[0].name.as_str(),
        "pattern dispatch",
        &resolved_arg,
    )?;
    let enum_decl = scope.semantic_index.enum_decl(scope.module, &enum_type)?;
    let selected_variant =
        scope
            .semantic_index
            .enum_variant_index(scope.module, &enum_type, variant_name)?;

    let mut wildcard = None;
    for function in functions {
        let FunctionParam::Pattern(pattern) = &function.params[0] else {
            return Err(Error::new(format!(
                "function {} cannot mix binding and pattern clauses",
                function.name
            )));
        };
        match pattern {
            Pattern::Constructor {
                name,
                binding: payload_binding,
            } => {
                let variant =
                    scope
                        .semantic_index
                        .enum_variant_index(scope.module, &enum_type, name)?;
                if variant == selected_variant {
                    let returned =
                        source_function_block_return_value(source_function_block(function)?)?;
                    let mut substitutions = Vec::new();
                    if let Some(payload_binding) = payload_binding {
                        let Some(payload) = selected_payload else {
                            return Err(Error::new(format!(
                                "function {} signature pattern {} requires a payload value",
                                function.name, name
                            )));
                        };
                        substitutions.push((&payload_binding.name, payload));
                    }
                    let returned = substitute_source_value_bindings(returned, &substitutions);
                    return resolve_source_value_expr(
                        scope,
                        expected_type,
                        &returned,
                        bindings,
                        depth + 1,
                    );
                }
            }
            Pattern::Wildcard => {
                wildcard = Some(function);
            }
        }
    }

    if let Some(function) = wildcard {
        let returned = source_function_block_return_value(source_function_block(function)?)?;
        return resolve_source_value_expr(scope, expected_type, &returned, bindings, depth + 1);
    }

    Err(Error::new(format!(
        "function {} has no pattern for variant {} of enum {}",
        functions[0].name, variant_name, enum_decl.name
    )))
}

fn resolve_source_function_body_value(
    scope: &SourceFunctionScope<'_>,
    function: &Function,
    substitutions: &[(&Identifier, &ValueExpr)],
    bindings: &[SourceValueBinding<'_>],
    depth: usize,
) -> Result<ValueExpr> {
    let body_scope = source_function_body_scope(scope, function);
    let scope = &body_scope;
    match function.body.as_ref().expect("validated function body") {
        FunctionBody::Block(body) => {
            let value = source_function_block_return_value(body)?;
            Ok(substitute_source_value_bindings(value, substitutions))
        }
        FunctionBody::Match(match_body) => {
            let Some((param_name, arg)) = substitutions.first().copied() else {
                return Err(Error::new(format!(
                    "function {} match body requires a parameter argument",
                    function.name
                )));
            };
            if match_body.scrutinee != *param_name {
                return Err(Error::new(format!(
                    "function {} match scrutinee {} must be parameter {}",
                    function.name, match_body.scrutinee, param_name
                )));
            }
            let (variant_name, selected_payload) =
                concrete_source_enum_value(function.name.as_str(), "match dispatch", arg)?;
            let FunctionParam::Binding(param) = &function.params[0] else {
                return Err(Error::new(format!(
                    "function {} match body requires a binding parameter",
                    function.name
                )));
            };
            let enum_decl = scope.semantic_index.enum_decl(scope.module, &param.ty)?;
            let selected_variant =
                scope
                    .semantic_index
                    .enum_variant_index(scope.module, &param.ty, variant_name)?;
            let subject = format!("function {}", function.name);
            let pattern_context = PatternCheckContext {
                module: scope.module,
                semantic_index: scope.semantic_index,
                enum_decl,
                enum_type: &param.ty,
                subject: &subject,
                label: "match",
                payload_context: PatternPayloadContext::SourceValue,
                binding_context: PatternBindingContext::Source { owner: &subject },
            };
            let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
            let mut wildcard = None;
            for arm in arms {
                match arm.pattern {
                    TypedMatchPattern::Variant { variant, binding }
                        if variant == selected_variant =>
                    {
                        let value = source_function_block_return_value(arm.body)?;
                        if let Some(binding) = binding {
                            let Some(payload) = selected_payload else {
                                return Err(Error::new(format!(
                                    "function {} match pattern {} requires a payload value",
                                    function.name, enum_decl.variants[variant].name
                                )));
                            };
                            let mut arm_substitutions = substitutions.to_vec();
                            arm_substitutions.push((&binding.name, payload));
                            return Ok(substitute_source_value_bindings(value, &arm_substitutions));
                        }
                        return Ok(substitute_source_value_bindings(value, substitutions));
                    }
                    TypedMatchPattern::Wildcard => {
                        wildcard = Some(arm.body);
                    }
                    _ => {}
                }
            }
            if let Some(body) = wildcard {
                let value = source_function_block_return_value(body)?;
                return Ok(substitute_source_value_bindings(value, substitutions));
            }
            Err(Error::new(format!(
                "function {} match has no arm for variant {} of enum {}",
                function.name, variant_name, enum_decl.name
            )))
        }
    }
    .and_then(|value| {
        resolve_source_value_expr(scope, &function.return_type, &value, bindings, depth + 1)
    })
}

fn source_function_block_return_value(body: &FunctionBlock) -> Result<ValueExpr> {
    if !body.statements.is_empty() {
        return Err(Error::new(
            "source function body must not perform statements",
        ));
    }
    Ok(match &body.returns {
        ReturnExpr::Value(value) => value.clone(),
        ReturnExpr::Call { name, arg } => ValueExpr::Call {
            name: name.clone(),
            arg: Box::new(arg.clone()),
        },
    })
}

fn concrete_source_enum_value<'a>(
    function_name: &str,
    usage: &str,
    value: &'a ValueExpr,
) -> Result<(&'a Identifier, Option<&'a ValueExpr>)> {
    match value {
        ValueExpr::Identifier(name) => Ok((name, None)),
        ValueExpr::EnumVariant { name, payload } => Ok((name, Some(payload.as_ref()))),
        ValueExpr::Call { .. } | ValueExpr::Record(_) => Err(Error::new(format!(
            "function {function_name} {usage} requires a concrete enum constructor argument"
        ))),
    }
}

fn substitute_source_value_bindings(
    value: ValueExpr,
    bindings: &[(&Identifier, &ValueExpr)],
) -> ValueExpr {
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|(binding_name, replacement)| {
                (name == **binding_name).then(|| (*replacement).clone())
            })
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_source_value_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_source_value_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_source_value_bindings(field.value, bindings),
                })
                .collect(),
        }),
    }
}

fn check_source_value_type(
    scope: &SourceFunctionScope<'_>,
    expected_type: &TypeRef,
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> Result<()> {
    let value_bindings = bindings
        .iter()
        .map(|binding| ValueBinding {
            name: binding.name,
            ty: binding.ty,
            label: binding.name.as_str(),
        })
        .collect::<Vec<_>>();
    canonical_source_value_with_bindings(
        scope.module,
        scope.semantic_index,
        expected_type,
        value,
        &value_bindings,
    )?;
    Ok(())
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

fn check_step(
    context: &ProcessCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<(Vec<CheckedProcessRef>, Vec<CheckedTransition>)> {
    let step_clauses = check_step_clauses(
        context.module,
        context.process,
        context.process_id,
        context.semantic_index,
        context.message_cases,
        state_space,
        types,
    )?;
    let (process_refs, process_ref_index) = collect_process_refs(
        context.process,
        context.process_id,
        context.entry_process,
        context.semantic_index,
        &step_clauses,
    )?;
    let mut step_context = StepCheckContext {
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
            &mut step_context,
            state_space,
            outputs,
            types,
            StepTransitionInput {
                current_state: clause.current_state,
                variant: clause.variant,
                message: clause.message,
                payload_binding: clause.payload_binding.as_ref(),
                state_payload_binding: clause.state_payload_binding.as_ref(),
                body: clause.body,
                declared_effects: &clause.step.effects,
            },
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
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<Vec<StepClause<'a>>> {
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    let mut explicit_clauses = vec![None; msg_enum.variants.len()];
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
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::Block(body),
                        payload_param: None,
                    },
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
                        process,
                        &msg_enum.variants,
                        &mut explicit_clauses,
                        &mut wildcard_clause,
                        pattern,
                        StepBodyClause {
                            step,
                            body: StepBodySource::Block(&arm.body),
                            payload_param: None,
                        },
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
                    process,
                    &msg_enum.variants,
                    &mut explicit_clauses,
                    &mut wildcard_clause,
                    pattern,
                    StepBodyClause {
                        step,
                        body: StepBodySource::StateMatch(match_body),
                        payload_param: None,
                    },
                )?;
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
            (Some(param), Some(checked_ty)) => Some(StepPayloadBinding {
                name: param.name.clone(),
                ty: param.ty.clone(),
                checked_ty: checked_ty.clone(),
            }),
            _ => None,
        };
        let message = message_cases.message_id(process_id, variant_id)?;
        match &clause.body {
            StepBodySource::Block(body) => {
                clauses.push(StepClause {
                    step: clause.step,
                    variant: variant_id,
                    message,
                    payload_binding,
                    current_state: None,
                    state_payload_binding: None,
                    body,
                });
            }
            StepBodySource::StateMatch(match_body) => expand_state_match_step_clauses(
                module,
                process,
                process_id,
                semantic_index,
                message_cases,
                state_space,
                types,
                clause.step,
                variant_id,
                message,
                payload_binding,
                match_body,
                &mut clauses,
            )?,
        }
    }

    Ok(clauses)
}

fn preadmit_concrete_step_state_values(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<()> {
    for step in &process.steps {
        let Some(body) = &step.body else {
            continue;
        };
        match check_step_shape(module, process, process_id, semantic_index, step)? {
            StepDispatchForm::ParameterPattern(pattern) => {
                let FunctionBody::Block(body) = body else {
                    continue;
                };
                preadmit_concrete_step_return(
                    module,
                    process,
                    semantic_index,
                    state_space,
                    types,
                    body,
                    &step_pattern_binding_names(&pattern),
                )?;
            }
            StepDispatchForm::BodyMatch => {
                let FunctionBody::Match(match_body) = body else {
                    continue;
                };
                for arm in &match_body.arms {
                    let pattern = check_step_pattern(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        &arm.pattern,
                    )?;
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        &arm.body,
                        &step_pattern_binding_names(&pattern),
                    )?;
                }
            }
            StepDispatchForm::StateMatch(pattern) => {
                let FunctionBody::Match(match_body) = body else {
                    continue;
                };
                let state_enum = semantic_index.enum_decl(module, &process.state_type)?;
                let subject = format!("process {}", process.name);
                let pattern_context = PatternCheckContext {
                    module,
                    semantic_index,
                    enum_decl: state_enum,
                    enum_type: &process.state_type,
                    subject: &subject,
                    label: "state match",
                    payload_context: PatternPayloadContext::SourceValue,
                    binding_context: PatternBindingContext::Source { owner: &subject },
                };
                let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
                let message_bindings = step_pattern_binding_names(&pattern);
                for arm in arms {
                    let mut bindings = message_bindings.clone();
                    if let TypedMatchPattern::Variant {
                        variant: _,
                        binding: Some(binding),
                    } = &arm.pattern
                    {
                        bindings.push(&binding.name);
                    } else if let TypedMatchPattern::Variant {
                        variant,
                        binding: None,
                    } = &arm.pattern
                    {
                        if state_enum.variants[*variant].payload_type.is_some() {
                            continue;
                        }
                    }
                    preadmit_concrete_step_return(
                        module,
                        process,
                        semantic_index,
                        state_space,
                        types,
                        arm.body,
                        &bindings,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn collect_concrete_state_payload_domains(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut local_types = CheckedTypeInterner::new(semantic_index);
    let mut state_space = StateSpace::new(module, semantic_index, process, &mut local_types)?;
    check_init(
        module,
        semantic_index,
        process,
        &mut state_space,
        &mut local_types,
    )?;
    preadmit_concrete_step_state_values(
        module,
        process,
        process_id,
        semantic_index,
        &mut state_space,
        &mut local_types,
    )?;

    let mut domains: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for state in state_space.values() {
        if let Some(payload) = state.payload() {
            domains
                .entry(payload.ty().label().to_string())
                .or_default()
                .insert(payload.label().to_string());
        }
    }
    Ok(domains)
}

fn step_pattern_binding_names(pattern: &StepPattern) -> Vec<&Identifier> {
    match pattern {
        StepPattern::Variant {
            binding: Some(binding),
            ..
        } => vec![&binding.name],
        StepPattern::Variant { binding: None, .. } | StepPattern::Wildcard => Vec::new(),
    }
}

fn preadmit_concrete_step_return(
    module: &Module,
    process: &Process,
    semantic_index: &SemanticIndex,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    body: &FunctionBlock,
    binding_names: &[&Identifier],
) -> Result<()> {
    let state_arg = match &body.returns {
        ReturnExpr::Call { name, arg }
            if name.as_str() == "Stop"
                || name.as_str() == "Continue"
                || name.as_str() == "Panic" =>
        {
            arg
        }
        _ => return Ok(()),
    };
    if matches!(state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
        || binding_names
            .iter()
            .any(|binding| source_value_uses_binding(state_arg, binding))
    {
        return Ok(());
    }

    let function_scope = SourceFunctionScope {
        module,
        process_name: Some(&process.name),
        process_functions: &process.functions,
        semantic_index,
    };
    let state_arg =
        resolve_source_value_expr(&function_scope, &process.state_type, state_arg, &[], 0)?;
    state_space.resolve_state_value(semantic_index, types, &state_arg)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn expand_state_match_step_clauses<'a>(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    step: &'a Function,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_binding: Option<StepPayloadBinding>,
    match_body: &'a super::ast::Match,
    clauses: &mut Vec<StepClause<'a>>,
) -> Result<()> {
    let state_enum = semantic_index.enum_decl(module, &process.state_type)?;
    let subject = format!("process {}", process.name);
    let pattern_context = PatternCheckContext {
        module,
        semantic_index,
        enum_decl: state_enum,
        enum_type: &process.state_type,
        subject: &subject,
        label: "state match",
        payload_context: PatternPayloadContext::SourceValue,
        binding_context: PatternBindingContext::Source { owner: &subject },
    };
    let arms = check_typed_match_arms(&pattern_context, &match_body.arms)?;
    let explicit_variants = arms
        .iter()
        .filter_map(|arm| match arm.pattern {
            TypedMatchPattern::Variant { variant, .. } => Some(variant),
            TypedMatchPattern::Wildcard => None,
        })
        .collect::<BTreeSet<_>>();

    for arm in arms {
        let cases = state_match_arm_cases(
            module,
            process,
            process_id,
            semantic_index,
            message_cases,
            state_space,
            types,
            state_enum,
            &explicit_variants,
            &arm.pattern,
        )?;
        for (current_state, state_payload_binding) in cases {
            validate_state_payload_binding_name(
                process,
                payload_binding.as_ref(),
                state_payload_binding.as_ref(),
            )?;
            clauses.push(StepClause {
                step,
                variant,
                message,
                payload_binding: payload_binding.clone(),
                current_state: Some(current_state),
                state_payload_binding,
                body: arm.body,
            });
        }
    }
    Ok(())
}

fn validate_state_payload_binding_name(
    process: &Process,
    message_payload_binding: Option<&StepPayloadBinding>,
    state_payload_binding: Option<&StepStatePayloadBinding>,
) -> Result<()> {
    let (Some(message_payload_binding), Some(state_payload_binding)) =
        (message_payload_binding, state_payload_binding)
    else {
        return Ok(());
    };
    if message_payload_binding.name == state_payload_binding.name {
        return Err(Error::new(format!(
            "process {} state payload binding {} conflicts with message payload binding",
            process.name, state_payload_binding.name
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn state_match_arm_cases(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    state_enum: &Enum,
    explicit_variants: &BTreeSet<usize>,
    pattern: &TypedMatchPattern,
) -> Result<Vec<(CheckedStateId, Option<StepStatePayloadBinding>)>> {
    match pattern {
        TypedMatchPattern::Variant { variant, binding } => {
            let variant_decl = &state_enum.variants[*variant];
            match (&variant_decl.payload_type, binding) {
                (None, None) => {
                    let value = ValueExpr::Identifier(variant_decl.name.clone());
                    let state = state_space.resolve_state_value(semantic_index, types, &value)?;
                    Ok(vec![(state, None)])
                }
                (None, Some(_)) => unreachable!("fieldless payload binding is checked earlier"),
                (Some(_), None) => Err(Error::new(format!(
                    "process {} state match pattern {} requires a payload binding",
                    process.name, variant_decl.name
                ))),
                (Some(payload_type), Some(binding)) => {
                    let checked_ty = types.intern(payload_type)?;
                    let payloads = state_match_payload_domain(
                        module,
                        process,
                        process_id,
                        semantic_index,
                        message_cases,
                        state_space,
                        payload_type,
                        &checked_ty,
                    )?;
                    let state_value = ValueExpr::EnumVariant {
                        name: variant_decl.name.clone(),
                        payload: Box::new(ValueExpr::Identifier(binding.name.clone())),
                    };
                    payloads
                        .into_iter()
                        .map(|payload| {
                            let state = state_space.resolve_state_value_with_bindings(
                                semantic_index,
                                types,
                                &state_value,
                                &[ValueBinding {
                                    name: &binding.name,
                                    ty: &binding.ty,
                                    label: payload.label(),
                                }],
                            )?;
                            Ok((
                                state,
                                Some(StepStatePayloadBinding {
                                    name: binding.name.clone(),
                                    ty: binding.ty.clone(),
                                    checked_ty: checked_ty.clone(),
                                    label: payload.label().to_string(),
                                }),
                            ))
                        })
                        .collect()
                }
            }
        }
        TypedMatchPattern::Wildcard => {
            let mut cases = Vec::new();
            for (variant_index, variant_decl) in state_enum.variants.iter().enumerate() {
                if explicit_variants.contains(&variant_index) {
                    continue;
                }
                match &variant_decl.payload_type {
                    None => {
                        let value = ValueExpr::Identifier(variant_decl.name.clone());
                        let state =
                            state_space.resolve_state_value(semantic_index, types, &value)?;
                        cases.push((state, None));
                    }
                    Some(payload_type) => {
                        let checked_ty = types.intern(payload_type)?;
                        let payload_name = Identifier::new("__state_payload")?;
                        let state_value = ValueExpr::EnumVariant {
                            name: variant_decl.name.clone(),
                            payload: Box::new(ValueExpr::Identifier(payload_name.clone())),
                        };
                        for payload in state_match_payload_domain(
                            module,
                            process,
                            process_id,
                            semantic_index,
                            message_cases,
                            state_space,
                            payload_type,
                            &checked_ty,
                        )? {
                            let state = state_space.resolve_state_value_with_bindings(
                                semantic_index,
                                types,
                                &state_value,
                                &[ValueBinding {
                                    name: &payload_name,
                                    ty: payload_type,
                                    label: payload.label(),
                                }],
                            )?;
                            cases.push((state, None));
                        }
                    }
                }
            }
            Ok(cases)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn state_match_payload_domain(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_cases: &MessageCaseTable,
    state_space: &StateSpace<'_>,
    payload_type: &TypeRef,
    checked_payload_type: &CheckedTypeRef,
) -> Result<Vec<CheckedPayloadValue>> {
    let mut payloads = BTreeMap::new();
    for state in state_space.values() {
        if let Some(payload) = state.payload() {
            if payload.ty() == checked_payload_type {
                payloads.insert(payload.label().to_string(), payload.clone());
            }
        }
    }
    let msg_enum = semantic_index.enum_decl(module, &process.msg_type)?;
    for (variant_index, message_variant) in msg_enum.variants.iter().enumerate() {
        let Some(message_payload_type) = &message_variant.payload_type else {
            continue;
        };
        if !semantic_index.same_type(message_payload_type, payload_type) {
            continue;
        }
        let variant_id = CheckedMessageVariantId::from_index(variant_index)?;
        for payload in message_cases.payload_values(process_id, variant_id)? {
            payloads.insert(payload.label().to_string(), payload.clone());
        }
    }
    Ok(payloads.into_values().collect())
}

fn insert_step_body_clause<'a>(
    process: &Process,
    message_variants: &[EnumVariant],
    explicit_clauses: &mut [Option<StepBodyClause<'a>>],
    wildcard_clause: &mut Option<StepBodyClause<'a>>,
    pattern: StepPattern,
    mut clause: StepBodyClause<'a>,
) -> Result<()> {
    match pattern {
        StepPattern::Variant { message, binding } => {
            clause.payload_param = binding;
            if explicit_clauses[message.index()].replace(clause).is_some() {
                return Err(Error::new(format!(
                    "process {} declares duplicate step pattern for message {}",
                    process.name,
                    message_variants[message.index()].name
                )));
            }
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

fn step_discovery_clauses<'a>(
    module: &Module,
    process: &'a Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &'a Function,
) -> Result<Vec<StepDiscoveryClause<'a>>> {
    let Some(body) = &step.body else {
        return Ok(Vec::new());
    };
    match check_step_shape(module, process, process_id, semantic_index, step)? {
        StepDispatchForm::ParameterPattern(pattern) => {
            let FunctionBody::Block(body) = body else {
                return Err(Error::new("step parameter pattern must use a block body"));
            };
            Ok(vec![StepDiscoveryClause {
                pattern,
                body,
                state_payload_bindings: Vec::new(),
            }])
        }
        StepDispatchForm::BodyMatch => {
            let FunctionBody::Match(match_body) = body else {
                return Err(Error::new("match step must use a match body"));
            };
            match_body
                .arms
                .iter()
                .map(|arm| {
                    Ok(StepDiscoveryClause {
                        pattern: check_step_pattern(
                            module,
                            process,
                            process_id,
                            semantic_index,
                            &arm.pattern,
                        )?,
                        body: &arm.body,
                        state_payload_bindings: Vec::new(),
                    })
                })
                .collect()
        }
        StepDispatchForm::StateMatch(pattern) => {
            let FunctionBody::Match(match_body) = body else {
                return Err(Error::new("state match step must use a match body"));
            };
            let state_enum = semantic_index.enum_decl(module, &process.state_type)?;
            let subject = format!("process {}", process.name);
            let pattern_context = PatternCheckContext {
                module,
                semantic_index,
                enum_decl: state_enum,
                enum_type: &process.state_type,
                subject: &subject,
                label: "state match",
                payload_context: PatternPayloadContext::SourceValue,
                binding_context: PatternBindingContext::Source { owner: &subject },
            };
            check_typed_match_arms(&pattern_context, &match_body.arms)?
                .into_iter()
                .map(|arm| {
                    let state_payload_bindings = match &arm.pattern {
                        TypedMatchPattern::Variant {
                            variant,
                            binding: Some(binding),
                        } => vec![StatePayloadDiscoveryBinding {
                            name: binding.name.clone(),
                            ty: state_enum.variants[*variant]
                                .payload_type
                                .clone()
                                .ok_or_else(|| {
                                    Error::new(format!(
                                        "process {} state match pattern {} does not carry a payload",
                                        process.name, state_enum.variants[*variant].name
                                    ))
                                })?,
                        }],
                        TypedMatchPattern::Variant { binding: None, .. }
                        | TypedMatchPattern::Wildcard => Vec::new(),
                    };
                    Ok(StepDiscoveryClause {
                        pattern: pattern.clone(),
                        body: arm.body,
                        state_payload_bindings,
                    })
                })
                .collect()
        }
    }
}

fn collect_step_blocks(step: &Function) -> Vec<&FunctionBlock> {
    match &step.body {
        Some(FunctionBody::Block(body)) => vec![body],
        Some(FunctionBody::Match(match_body)) => {
            match_body.arms.iter().map(|arm| &arm.body).collect()
        }
        None => Vec::new(),
    }
}

fn check_step_match_scrutinee_parameter<'a>(
    process: &Process,
    step: &'a Function,
    match_scrutinee: &Identifier,
) -> Result<&'a Param> {
    let Some(FunctionParam::Binding(message_param)) = step.params.get(1) else {
        return Err(Error::new(format!(
            "process {} match step must declare a typed message parameter",
            process.name
        )));
    };
    if message_param.name.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "process {} message parameter {} conflicts with a step parameter name",
            process.name, message_param.name
        )));
    }
    if message_param.name != *match_scrutinee {
        return Err(Error::new(format!(
            "process {} match scrutinee {} must be the step message parameter {}",
            process.name, match_scrutinee, message_param.name
        )));
    }
    Ok(message_param)
}

fn collect_explicit_step_variants(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
) -> Result<BTreeSet<CheckedMessageVariantId>> {
    let mut variants = BTreeSet::new();
    for step in &process.steps {
        for clause in step_discovery_clauses(module, process, process_id, semantic_index, step)? {
            if let StepPattern::Variant { message, .. } = clause.pattern {
                variants.insert(message);
            }
        }
    }
    Ok(variants)
}

fn matching_message_cases<'a>(
    cases: &'a [DiscoveredMessageCase],
    pattern: &StepPattern,
    explicit_variants: &BTreeSet<CheckedMessageVariantId>,
) -> Vec<&'a DiscoveredMessageCase> {
    cases
        .iter()
        .filter(|case| match pattern {
            StepPattern::Variant { message, .. } => case.variant() == *message,
            StepPattern::Wildcard => !explicit_variants.contains(&case.variant()),
        })
        .collect()
}

fn payload_value_bindings<'a>(
    pattern: &'a StepPattern,
    case: &'a DiscoveredMessageCase,
) -> Vec<DiscoveryValueBinding> {
    match (pattern, case.payload()) {
        (
            StepPattern::Variant {
                binding: Some(param),
                ..
            },
            Some(payload),
        ) => vec![DiscoveryValueBinding {
            name: param.name.clone(),
            ty: param.ty.clone(),
            label: payload.label().to_string(),
        }],
        _ => Vec::new(),
    }
}

fn resolve_send_target_process_for_discovery(
    process: &Process,
    semantic_index: &SemanticIndex,
    process_refs: &BTreeMap<Identifier, CheckedProcessId>,
    pattern: &StepPattern,
    target: &Identifier,
) -> Result<CheckedProcessId> {
    if let Some(target_process) = process_refs.get(target) {
        return Ok(*target_process);
    }
    if let StepPattern::Variant {
        binding: Some(param),
        ..
    } = pattern
    {
        if param.name == *target {
            return semantic_index
                .process_ref_target_type(&param.ty)?
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} send target {} is not a process reference payload",
                        process.name, target
                    ))
                });
        }
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        process.name, target
    )))
}

fn check_step_pattern(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_pattern: &Pattern,
) -> Result<StepPattern> {
    step_pattern_from_typed(check_step_typed_pattern(
        module,
        process,
        process_id,
        semantic_index,
        message_pattern,
    )?)
}

fn check_step_typed_pattern(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    message_pattern: &Pattern,
) -> Result<TypedMatchPattern> {
    match message_pattern {
        Pattern::Constructor { name, binding } => {
            let message = semantic_index.message_id_for_step_pattern(module, process_id, name)?;
            let variant = semantic_index.message_variant(module, process_id, message)?;
            let binding =
                check_step_payload_pattern(process, semantic_index, variant, binding.as_ref())?;
            Ok(TypedMatchPattern::Variant {
                variant: message.index(),
                binding,
            })
        }
        Pattern::Wildcard => Ok(TypedMatchPattern::Wildcard),
    }
}

fn step_pattern_from_typed(pattern: TypedMatchPattern) -> Result<StepPattern> {
    match pattern {
        TypedMatchPattern::Variant { variant, binding } => Ok(StepPattern::Variant {
            message: CheckedMessageVariantId::from_index(variant)?,
            binding,
        }),
        TypedMatchPattern::Wildcard => Ok(StepPattern::Wildcard),
    }
}

fn check_step_shape(
    module: &Module,
    process: &Process,
    process_id: CheckedProcessId,
    semantic_index: &SemanticIndex,
    step: &Function,
) -> Result<StepDispatchForm> {
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

    if let Some(FunctionBody::Match(match_body)) = &step.body {
        if match_body.scrutinee.as_str() == STEP_STATE_PARAMETER_NAME {
            let FunctionParam::Pattern(message_pattern) = &step.params[1] else {
                return Err(Error::new(
                    "state match step must declare a message constructor pattern",
                ));
            };
            return Ok(StepDispatchForm::StateMatch(check_step_pattern(
                module,
                process,
                process_id,
                semantic_index,
                message_pattern,
            )?));
        }
        let message_param =
            check_step_match_scrutinee_parameter(process, step, &match_body.scrutinee)?;
        if !semantic_index.same_type(&message_param.ty, &process.msg_type) {
            return Err(Error::new(format!(
                "process {} message parameter {} has type {}, expected {}",
                process.name, message_param.name, message_param.ty, process.msg_type
            )));
        }
        return Ok(StepDispatchForm::BodyMatch);
    }

    let FunctionParam::Pattern(message_pattern) = &step.params[1] else {
        return Err(Error::new(
            "step second parameter must be a message constructor pattern or wildcard pattern",
        ));
    };
    Ok(StepDispatchForm::ParameterPattern(check_step_pattern(
        module,
        process,
        process_id,
        semantic_index,
        message_pattern,
    )?))
}

fn check_step_payload_pattern(
    process: &Process,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    binding: Option<&Param>,
) -> Result<Option<PatternPayloadParam>> {
    check_pattern_payload_binding(
        semantic_index,
        variant,
        binding,
        "step pattern",
        PatternPayloadContext::StepPattern,
        PatternBindingContext::Step { process },
    )
}

fn pattern_binding_subject(context: PatternBindingContext<'_>) -> String {
    match context {
        PatternBindingContext::Step { process } => format!("process {}", process.name),
        PatternBindingContext::Source { owner } => owner.to_string(),
    }
}

fn validate_pattern_binding_name(
    context: PatternBindingContext<'_>,
    semantic_index: &SemanticIndex,
    binding: &Identifier,
) -> Result<()> {
    let subject = pattern_binding_subject(context);
    if binding.as_str() == STEP_STATE_PARAMETER_NAME {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a reserved state parameter name"
        )));
    }
    if semantic_index.process_id(binding).is_ok() {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a process declaration"
        )));
    }
    if semantic_index.identifier_conflicts_with_declared_value(binding) {
        return Err(Error::new(format!(
            "{subject} payload binding {binding} conflicts with a declared type or value constructor"
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
            clause.state_payload_binding.as_ref(),
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
        for body in collect_step_blocks(step) {
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
    state_payload_binding: Option<&StepStatePayloadBinding>,
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
        if let Some(binding) = state_payload_binding {
            if binding.name == *name {
                return Err(Error::new(format!(
                    "process {} process reference {} conflicts with state payload binding",
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
    context: &mut StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    outputs: &mut OutputPool,
    types: &mut CheckedTypeInterner<'_>,
    input: StepTransitionInput<'_>,
) -> Result<CheckedTransition> {
    let payload_template_binding = input.payload_binding.map(|binding| ValueTemplateBinding {
        name: &binding.name,
        ty: &binding.ty,
        checked_ty: &binding.checked_ty,
        source: ValueTemplateSource::ReceivedPayload,
    });
    let state_template_binding = input
        .state_payload_binding
        .map(|binding| ValueTemplateBinding {
            name: &binding.name,
            ty: &binding.ty,
            checked_ty: &binding.checked_ty,
            source: ValueTemplateSource::CurrentStatePayload,
        });
    let template_bindings = payload_template_binding
        .iter()
        .chain(state_template_binding.iter())
        .copied()
        .collect::<Vec<_>>();
    let function_scope = SourceFunctionScope {
        module: context.module,
        process_name: Some(&context.process.name),
        process_functions: &context.process.functions,
        semantic_index: context.semantic_index,
    };
    let mut source_bindings = Vec::new();
    if let Some(binding) = input.payload_binding {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    if let Some(binding) = input.state_payload_binding {
        source_bindings.push(SourceValueBinding {
            name: &binding.name,
            ty: &binding.ty,
        });
    }
    let mut actions = Vec::with_capacity(input.body.statements.len());
    for statement in &input.body.statements {
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
                let send_target =
                    resolve_checked_send_target(context, input.payload_binding, target)?;
                let message_id = resolve_send_message_case(
                    context,
                    types,
                    send_target.target_process,
                    message,
                    payload.as_ref(),
                    &source_bindings,
                    &template_bindings,
                )?;
                actions.push(CheckedAction::Send {
                    target: send_target.target,
                    message: message_id.message,
                    payload: message_id.payload,
                });
            }
        }
    }

    let (step_result, state_arg) = match &input.body.returns {
        ReturnExpr::Call { name, arg } if name.as_str() == "Stop" => (CheckedStepResult::Stop, arg),
        ReturnExpr::Call { name, arg } if name.as_str() == "Continue" => {
            (CheckedStepResult::Continue, arg)
        }
        ReturnExpr::Call { name, arg } if name.as_str() == "Panic" => {
            (CheckedStepResult::Panic, arg)
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)",
            ));
        }
    };
    let state_arg = resolve_source_value_expr(
        &function_scope,
        &context.process.state_type,
        state_arg,
        &source_bindings,
        0,
    )?;
    let next_state = if matches!(&state_arg, ValueExpr::Identifier(name) if name.as_str() == STEP_STATE_PARAMETER_NAME)
    {
        CheckedNextState::Current
    } else if template_bindings
        .iter()
        .any(|binding| source_value_uses_binding(&state_arg, binding.name))
    {
        let template = checked_value_template_with_binding(
            context.module,
            context.semantic_index,
            types,
            &context.process.state_type,
            &state_arg,
            &template_bindings,
        )?;
        populate_template_state_values(
            context,
            state_space,
            types,
            input.variant,
            &state_arg,
            input.payload_binding,
            input.state_payload_binding,
        )?;
        CheckedNextState::Template(template)
    } else {
        CheckedNextState::Value(state_space.resolve_state_value(
            context.semantic_index,
            types,
            &state_arg,
        )?)
    };

    Ok(CheckedTransition::new(CheckedTransitionParts {
        current_state: input.current_state,
        message: input.message,
        step_result,
        next_state,
        effects: input.declared_effects.to_vec(),
        actions,
    }))
}

struct ResolvedCheckedSendTarget {
    target: CheckedSendTarget,
    target_process: CheckedProcessId,
}

fn resolve_checked_send_target(
    context: &StepCheckContext<'_>,
    payload_binding: Option<&StepPayloadBinding>,
    target: &Identifier,
) -> Result<ResolvedCheckedSendTarget> {
    if let Some(binding) = context.process_ref_index.get(target) {
        return Ok(ResolvedCheckedSendTarget {
            target: CheckedSendTarget::ProcessRef(binding.id),
            target_process: binding.target,
        });
    }
    if let Some(binding) = payload_binding {
        if binding.name == *target {
            let target_process = context
                .semantic_index
                .process_ref_target_type(&binding.ty)?
                .ok_or_else(|| {
                    Error::new(format!(
                        "process {} send target {} is not a process reference payload",
                        context.process.name, target
                    ))
                })?;
            return Ok(ResolvedCheckedSendTarget {
                target: CheckedSendTarget::ReceivedPayload {
                    ty: binding.checked_ty.clone(),
                    target: target_process,
                },
                target_process,
            });
        }
    }
    Err(Error::new(format!(
        "process {} sends to undeclared process reference {}",
        context.process.name, target
    )))
}

struct CheckedSendMessage {
    message: CheckedMessageId,
    payload: Option<CheckedValueTemplate>,
}

fn resolve_send_message_case(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    target_process: CheckedProcessId,
    message: &Identifier,
    payload: Option<&ValueExpr>,
    source_bindings: &[SourceValueBinding<'_>],
    bindings: &[ValueTemplateBinding<'_>],
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
            )));
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "process {} sends message {} without required payload",
                context.process.name, variant_decl.name
            )));
        }
        (Some(payload_type), Some(payload)) => {
            let resolved_payload = {
                let function_scope = SourceFunctionScope {
                    module: context.module,
                    process_name: Some(&context.process.name),
                    process_functions: &context.process.functions,
                    semantic_index: context.semantic_index,
                };
                resolve_source_value_expr(
                    &function_scope,
                    payload_type,
                    payload,
                    source_bindings,
                    0,
                )?
            };
            Some(checked_send_payload_template(
                context,
                types,
                payload_type,
                &resolved_payload,
                bindings,
            )?)
        }
    };
    Ok(CheckedSendMessage {
        message: context.message_cases.message_id(target_process, variant)?,
        payload,
    })
}

fn checked_send_payload_template(
    context: &mut StepCheckContext<'_>,
    types: &mut CheckedTypeInterner<'_>,
    expected_type: &TypeRef,
    payload: &ValueExpr,
    bindings: &[ValueTemplateBinding<'_>],
) -> Result<CheckedValueTemplate> {
    if let Some(target_process) = context
        .semantic_index
        .process_ref_target_type(expected_type)?
    {
        let ValueExpr::Identifier(name) = payload else {
            return Err(Error::new(format!(
                "process {} sends process reference payload of type {} using a non-reference value",
                context.process.name, expected_type
            )));
        };
        if let Some(binding) = bindings.iter().find(|binding| name == binding.name) {
            if binding.ty == expected_type {
                return Ok(match binding.source {
                    ValueTemplateSource::ReceivedPayload => CheckedValueTemplate::ReceivedPayload {
                        ty: binding.checked_ty.clone(),
                    },
                    ValueTemplateSource::CurrentStatePayload => {
                        CheckedValueTemplate::CurrentStatePayload {
                            ty: binding.checked_ty.clone(),
                        }
                    }
                });
            }
            return Err(Error::new(format!(
                "value binding {} has type {}, expected {}",
                binding.name, binding.ty, expected_type
            )));
        }
        let process_ref = context.process_ref_index.get(name).ok_or_else(|| {
            Error::new(format!(
                "process {} payload {} is not a bound process reference",
                context.process.name, name
            ))
        })?;
        if process_ref.target != target_process {
            return Err(Error::new(format!(
                "process {} payload {} targets process id {}, expected {}",
                context.process.name,
                name,
                process_ref.target.as_u32(),
                target_process.as_u32()
            )));
        }
        return Ok(CheckedValueTemplate::ProcessRef {
            ty: types.intern(expected_type)?,
            target: target_process,
            process_ref: process_ref.id,
        });
    }

    checked_value_template_with_binding(
        context.module,
        context.semantic_index,
        types,
        expected_type,
        payload,
        bindings,
    )
}

fn populate_template_state_values(
    context: &StepCheckContext<'_>,
    state_space: &mut StateSpace<'_>,
    types: &mut CheckedTypeInterner<'_>,
    variant: CheckedMessageVariantId,
    state_arg: &ValueExpr,
    payload_binding: Option<&StepPayloadBinding>,
    state_payload_binding: Option<&StepStatePayloadBinding>,
) -> Result<()> {
    if let Some(binding) = payload_binding {
        for payload in context
            .message_cases
            .payload_values(context.process_id, variant)?
        {
            let mut bindings = vec![ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: payload.label(),
            }];
            if let Some(state_binding) = state_payload_binding {
                bindings.push(ValueBinding {
                    name: &state_binding.name,
                    ty: &state_binding.ty,
                    label: &state_binding.label,
                });
            }
            state_space.resolve_state_value_with_bindings(
                context.semantic_index,
                types,
                state_arg,
                &bindings,
            )?;
        }
        return Ok(());
    }
    if let Some(binding) = state_payload_binding {
        state_space.resolve_state_value_with_bindings(
            context.semantic_index,
            types,
            state_arg,
            &[ValueBinding {
                name: &binding.name,
                ty: &binding.ty,
                label: &binding.label,
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
