mod authority;
mod init;
mod message_cases;
mod outputs;
mod payload_patterns;
mod preflight;
mod source_functions;
mod state_space;
mod static_validation;
mod steps;
mod supervision;
mod symbols;
mod types;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mantle_artifact::{
    ArtifactValue, MAX_ACTIONS_PER_PROCESS, MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS,
    MAX_PROCESS_COUNT, MAX_SPAWN_SITES_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS,
    MapProjectionMode,
};

use super::ast::{
    CollectionPatternBinding, ConstructorPayloadPattern, Determinism, Effect, Enum, EnumVariant,
    ForEachItem, Function, FunctionBlock, FunctionBody, FunctionParam, Identifier, ListPattern,
    ListValue, MapPattern, MapPatternCompleteness, MapValue, MapValueEntry, Match, MatchArm,
    Module, Param, Pattern, Process, Record, RecordPatternField, RecordValue, RecordValueField,
    ReturnExpr, Statement, TypeRef, ValueExpr,
};
pub(in crate::language::checker) use super::checked::CheckedCapabilityDescriptor;
use super::checked::{
    CheckedAction, CheckedEnumVariantId, CheckedLoopElement, CheckedLoopElementId,
    CheckedMessageCase, CheckedMessageId, CheckedMessageVariantId, CheckedNextState,
    CheckedPayloadValue, CheckedProcess, CheckedProcessId, CheckedProcessParts, CheckedProcessRef,
    CheckedProcessRefId, CheckedProgram, CheckedProgramParts, CheckedSendTarget, CheckedStateId,
    CheckedStepResult, CheckedTransition, CheckedTransitionParts, CheckedTypeRef,
    CheckedValueTemplate, checked_action_count,
};
use super::diagnostic::{Error, Result};
use super::{LIST_TYPE, MAP_TYPE, MAX_VALUE_NESTING, PROC_RESULT_TYPE, PROCESS_REF_TYPE};
pub(in crate::language::checker) use authority::{AuthorityBinding, SpawnSiteAllocator};
use authority::{collect_authorities, validate_authority_usage};
use init::check_init;
use message_cases::{DiscoveredMessageCase, MessageCaseTable};
use outputs::OutputPool;
pub(in crate::language::checker) use payload_patterns::*;
use preflight::{validate_enum_variant_counts, validate_process_declarations_before_message_cases};
use source_functions::{
    check_source_value_type, resolve_source_value_expr, validate_source_function_declarations,
};
use state_space::{
    StateSpace, ValueBinding, ValueTemplateBinding, ValueTemplateSource,
    canonical_source_value_with_bindings, checked_value_template_with_binding,
    source_value_uses_binding,
};
use static_validation::validate_action_references;
use steps::{check_step, pattern_binding_subject, validate_pattern_binding_name};
use supervision::{SupervisorChildBinding, check_supervisors};
use symbols::{CollectionType, SemanticIndex};
use types::CheckedTypeInterner;

const STEP_STATE_PARAMETER_NAME: &str = "state";
pub(super) const CHECKED_TYPE_LABEL_PREFIX: &str = "__strata_checked_";

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
    supervisor_child_index: &'a BTreeMap<Identifier, SupervisorChildBinding>,
    authority_index: &'a BTreeMap<Identifier, AuthorityBinding>,
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
    payload_params: Vec<PatternPayloadParam>,
    payload_guard: Option<PatternPayloadGuard>,
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
    payload_guard: Option<CheckedPayloadValue>,
    payload_bindings: Vec<StepPayloadBinding>,
    current_state: Option<CheckedStateId>,
    state_payload_bindings: Vec<StepStatePayloadBinding>,
    body: &'a FunctionBlock,
}

struct StepTransitionInput<'a> {
    current_state: Option<CheckedStateId>,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
    payload_guard: Option<&'a CheckedPayloadValue>,
    payload_bindings: &'a [StepPayloadBinding],
    state_payload_bindings: &'a [StepStatePayloadBinding],
    body: &'a FunctionBlock,
    declared_effects: &'a [Effect],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepPattern {
    Variant {
        message: CheckedMessageVariantId,
        bindings: Vec<PatternPayloadParam>,
        payload_guard: Option<PatternPayloadGuard>,
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
struct PayloadProjectionSegment {
    ty: TypeRef,
    kind: PayloadProjectionSegmentKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PayloadProjectionSegmentKind {
    EnumPayload {
        enum_ty: TypeRef,
        variant: CheckedEnumVariantId,
    },
    RecordField {
        field: Identifier,
    },
    ListIndex {
        index: usize,
        len: usize,
    },
    ListPrefixIndex {
        index: usize,
        prefix_len: usize,
    },
    ListRest {
        prefix_len: usize,
    },
    MapValue {
        key: ArtifactValue,
        keys: Arc<[ArtifactValue]>,
        projection: MapProjectionMode,
    },
    MapRest {
        excluded_keys: Arc<[ArtifactValue]>,
    },
}

impl PayloadProjectionSegment {
    fn enum_payload(enum_ty: TypeRef, ty: TypeRef, variant: CheckedEnumVariantId) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::EnumPayload { enum_ty, variant },
        }
    }

    fn record_field(ty: TypeRef, field: Identifier) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::RecordField { field },
        }
    }

    fn list_index(ty: TypeRef, index: usize, len: usize) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::ListIndex { index, len },
        }
    }

    fn list_prefix_index(ty: TypeRef, index: usize, prefix_len: usize) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::ListPrefixIndex { index, prefix_len },
        }
    }

    fn list_rest(ty: TypeRef, prefix_len: usize) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::ListRest { prefix_len },
        }
    }

    fn map_value(
        ty: TypeRef,
        key: ArtifactValue,
        keys: Arc<[ArtifactValue]>,
        projection: MapProjectionMode,
    ) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::MapValue {
                key,
                keys,
                projection,
            },
        }
    }

    fn map_rest(ty: TypeRef, excluded_keys: Arc<[ArtifactValue]>) -> Self {
        Self {
            ty,
            kind: PayloadProjectionSegmentKind::MapRest { excluded_keys },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PayloadBindingPath {
    segments: Vec<PayloadProjectionSegment>,
}

impl PayloadBindingPath {
    fn whole() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    fn then(&self, segment: PayloadProjectionSegment) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment);
        Self { segments }
    }

    fn is_whole(&self) -> bool {
        self.segments.is_empty()
    }

    fn segments(&self) -> &[PayloadProjectionSegment] {
        &self.segments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternPayloadParam {
    name: Identifier,
    ty: TypeRef,
    path: PayloadBindingPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternPayloadGuard {
    enum_ty: TypeRef,
    variant: CheckedEnumVariantId,
    payload: Option<Box<PatternPayloadGuard>>,
}

struct NestedPatternBindingScope<'a, 'seen> {
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    binding_context: PatternBindingContext<'a>,
    context: &'a str,
    seen_bindings: &'seen mut BTreeSet<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyConstructorPattern {
    Allow,
    Reject,
}

impl NestedPatternBindingScope<'_, '_> {
    fn subject(&self) -> String {
        pattern_binding_subject(self.binding_context)
    }
}

#[derive(Clone, Copy)]
struct MapPatternType<'a> {
    key: &'a TypeRef,
    value: &'a TypeRef,
    capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatePayloadDiscoveryBinding {
    name: Identifier,
    payload_ty: TypeRef,
    ty: TypeRef,
    path: PayloadBindingPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveryValueBinding {
    name: Identifier,
    ty: TypeRef,
    label: String,
    value: Option<ArtifactValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConcreteStatePayloadDomain {
    ty: TypeRef,
    values: Vec<ArtifactValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PayloadDomainKey {
    value: Option<ArtifactValue>,
    process_ref_target: Option<CheckedProcessId>,
    process_ref_label: Option<String>,
}

impl PayloadDomainKey {
    fn from_payload(payload: &CheckedPayloadValue) -> Result<Self> {
        if let Some(value) = payload.value() {
            return Ok(Self {
                value: Some(value.clone()),
                process_ref_target: None,
                process_ref_label: None,
            });
        }
        let process_ref = payload.process_ref_payload().ok_or_else(|| {
            Error::new(
                "checked payload must carry either an artifact value or process reference metadata",
            )
        })?;
        Ok(Self {
            value: None,
            process_ref_target: Some(process_ref.target()),
            process_ref_label: Some(payload.label().to_string()),
        })
    }
}

struct SendPayloadDiscoveryContext<'a, 'types, 'semantic> {
    sender_cases: &'a [DiscoveredMessageCase],
    concrete_state_payloads: &'a [ConcreteStatePayloadDomain],
    module: &'semantic Module,
    semantic_index: &'semantic SemanticIndex,
    process_refs: &'a BTreeMap<Identifier, CheckedProcessId>,
    types: &'types mut CheckedTypeInterner<'semantic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepPayloadBinding {
    name: Identifier,
    payload_ty: TypeRef,
    ty: TypeRef,
    checked_payload_ty: CheckedTypeRef,
    checked_ty: CheckedTypeRef,
    path: PayloadBindingPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StepStatePayloadBinding {
    name: Identifier,
    payload_ty: TypeRef,
    ty: TypeRef,
    checked_payload_ty: CheckedTypeRef,
    checked_ty: CheckedTypeRef,
    label: String,
    value: ArtifactValue,
    path: PayloadBindingPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypedMatchPattern {
    Variant {
        variant: usize,
        bindings: Vec<PatternPayloadParam>,
        payload_guard: Option<PatternPayloadGuard>,
    },
    Wildcard,
}

struct TypedMatchArm<'a> {
    pattern: TypedMatchPattern,
    body: &'a FunctionBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PayloadSensitivePattern {
    variant: usize,
    payload_guard: Option<PatternPayloadGuard>,
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
    process_refs: Option<&'a BTreeMap<Identifier, CheckedProcessId>>,
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
    EnumPattern,
    RecordPattern,
    ListPattern,
    MapPattern,
}

pub fn check_module(module: Module) -> Result<CheckedProgram> {
    if !module.imports.is_empty() {
        return Err(Error::new("imports require checking from a source program"));
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
    validate_enum_variant_counts(&module)?;
    let mut types = CheckedTypeInterner::new(&module, &semantic_index);
    let entry_process = semantic_index
        .process_id_by_name("Main")
        .map_err(|_| Error::new("entry process Main is not declared"))?;
    validate_source_function_declarations(&module, &semantic_index)?;
    validate_process_declarations_before_message_cases(&module, &semantic_index, entry_process)?;
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
    let checked_types = types.into_types();

    Ok(CheckedProgram::new(CheckedProgramParts {
        module,
        entry_process,
        entry_message,
        types: checked_types,
        outputs: outputs.into_values(),
        processes: checked_processes,
    }))
}

fn check_process<'a>(
    context: &ModuleCheckContext<'a>,
    process: &'a Process,
    process_id: CheckedProcessId,
    types: &mut CheckedTypeInterner<'a>,
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
    let (authorities, authority_index) = collect_authorities(
        context.module,
        context.semantic_index,
        process,
        context.entry_process,
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
    let mut spawn_sites = SpawnSiteAllocator::default();
    let (supervisor_plans, supervisor_child_index) = check_supervisors(
        context.module,
        context.semantic_index,
        process,
        process_id,
        context.entry_process,
        &mut spawn_sites,
    )?;
    let (process_refs, transitions) = check_step(
        &process_context,
        &authority_index,
        &supervisor_child_index,
        &mut spawn_sites,
        &mut state_space,
        outputs,
        types,
    )?;
    let spawn_sites = spawn_sites.into_sites();
    validate_count(
        &format!("process {} spawn_site_count", process.name),
        spawn_sites.len(),
        0,
        MAX_SPAWN_SITES_PER_PROCESS,
    )?;
    validate_authority_usage(process, &authorities, &spawn_sites)?;
    let state_values = state_space.into_values()?;

    Ok(CheckedProcess::with_authority(
        CheckedProcessParts {
            debug_name: process.name.clone(),
            state_type: types.intern(&process.state_type)?,
            state_values,
            message_type: types.intern(&process.msg_type)?,
            message_cases: context.message_cases.cases_for(process_id)?.to_vec(),
            process_refs,
            mailbox_bound: process.mailbox_bound,
            init_state,
            transitions,
        },
        authorities,
        spawn_sites,
        supervisor_plans,
    ))
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
            "entry message {} must not require a payload",
            first_message.name
        )));
    }
    Ok(())
}

fn total_action_count(transitions: &[CheckedTransition]) -> Result<usize> {
    transitions.iter().try_fold(0usize, |total, transition| {
        total
            .checked_add(checked_action_count(transition.actions())?)
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
