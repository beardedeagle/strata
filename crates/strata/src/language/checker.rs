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
    ArtifactValue, MAX_ACTIONS_PER_PROCESS, MAX_ENUM_VARIANTS_PER_TYPE, MAX_IDENTIFIER_BYTES,
    MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TYPE_COUNT, MapProjectionMode,
};

use super::ast::{
    CollectionPatternBinding, ConstructorPayloadPattern, Determinism, Effect, Enum, EnumVariant,
    Function, FunctionBlock, FunctionBody, FunctionParam, Identifier, ListPattern, ListValue,
    MapPattern, MapPatternCompleteness, MapValue, MapValueEntry, Match, MatchArm, Module, Param,
    Pattern, Process, Record, RecordPatternField, RecordValue, RecordValueField, ReturnExpr,
    Statement, TypeRef, ValueExpr,
};
use super::checked::{
    CheckedAction, CheckedEnumVariantId, CheckedMessageCase, CheckedMessageId,
    CheckedMessageVariantId, CheckedNextState, CheckedPayloadValue, CheckedProcess,
    CheckedProcessId, CheckedProcessParts, CheckedProcessRef, CheckedProcessRefId, CheckedProgram,
    CheckedProgramParts, CheckedSendTarget, CheckedStateId, CheckedStepResult, CheckedTransition,
    CheckedTransitionParts, CheckedTypeId, CheckedTypeKind, CheckedTypeRef, CheckedValueTemplate,
};
use super::diagnostic::{Error, Result};
use super::{LIST_TYPE, MAP_TYPE, MAX_VALUE_NESTING, PROC_RESULT_TYPE, PROCESS_REF_TYPE};
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
use symbols::{CollectionType, SemanticIndex};

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
    module: &'a Module,
    semantic_index: &'a SemanticIndex,
    entries: Vec<(TypeRef, CheckedTypeRef)>,
}

impl<'a> CheckedTypeInterner<'a> {
    fn new(module: &'a Module, semantic_index: &'a SemanticIndex) -> Self {
        Self {
            module,
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
        let kind = match process_ref_target {
            Some(target) => CheckedTypeKind::ProcessRef { target },
            None => CheckedTypeKind::Value {
                enum_variants: self
                    .semantic_index
                    .enum_decl(self.module, ty)
                    .map(|enum_decl| {
                        enum_decl
                            .variants
                            .iter()
                            .map(|variant| variant.name.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        };
        let checked = CheckedTypeRef::new(id, checked_type_label(ty, process_ref_target)?, kind);
        self.entries.push((ty.clone(), checked.clone()));
        Ok(checked)
    }

    fn source_type(&self, checked_ty: &CheckedTypeRef) -> Result<&TypeRef> {
        self.entries
            .get(checked_ty.id().index())
            .filter(|(_, checked)| checked == checked_ty)
            .map(|(ty, _)| ty)
            .ok_or_else(|| {
                Error::new(format!(
                    "checked type id {} is not interned",
                    checked_ty.id().as_u32()
                ))
            })
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
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let mut label = format!(
                "{CHECKED_TYPE_LABEL_PREFIX}{}",
                checked_type_label_component(&TypeRef::Named(constructor.clone()))?
            );
            label.push('_');
            label.push_str(&args.len().to_string());
            label.push('_');
            label.push_str(&const_args.len().to_string());
            for arg in args {
                label.push('_');
                label.push_str(&checked_type_label_component(arg)?);
            }
            for value in const_args {
                label.push('_');
                label.push_str(&value.to_string());
            }
            if label.len() > MAX_IDENTIFIER_BYTES {
                return Err(Error::new(format!(
                    "checked type label for {ty} exceeds maximum identifier length of {MAX_IDENTIFIER_BYTES} bytes"
                )));
            }
            Ok(label)
        }
    }
}

fn checked_type_label_component(ty: &TypeRef) -> Result<String> {
    match ty {
        TypeRef::Named(name) => Ok(format!("{}_{}", name.as_str().len(), name)),
        TypeRef::Applied {
            constructor,
            args,
            const_args,
        } => {
            let mut label = format!(
                "{}_{}_{}_{}",
                constructor.as_str().len(),
                constructor,
                args.len(),
                const_args.len()
            );
            for arg in args {
                label.push('_');
                label.push_str(&checked_type_label_component(arg)?);
            }
            for value in const_args {
                label.push('_');
                label.push_str(&value.to_string());
            }
            Ok(label)
        }
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
    payload_bindings: Vec<StepPayloadBinding>,
    current_state: Option<CheckedStateId>,
    state_payload_bindings: Vec<StepStatePayloadBinding>,
    body: &'a FunctionBlock,
}

struct StepTransitionInput<'a> {
    current_state: Option<CheckedStateId>,
    variant: CheckedMessageVariantId,
    message: CheckedMessageId,
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
        keys: Vec<ArtifactValue>,
        projection: MapProjectionMode,
    },
    MapRest {
        excluded_keys: Vec<ArtifactValue>,
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
        keys: Vec<ArtifactValue>,
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

    fn map_rest(ty: TypeRef, excluded_keys: Vec<ArtifactValue>) -> Self {
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
    seen_bindings: &'seen mut BTreeSet<String>,
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

fn map_rest_type(
    key_type: &TypeRef,
    value_type: &TypeRef,
    capacity: usize,
    excluded_key_count: usize,
) -> Result<TypeRef> {
    let rest_capacity = capacity.checked_sub(excluded_key_count).ok_or_else(|| {
        Error::new(format!(
            "map rest binding excludes {excluded_key_count} keys from capacity {capacity}"
        ))
    })?;
    Ok(TypeRef::Applied {
        constructor: Identifier::new(MAP_TYPE)?,
        args: vec![key_type.clone(), value_type.clone()],
        const_args: vec![rest_capacity],
    })
}

fn list_rest_type(element_type: &TypeRef, capacity: usize, prefix_len: usize) -> Result<TypeRef> {
    let rest_capacity = capacity.checked_sub(prefix_len).ok_or_else(|| {
        Error::new(format!(
            "list rest binding removes {prefix_len} prefix elements from capacity {capacity}"
        ))
    })?;
    Ok(TypeRef::Applied {
        constructor: Identifier::new(LIST_TYPE)?,
        args: vec![element_type.clone()],
        const_args: vec![rest_capacity],
    })
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
    values: BTreeSet<ArtifactValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PayloadDomainKey {
    value: Option<ArtifactValue>,
    process_ref_target: Option<CheckedProcessId>,
    process_ref_label: Option<String>,
}

impl PayloadDomainKey {
    fn from_payload(payload: &CheckedPayloadValue) -> Self {
        if let Some(value) = payload.value() {
            return Self {
                value: Some(value.clone()),
                process_ref_target: None,
                process_ref_label: None,
            };
        }
        let process_ref = payload
            .process_ref_payload()
            .expect("non-value payloads must carry process reference metadata");
        Self {
            value: None,
            process_ref_target: Some(process_ref.target()),
            process_ref_label: Some(payload.label().to_string()),
        }
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
    EnumPattern,
    RecordPattern,
    ListPattern,
    MapPattern,
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
    validate_enum_variant_counts(&module)?;
    let mut types = CheckedTypeInterner::new(&module, &semantic_index);
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

fn validate_enum_variant_counts(module: &Module) -> Result<()> {
    for enum_decl in &module.enums {
        validate_count(
            &format!("enum {} variant_count", enum_decl.name),
            enum_decl.variants.len(),
            0,
            MAX_ENUM_VARIANTS_PER_TYPE,
        )?;
    }
    Ok(())
}

fn validate_process_declarations_before_message_cases(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<()> {
    let mut validation_types = CheckedTypeInterner::new(module, semantic_index);
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
        Pattern::Constructor { name, payload } => {
            let variant_index = context.semantic_index.enum_variant_index(
                context.module,
                context.enum_type,
                name,
            )?;
            let variant = &context.enum_decl.variants[variant_index];
            let bindings = check_pattern_payload_bindings(
                context.module,
                context.semantic_index,
                variant,
                payload.as_ref(),
                context.label,
                context.payload_context,
                context.binding_context,
            )?;
            let payload_guard = check_pattern_payload_guard(
                context.module,
                context.semantic_index,
                variant,
                payload.as_ref(),
            )?;
            Ok(TypedMatchPattern::Variant {
                variant: variant_index,
                bindings,
                payload_guard,
            })
        }
        Pattern::Record { name, .. } => Err(Error::new(format!(
            "{} {} pattern {name} destructures a record, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::List(_) => Err(Error::new(format!(
            "{} {} pattern List[...] destructures a list, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::Map(_) => Err(Error::new(format!(
            "{} {} pattern Map[...] destructures a map, but this match expects enum constructors",
            context.subject, context.label
        ))),
        Pattern::Wildcard => Ok(TypedMatchPattern::Wildcard),
    }
}

fn check_pattern_payload_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    payload: Option<&ConstructorPayloadPattern>,
    context: &str,
    payload_context: PatternPayloadContext,
    binding_context: PatternBindingContext<'_>,
) -> Result<Vec<PatternPayloadParam>> {
    match (&variant.payload_type, payload) {
        (None, None) => Ok(Vec::new()),
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
        (Some(_), None) => Ok(Vec::new()),
        (Some(payload_type), Some(ConstructorPayloadPattern::Binding(binding))) => {
            check_whole_payload_binding(
                semantic_index,
                binding_context,
                binding,
                payload_type,
                context,
            )
        }
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            check_destructured_payload_bindings(
                module,
                semantic_index,
                binding_context,
                context,
                payload_type,
                pattern,
            )
        }
    }
}

fn check_pattern_payload_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    variant: &EnumVariant,
    payload: Option<&ConstructorPayloadPattern>,
) -> Result<Option<PatternPayloadGuard>> {
    let Some(payload_type) = &variant.payload_type else {
        return Ok(None);
    };
    let Some(ConstructorPayloadPattern::Destructure(pattern)) = payload else {
        return Ok(None);
    };
    nested_pattern_payload_guard(module, semantic_index, payload_type, pattern)
}

fn nested_pattern_payload_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    expected_type: &TypeRef,
    pattern: &Pattern,
) -> Result<Option<PatternPayloadGuard>> {
    let Pattern::Constructor { name, payload } = pattern else {
        return Ok(None);
    };
    let enum_decl = semantic_index
        .enum_decl(module, expected_type)
        .map_err(|_| {
            Error::new(format!(
                "nested constructor pattern {name} cannot match value type {expected_type}"
            ))
        })?;
    let variant_index = semantic_index.enum_variant_index(module, expected_type, name)?;
    let variant = &enum_decl.variants[variant_index];
    let variant_id = CheckedEnumVariantId::from_index(variant_index)?;
    let payload_guard = match (&variant.payload_type, payload) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(Error::new(format!(
                "nested constructor pattern {name} does not carry a payload"
            )));
        }
        (Some(_), None) => {
            return Err(Error::new(format!(
                "nested constructor pattern {name} requires a payload pattern"
            )));
        }
        (Some(_), Some(ConstructorPayloadPattern::Binding(_))) => None,
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            nested_pattern_payload_guard(module, semantic_index, payload_type, pattern)?
                .map(Box::new)
        }
    };
    Ok(Some(PatternPayloadGuard {
        enum_ty: expected_type.clone(),
        variant: variant_id,
        payload: payload_guard,
    }))
}

fn check_whole_payload_binding(
    semantic_index: &SemanticIndex,
    binding_context: PatternBindingContext<'_>,
    binding: &Param,
    payload_type: &TypeRef,
    context: &str,
) -> Result<Vec<PatternPayloadParam>> {
    validate_pattern_binding_name(binding_context, semantic_index, &binding.name)?;
    if !semantic_index.same_type(&binding.ty, payload_type) {
        let subject = pattern_binding_subject(binding_context);
        return Err(Error::new(format!(
            "{subject} {context} payload {} has type {}, expected {}",
            binding.name, binding.ty, payload_type
        )));
    }
    Ok(vec![PatternPayloadParam {
        name: binding.name.clone(),
        ty: binding.ty.clone(),
        path: PayloadBindingPath::whole(),
    }])
}

fn check_destructured_payload_bindings(
    module: &Module,
    semantic_index: &SemanticIndex,
    binding_context: PatternBindingContext<'_>,
    context: &str,
    payload_type: &TypeRef,
    pattern: &Pattern,
) -> Result<Vec<PatternPayloadParam>> {
    let subject = pattern_binding_subject(binding_context);
    let mut seen_bindings = BTreeSet::new();
    let base_path = PayloadBindingPath::whole();
    let mut nested_scope = NestedPatternBindingScope {
        module,
        semantic_index,
        binding_context,
        context,
        seen_bindings: &mut seen_bindings,
    };
    match pattern {
        Pattern::Record { name, fields } => {
            let record = semantic_index.record_decl(module, payload_type).map_err(|_| {
                Error::new(format!(
                    "{subject} {context} record payload pattern {name} cannot match payload type {payload_type}"
                ))
            })?;
            if record.name != *name {
                return Err(Error::new(format!(
                    "{subject} {context} record payload pattern {name} cannot match record {}",
                    record.name
                )));
            }
            check_record_payload_pattern_bindings(&mut nested_scope, record, fields, &base_path)
        }
        Pattern::List(pattern) => {
            let Some(CollectionType::List { element, capacity }) =
                semantic_index.collection_type(payload_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {context} list payload pattern cannot match payload type {payload_type}"
                )));
            };
            if let Some(pattern_type) = &pattern.element_type
                && !semantic_index.same_type(pattern_type, element)
            {
                return Err(Error::new(format!(
                    "{subject} {context} list payload pattern has element type {pattern_type}, expected {element}"
                )));
            }
            validate_list_payload_pattern_capacity(
                &subject,
                context,
                payload_type,
                pattern,
                capacity,
            )?;
            check_list_payload_pattern_bindings(
                &mut nested_scope,
                element,
                capacity,
                pattern,
                &base_path,
            )
        }
        Pattern::Map(pattern) => {
            let Some(CollectionType::Map {
                key,
                value,
                capacity,
            }) = semantic_index.collection_type(payload_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern cannot match payload type {payload_type}"
                )));
            };
            if let Some(pattern_key_type) = &pattern.key_type
                && !semantic_index.same_type(pattern_key_type, key)
            {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern has key type {pattern_key_type}, expected {key}"
                )));
            }
            if let Some(pattern_value_type) = &pattern.value_type
                && !semantic_index.same_type(pattern_value_type, value)
            {
                return Err(Error::new(format!(
                    "{subject} {context} map payload pattern has value type {pattern_value_type}, expected {value}"
                )));
            }
            validate_map_payload_pattern_capacity(
                &subject,
                context,
                payload_type,
                pattern,
                capacity,
            )?;
            check_map_payload_pattern_bindings(
                &mut nested_scope,
                MapPatternType {
                    key,
                    value,
                    capacity,
                },
                pattern,
                &base_path,
            )
        }
        Pattern::Constructor { .. } => check_constructor_payload_pattern_bindings(
            &mut nested_scope,
            payload_type,
            pattern,
            &base_path,
            EmptyConstructorPattern::Allow,
        ),
        Pattern::Wildcard => Ok(Vec::new()),
    }
}

fn check_nested_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    expected_type: &TypeRef,
    pattern: &Pattern,
    base_path: &PayloadBindingPath,
    empty_constructor: EmptyConstructorPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let subject = scope.subject();
    match pattern {
        Pattern::Record { name, fields } => {
            let record = scope
                .semantic_index
                .record_decl(scope.module, expected_type)
                .map_err(|_| {
                    Error::new(format!(
                        "{subject} {} nested record pattern {name} cannot match value type {expected_type}",
                        scope.context
                    ))
                })?;
            if record.name != *name {
                return Err(Error::new(format!(
                    "{subject} {} nested record pattern {name} cannot match record {}",
                    scope.context, record.name
                )));
            }
            check_record_payload_pattern_bindings(scope, record, fields, base_path)
        }
        Pattern::List(pattern) => {
            let Some(CollectionType::List { element, capacity }) =
                scope.semantic_index.collection_type(expected_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {} nested list pattern cannot match value type {expected_type}",
                    scope.context
                )));
            };
            if let Some(pattern_type) = &pattern.element_type
                && !scope.semantic_index.same_type(pattern_type, element)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested list pattern has element type {pattern_type}, expected {element}",
                    scope.context
                )));
            }
            validate_list_payload_pattern_capacity(
                &subject,
                scope.context,
                expected_type,
                pattern,
                capacity,
            )?;
            check_list_payload_pattern_bindings(scope, element, capacity, pattern, base_path)
        }
        Pattern::Map(pattern) => {
            let Some(CollectionType::Map {
                key,
                value,
                capacity,
            }) = scope.semantic_index.collection_type(expected_type)?
            else {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern cannot match value type {expected_type}",
                    scope.context
                )));
            };
            if let Some(pattern_key_type) = &pattern.key_type
                && !scope.semantic_index.same_type(pattern_key_type, key)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern has key type {pattern_key_type}, expected {key}",
                    scope.context
                )));
            }
            if let Some(pattern_value_type) = &pattern.value_type
                && !scope.semantic_index.same_type(pattern_value_type, value)
            {
                return Err(Error::new(format!(
                    "{subject} {} nested map pattern has value type {pattern_value_type}, expected {value}",
                    scope.context
                )));
            }
            validate_map_payload_pattern_capacity(
                &subject,
                scope.context,
                expected_type,
                pattern,
                capacity,
            )?;
            check_map_payload_pattern_bindings(
                scope,
                MapPatternType {
                    key,
                    value,
                    capacity,
                },
                pattern,
                base_path,
            )
        }
        Pattern::Constructor { .. } => check_constructor_payload_pattern_bindings(
            scope,
            expected_type,
            pattern,
            base_path,
            empty_constructor,
        ),
        Pattern::Wildcard => Ok(Vec::new()),
    }
}

fn check_constructor_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    enum_type: &TypeRef,
    pattern: &Pattern,
    base_path: &PayloadBindingPath,
    empty_constructor: EmptyConstructorPattern,
) -> Result<Vec<PatternPayloadParam>> {
    let Pattern::Constructor { name, payload } = pattern else {
        return Err(Error::new("expected constructor payload pattern"));
    };
    let subject = scope.subject();
    let enum_decl = scope
        .semantic_index
        .enum_decl(scope.module, enum_type)
        .map_err(|_| {
            Error::new(format!(
                "{subject} {} nested constructor pattern {name} cannot match value type {enum_type}",
                scope.context
            ))
        })?;
    let variant_index = scope
        .semantic_index
        .enum_variant_index(scope.module, enum_type, name)?;
    let variant = &enum_decl.variants[variant_index];
    let variant_id = CheckedEnumVariantId::from_index(variant_index)?;
    match (&variant.payload_type, payload) {
        (None, None) => match empty_constructor {
            EmptyConstructorPattern::Allow => Ok(Vec::new()),
            EmptyConstructorPattern::Reject => Err(Error::new(format!(
                "{subject} {} nested constructor pattern {name} must bind at least one nested value",
                scope.context
            ))),
        },
        (None, Some(_)) => Err(Error::new(format!(
            "{subject} {} nested constructor pattern {name} does not carry a payload",
            scope.context
        ))),
        (Some(_), None) => Err(Error::new(format!(
            "{subject} {} nested constructor pattern {name} requires a payload pattern",
            scope.context
        ))),
        (Some(payload_type), Some(ConstructorPayloadPattern::Binding(binding))) => {
            validate_pattern_binding_name(
                scope.binding_context,
                scope.semantic_index,
                &binding.name,
            )?;
            if !scope.semantic_index.same_type(&binding.ty, payload_type) {
                return Err(Error::new(format!(
                    "{subject} {} nested constructor payload {} has type {}, expected {}",
                    scope.context, binding.name, binding.ty, payload_type
                )));
            }
            if scope
                .semantic_index
                .process_ref_target_type(payload_type)?
                .is_some()
            {
                return Err(Error::new(format!(
                    "{subject} {} nested constructor payload {} cannot bind process reference payload type {}; process references must be direct message payload bindings",
                    scope.context, binding.name, payload_type
                )));
            }
            add_pattern_payload_binding(
                &subject,
                scope.seen_bindings,
                PatternPayloadParam {
                    name: binding.name.clone(),
                    ty: binding.ty.clone(),
                    path: base_path.then(PayloadProjectionSegment::enum_payload(
                        enum_type.clone(),
                        payload_type.clone(),
                        variant_id,
                    )),
                },
            )
        }
        (Some(payload_type), Some(ConstructorPayloadPattern::Destructure(pattern))) => {
            let nested_path = base_path.then(PayloadProjectionSegment::enum_payload(
                enum_type.clone(),
                payload_type.clone(),
                variant_id,
            ));
            check_nested_pattern_bindings(
                scope,
                payload_type,
                pattern,
                &nested_path,
                EmptyConstructorPattern::Allow,
            )
        }
    }
}

fn add_pattern_payload_binding(
    subject: &str,
    seen_bindings: &mut BTreeSet<String>,
    binding: PatternPayloadParam,
) -> Result<Vec<PatternPayloadParam>> {
    if !seen_bindings.insert(binding.name.to_string()) {
        return Err(Error::new(format!(
            "{subject} payload binding {} is declared more than once",
            binding.name
        )));
    }
    Ok(vec![binding])
}

fn check_record_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    record: &Record,
    fields: &[RecordPatternField],
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    if fields.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} record payload pattern {} must bind at least one field",
            scope.context, record.name
        )));
    }

    let mut seen_fields = BTreeSet::new();
    let mut bindings = Vec::with_capacity(fields.len());
    for field in fields {
        let subject = scope.subject();
        if !seen_fields.insert(field.field.as_str()) {
            return Err(Error::new(format!(
                "{subject} {} record payload pattern {} binds field {} more than once",
                scope.context, record.name, field.field
            )));
        }
        let Some(field_decl) = record
            .fields
            .iter()
            .find(|candidate| candidate.name == field.field)
        else {
            return Err(Error::new(format!(
                "{subject} {} record payload pattern {} has no field {}",
                scope.context, record.name, field.field
            )));
        };
        if !scope.seen_bindings.insert(field.binding.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} payload binding {} is declared more than once",
                scope.context, field.binding
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, &field.binding)?;
        bindings.push(PatternPayloadParam {
            name: field.binding.clone(),
            ty: field_decl.ty.clone(),
            path: base_path.then(PayloadProjectionSegment::record_field(
                field_decl.ty.clone(),
                field.field.clone(),
            )),
        });
    }
    Ok(bindings)
}

fn check_list_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    element_type: &TypeRef,
    capacity: usize,
    pattern: &ListPattern,
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    let mut bindings = Vec::new();
    for (index, binding) in pattern.elements.iter().enumerate() {
        let element_path =
            base_path.then(list_element_binding_segment(element_type, index, pattern));
        match binding {
            CollectionPatternBinding::Binding(name) => {
                let subject = scope.subject();
                if !scope.seen_bindings.insert(name.to_string()) {
                    return Err(Error::new(format!(
                        "{subject} {} list payload pattern binding {name} is declared more than once",
                        scope.context
                    )));
                }
                validate_pattern_binding_name(scope.binding_context, scope.semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: element_type.clone(),
                    path: element_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let nested_bindings = check_nested_pattern_bindings(
                    scope,
                    element_type,
                    pattern,
                    &element_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    let subject = scope.subject();
                    return Err(Error::new(format!(
                        "{subject} {} list payload nested pattern must bind at least one value in this source slice",
                        scope.context
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        let subject = scope.subject();
        if !scope.seen_bindings.insert(rest.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} list payload pattern binding {rest} is declared more than once",
                scope.context
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, rest)?;
        let rest_ty = list_rest_type(element_type, capacity, pattern.elements.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: base_path.then(PayloadProjectionSegment::list_rest(
                rest_ty,
                pattern.elements.len(),
            )),
        });
    }
    if bindings.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} list payload pattern must bind at least one value in this source slice",
            scope.context
        )));
    }
    Ok(bindings)
}

fn validate_list_payload_pattern_capacity(
    subject: &str,
    context: &str,
    payload_type: &TypeRef,
    pattern: &ListPattern,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} {context} list payload pattern has capacity {pattern_capacity}, expected {capacity}"
        )));
    }
    if pattern.elements.len() > capacity {
        return Err(Error::new(format!(
            "{subject} {context} list payload pattern length {} exceeds capacity {capacity} for {payload_type}",
            pattern.elements.len()
        )));
    }
    if pattern.rest.is_some() && pattern.elements.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} list rest payload pattern must declare at least one prefix element"
        )));
    }
    Ok(())
}

fn check_map_payload_pattern_bindings(
    scope: &mut NestedPatternBindingScope<'_, '_>,
    map_type: MapPatternType<'_>,
    pattern: &MapPattern,
    base_path: &PayloadBindingPath,
) -> Result<Vec<PatternPayloadParam>> {
    let mut seen_keys = BTreeSet::new();
    let mut entry_keys = Vec::with_capacity(pattern.entries.len());
    for entry in &pattern.entries {
        let key = canonical_source_value_with_bindings(
            scope.module,
            scope.semantic_index,
            map_type.key,
            &entry.key,
            &[],
        )?;
        if !seen_keys.insert(key.clone()) {
            return Err(Error::new(format!(
                "map pattern duplicates key {}",
                key.label()
            )));
        }
        entry_keys.push(key);
    }
    let keys = seen_keys.into_iter().collect::<Vec<_>>();
    let mut bindings = Vec::new();
    for (entry, key) in pattern.entries.iter().zip(entry_keys) {
        let value_path = base_path.then(PayloadProjectionSegment::map_value(
            map_type.value.clone(),
            key,
            keys.clone(),
            map_pattern_projection(pattern),
        ));
        match &entry.binding {
            CollectionPatternBinding::Binding(name) => {
                let subject = scope.subject();
                if !scope.seen_bindings.insert(name.to_string()) {
                    return Err(Error::new(format!(
                        "{subject} {} map payload pattern binding {name} is declared more than once",
                        scope.context
                    )));
                }
                validate_pattern_binding_name(scope.binding_context, scope.semantic_index, name)?;
                bindings.push(PatternPayloadParam {
                    name: name.clone(),
                    ty: map_type.value.clone(),
                    path: value_path,
                });
            }
            CollectionPatternBinding::Pattern(pattern) => {
                let nested_bindings = check_nested_pattern_bindings(
                    scope,
                    map_type.value,
                    pattern,
                    &value_path,
                    EmptyConstructorPattern::Reject,
                )?;
                if nested_bindings.is_empty() {
                    let subject = scope.subject();
                    return Err(Error::new(format!(
                        "{subject} {} map payload nested pattern must bind at least one value in this source slice",
                        scope.context
                    )));
                }
                bindings.extend(nested_bindings);
            }
            CollectionPatternBinding::Wildcard => {}
        }
    }
    if let Some(rest) = &pattern.rest {
        let subject = scope.subject();
        if !scope.seen_bindings.insert(rest.to_string()) {
            return Err(Error::new(format!(
                "{subject} {} map payload pattern binding {rest} is declared more than once",
                scope.context
            )));
        }
        validate_pattern_binding_name(scope.binding_context, scope.semantic_index, rest)?;
        let rest_ty = map_rest_type(map_type.key, map_type.value, map_type.capacity, keys.len())?;
        bindings.push(PatternPayloadParam {
            name: rest.clone(),
            ty: rest_ty.clone(),
            path: base_path.then(PayloadProjectionSegment::map_rest(rest_ty, keys)),
        });
    }
    if bindings.is_empty() {
        let subject = scope.subject();
        return Err(Error::new(format!(
            "{subject} {} map payload pattern must bind at least one value in this source slice",
            scope.context
        )));
    }
    Ok(bindings)
}

fn validate_map_payload_pattern_capacity(
    subject: &str,
    context: &str,
    payload_type: &TypeRef,
    pattern: &MapPattern,
    capacity: usize,
) -> Result<()> {
    if let Some(pattern_capacity) = pattern.capacity
        && pattern_capacity != capacity
    {
        return Err(Error::new(format!(
            "{subject} {context} map payload pattern has capacity {pattern_capacity}, expected {capacity}"
        )));
    }
    if pattern.entries.len() > capacity {
        return Err(Error::new(format!(
            "{subject} {context} map payload pattern entry count {} exceeds capacity {capacity} for {payload_type}",
            pattern.entries.len()
        )));
    }
    if pattern.rest.is_some() && pattern.completeness != MapPatternCompleteness::Subset {
        return Err(Error::new(format!(
            "{subject} {context} map rest binding requires a subset map payload pattern"
        )));
    }
    if pattern.rest.is_some() && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} map rest payload pattern must declare at least one key"
        )));
    }
    if pattern.completeness == MapPatternCompleteness::Subset && pattern.entries.is_empty() {
        return Err(Error::new(format!(
            "{subject} {context} subset map payload pattern must declare at least one key"
        )));
    }
    Ok(())
}

fn map_pattern_projection(pattern: &MapPattern) -> MapProjectionMode {
    match pattern.completeness {
        MapPatternCompleteness::Exact => MapProjectionMode::Exact,
        MapPatternCompleteness::Subset => MapProjectionMode::Subset,
    }
}

fn list_element_binding_segment(
    element_type: &TypeRef,
    index: usize,
    pattern: &ListPattern,
) -> PayloadProjectionSegment {
    if pattern.rest.is_some() {
        PayloadProjectionSegment::list_prefix_index(
            element_type.clone(),
            index,
            pattern.elements.len(),
        )
    } else {
        PayloadProjectionSegment::list_index(element_type.clone(), index, pattern.elements.len())
    }
}

fn payload_binding_value(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload_value: &ArtifactValue,
    binding: &PatternPayloadParam,
) -> Result<Option<ArtifactValue>> {
    let mut value = payload_value.clone();
    for segment in binding.path.segments() {
        value = match &segment.kind {
            PayloadProjectionSegmentKind::EnumPayload { enum_ty, variant } => {
                let ArtifactValue::EnumVariant {
                    variant: actual,
                    payload,
                } = value
                else {
                    return Ok(None);
                };
                let enum_decl = semantic_index.enum_decl(module, enum_ty)?;
                let Some(expected) = enum_decl
                    .variants
                    .get(variant.index())
                    .map(|variant| variant.name.as_str())
                else {
                    return Ok(None);
                };
                if actual != expected {
                    return Ok(None);
                }
                *payload
            }
            PayloadProjectionSegmentKind::RecordField { field } => {
                let Ok(projected) = value.project_record_field(field.as_str()) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListIndex { index, len } => {
                let Ok(projected) = value.project_list_element(*index, *len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListPrefixIndex { index, prefix_len } => {
                let Ok(projected) = value.project_list_prefix_element(*index, *prefix_len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::ListRest { prefix_len } => {
                let Ok(projected) = value.project_list_rest(*prefix_len) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::MapValue {
                key,
                keys,
                projection,
            } => {
                let Ok(projected) = value.project_map_value(key, keys, *projection) else {
                    return Ok(None);
                };
                projected
            }
            PayloadProjectionSegmentKind::MapRest { excluded_keys } => {
                let Ok(projected) = value.project_map_rest(excluded_keys) else {
                    return Ok(None);
                };
                projected
            }
        };
    }
    Ok(Some(value))
}

fn payload_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: &CheckedPayloadValue,
    guard: &PatternPayloadGuard,
) -> Result<bool> {
    let Some(value) = payload.value() else {
        return Ok(false);
    };
    artifact_value_matches_guard(module, semantic_index, value, guard)
}

fn artifact_value_matches_guard(
    module: &Module,
    semantic_index: &SemanticIndex,
    value: &ArtifactValue,
    guard: &PatternPayloadGuard,
) -> Result<bool> {
    let enum_decl = semantic_index.enum_decl(module, &guard.enum_ty)?;
    let Some(variant) = enum_decl.variants.get(guard.variant.index()) else {
        return Ok(false);
    };
    match (&variant.payload_type, &guard.payload, value) {
        (None, None, ArtifactValue::Atom(actual)) => Ok(actual == variant.name.as_str()),
        (None, None, _) => Ok(false),
        (None, Some(_), _) => Err(Error::new(format!(
            "fieldless enum variant {} has a nested payload guard",
            variant.name
        ))),
        (
            Some(_),
            nested_guard,
            ArtifactValue::EnumVariant {
                variant: actual,
                payload,
            },
        ) if actual == variant.name.as_str() => match nested_guard {
            Some(nested_guard) => {
                artifact_value_matches_guard(module, semantic_index, payload, nested_guard)
            }
            None => Ok(true),
        },
        (Some(_), _, _) => Ok(false),
    }
}

fn checked_payload_binding(
    module: &Module,
    semantic_index: &SemanticIndex,
    payload: &CheckedPayloadValue,
    binding: &PatternPayloadParam,
) -> Result<Option<(String, Option<ArtifactValue>)>> {
    let Some(payload_value) = payload.value() else {
        return Ok(binding
            .path
            .is_whole()
            .then(|| (payload.label().to_string(), None)));
    };
    Ok(
        payload_binding_value(module, semantic_index, payload_value, binding)?
            .map(|value| (value.label(), Some(value))),
    )
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
