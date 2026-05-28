use std::collections::BTreeSet;

use mantle_artifact::validate_message_label;

use crate::language::ast::{Effect, Identifier, Module};
use crate::language::diagnostic::{Error, Result};

use super::{
    CheckedAuthorityId, CheckedMessageId, CheckedMessageVariantId, CheckedNextState,
    CheckedOutputId, CheckedPayloadValue, CheckedProcessId, CheckedProcessRefId,
    CheckedSpawnSiteId, CheckedStateId, CheckedStateValue, CheckedSupervisorChildId,
    CheckedSupervisorId, CheckedTypeRef, CheckedValueTemplate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedStepResult {
    Continue,
    Stop,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedMessageCase {
    label: String,
    variant: CheckedMessageVariantId,
    payload_type: Option<CheckedTypeRef>,
}

impl CheckedMessageCase {
    pub(in crate::language) fn new(
        label: String,
        variant: CheckedMessageVariantId,
        payload_type: Option<CheckedTypeRef>,
    ) -> Result<Self> {
        validate_message_label(&label).map_err(|err| Error::new(err.to_string()))?;
        Ok(Self {
            label,
            variant,
            payload_type,
        })
    }

    pub(in crate::language) fn label(&self) -> &str {
        &self.label
    }

    pub(in crate::language) fn variant(&self) -> CheckedMessageVariantId {
        self.variant
    }

    pub(in crate::language) fn payload_type(&self) -> Option<&CheckedTypeRef> {
        self.payload_type.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcessRef {
    debug_name: Identifier,
    target: CheckedProcessId,
}

impl CheckedProcessRef {
    pub(in crate::language) fn new(debug_name: Identifier, target: CheckedProcessId) -> Self {
        Self { debug_name, target }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn target(&self) -> CheckedProcessId {
        self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::language) enum CheckedCapabilityDescriptor {
    Spawn { target: CheckedProcessId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedAuthority {
    debug_name: Identifier,
    descriptor: CheckedCapabilityDescriptor,
}

impl CheckedAuthority {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        descriptor: CheckedCapabilityDescriptor,
    ) -> Self {
        Self {
            debug_name,
            descriptor,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn descriptor(&self) -> CheckedCapabilityDescriptor {
        self.descriptor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedSpawnKind {
    DynamicLocal,
    LexicalSupervisorChild,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedSpawnSite {
    target: CheckedProcessId,
    authority: Option<CheckedAuthorityId>,
    supervisor: Option<CheckedSupervisorId>,
    child: Option<CheckedSupervisorChildId>,
    kind: CheckedSpawnKind,
}

impl CheckedSpawnSite {
    pub(in crate::language) fn dynamic_local(
        target: CheckedProcessId,
        authority: CheckedAuthorityId,
    ) -> Self {
        Self {
            target,
            authority: Some(authority),
            supervisor: None,
            child: None,
            kind: CheckedSpawnKind::DynamicLocal,
        }
    }

    pub(in crate::language) fn lexical_supervisor_child(
        target: CheckedProcessId,
        supervisor: CheckedSupervisorId,
        child: CheckedSupervisorChildId,
    ) -> Self {
        Self {
            target,
            authority: None,
            supervisor: Some(supervisor),
            child: Some(child),
            kind: CheckedSpawnKind::LexicalSupervisorChild,
        }
    }

    pub(in crate::language) fn target(&self) -> CheckedProcessId {
        self.target
    }

    pub(in crate::language) fn authority(&self) -> Option<CheckedAuthorityId> {
        self.authority
    }

    pub(in crate::language) fn supervisor(&self) -> Option<CheckedSupervisorId> {
        self.supervisor
    }

    pub(in crate::language) fn child(&self) -> Option<CheckedSupervisorChildId> {
        self.child
    }

    pub(in crate::language) fn kind(&self) -> CheckedSpawnKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedSupervisorStrategy {
    OneForOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) enum CheckedSupervisorChildMode {
    Permanent,
    Transient,
    Temporary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::language) struct CheckedSupervisorRestartIntensity {
    max_restarts: u32,
    within_ms: u64,
}

impl CheckedSupervisorRestartIntensity {
    pub(in crate::language) fn new(max_restarts: u32, within_ms: u64) -> Result<Self> {
        if max_restarts == 0 {
            return Err(Error::new(
                "supervisor restart intensity max_restarts must be greater than zero",
            ));
        }
        if within_ms == 0 {
            return Err(Error::new(
                "supervisor restart intensity within_ms must be greater than zero",
            ));
        }
        Ok(Self {
            max_restarts,
            within_ms,
        })
    }

    pub(in crate::language) fn max_restarts(&self) -> u32 {
        self.max_restarts
    }

    pub(in crate::language) fn within_ms(&self) -> u64 {
        self.within_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedSupervisorChild {
    debug_name: Identifier,
    target: CheckedProcessId,
    mode: CheckedSupervisorChildMode,
    spawn_site: CheckedSpawnSiteId,
}

impl CheckedSupervisorChild {
    pub(in crate::language) fn new(
        debug_name: Identifier,
        target: CheckedProcessId,
        mode: CheckedSupervisorChildMode,
        spawn_site: CheckedSpawnSiteId,
    ) -> Self {
        Self {
            debug_name,
            target,
            mode,
            spawn_site,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn target(&self) -> CheckedProcessId {
        self.target
    }

    pub(in crate::language) fn mode(&self) -> CheckedSupervisorChildMode {
        self.mode
    }

    pub(in crate::language) fn spawn_site(&self) -> CheckedSpawnSiteId {
        self.spawn_site
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedSupervisorPlan {
    strategy: CheckedSupervisorStrategy,
    intensity: CheckedSupervisorRestartIntensity,
    children: Vec<CheckedSupervisorChild>,
}

impl CheckedSupervisorPlan {
    pub(in crate::language) fn new(
        strategy: CheckedSupervisorStrategy,
        intensity: CheckedSupervisorRestartIntensity,
        children: Vec<CheckedSupervisorChild>,
    ) -> Result<Self> {
        if children.is_empty() {
            return Err(Error::new(
                "supervisor declarations must contain at least one child",
            ));
        }
        Ok(Self {
            strategy,
            intensity,
            children,
        })
    }

    pub(in crate::language) fn strategy(&self) -> CheckedSupervisorStrategy {
        self.strategy
    }

    pub(in crate::language) fn intensity(&self) -> CheckedSupervisorRestartIntensity {
        self.intensity
    }

    pub(in crate::language) fn children(&self) -> &[CheckedSupervisorChild] {
        &self.children
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedAction {
    Emit {
        output: CheckedOutputId,
    },
    Spawn {
        target: CheckedProcessId,
        process_ref: CheckedProcessRefId,
        spawn_site: CheckedSpawnSiteId,
    },
    SpawnOutcome {
        outcome: super::CheckedEffectOutcomeId,
        outcome_ty: CheckedTypeRef,
        target: CheckedProcessId,
        spawn_site: CheckedSpawnSiteId,
    },
    Send {
        target: CheckedSendTarget,
        message: CheckedMessageId,
        payload: Option<Box<CheckedValueTemplate>>,
    },
    SendOutcome {
        outcome: super::CheckedEffectOutcomeId,
        outcome_ty: CheckedTypeRef,
        target: CheckedSendTarget,
        message: CheckedMessageId,
        payload: Option<Box<CheckedValueTemplate>>,
    },
    IfElse {
        condition: CheckedValueTemplate,
        then_actions: Vec<CheckedAction>,
        else_actions: Vec<CheckedAction>,
    },
    ForEach {
        element: CheckedLoopElement,
        collection: CheckedValueTemplate,
        max_items: usize,
        body: Vec<CheckedAction>,
    },
}

impl CheckedAction {
    pub(in crate::language) fn collect_effects(&self, effects: &mut BTreeSet<Effect>) {
        match self {
            Self::Emit { .. } => {
                effects.insert(Effect::Emit);
            }
            Self::Spawn { .. } | Self::SpawnOutcome { .. } => {
                effects.insert(Effect::Spawn);
            }
            Self::Send { .. } | Self::SendOutcome { .. } => {
                effects.insert(Effect::Send);
            }
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                for action in then_actions {
                    action.collect_effects(effects);
                }
                for action in else_actions {
                    action.collect_effects(effects);
                }
            }
            Self::ForEach { body, .. } => {
                for action in body {
                    action.collect_effects(effects);
                }
            }
        }
    }

    pub(in crate::language) fn action_count(&self) -> Result<usize> {
        match self {
            Self::Emit { .. }
            | Self::Spawn { .. }
            | Self::SpawnOutcome { .. }
            | Self::Send { .. }
            | Self::SendOutcome { .. } => Ok(1),
            Self::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                let then_count = checked_action_count(then_actions)?;
                let else_count = checked_action_count(else_actions)?;
                then_count
                    .checked_add(else_count)
                    .and_then(|count| count.checked_add(1))
                    .ok_or_else(|| Error::new("checked action_count overflowed"))
            }
            Self::ForEach { body, .. } => checked_action_count(body)?
                .checked_add(1)
                .ok_or_else(|| Error::new("checked action_count overflowed")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedLoopElement {
    id: super::CheckedLoopElementId,
    ty: CheckedTypeRef,
}

impl CheckedLoopElement {
    pub(in crate::language) fn new(id: super::CheckedLoopElementId, ty: CheckedTypeRef) -> Self {
        Self { id, ty }
    }

    pub(in crate::language) fn id(&self) -> super::CheckedLoopElementId {
        self.id
    }

    pub(in crate::language) fn ty(&self) -> &CheckedTypeRef {
        &self.ty
    }
}

pub(in crate::language) fn checked_action_count(actions: &[CheckedAction]) -> Result<usize> {
    actions.iter().try_fold(0usize, |count, action| {
        count
            .checked_add(action.action_count()?)
            .ok_or_else(|| Error::new("checked action_count overflowed"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) enum CheckedSendTarget {
    ProcessRef(CheckedProcessRefId),
    SupervisorChild {
        supervisor: CheckedSupervisorId,
        child: CheckedSupervisorChildId,
        target: CheckedProcessId,
    },
    ReceivedPayload {
        ty: CheckedTypeRef,
        target: CheckedProcessId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedTransition {
    current_state: Option<CheckedStateId>,
    message: CheckedMessageId,
    payload_guard: Option<CheckedPayloadValue>,
    step_result: CheckedStepResult,
    next_state: CheckedNextState,
    effects: Vec<Effect>,
    actions: Vec<CheckedAction>,
}

impl CheckedTransition {
    pub(in crate::language) fn new(parts: CheckedTransitionParts) -> Self {
        Self {
            current_state: parts.current_state,
            message: parts.message,
            payload_guard: None,
            step_result: parts.step_result,
            next_state: parts.next_state,
            effects: parts.effects,
            actions: parts.actions,
        }
    }

    pub(in crate::language) fn with_payload_guard(mut self, guard: CheckedPayloadValue) -> Self {
        self.payload_guard = Some(guard);
        self
    }

    pub(in crate::language) fn current_state(&self) -> Option<CheckedStateId> {
        self.current_state
    }

    pub(in crate::language) fn message(&self) -> CheckedMessageId {
        self.message
    }

    pub(in crate::language) fn payload_guard(&self) -> Option<&CheckedPayloadValue> {
        self.payload_guard.as_ref()
    }

    pub(in crate::language) fn step_result(&self) -> CheckedStepResult {
        self.step_result
    }

    #[cfg(test)]
    pub(in crate::language) fn next_state(&self) -> CheckedNextState {
        self.next_state.clone()
    }

    pub(in crate::language) fn next_state_ref(&self) -> &CheckedNextState {
        &self.next_state
    }

    pub(in crate::language) fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub(in crate::language) fn actions(&self) -> &[CheckedAction] {
        &self.actions
    }
}

pub(in crate::language) struct CheckedTransitionParts {
    pub current_state: Option<CheckedStateId>,
    pub message: CheckedMessageId,
    pub step_result: CheckedStepResult,
    pub next_state: CheckedNextState,
    pub effects: Vec<Effect>,
    pub actions: Vec<CheckedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language) struct CheckedProcess {
    debug_name: Identifier,
    state_type: CheckedTypeRef,
    state_values: Vec<CheckedStateValue>,
    message_type: CheckedTypeRef,
    message_cases: Vec<CheckedMessageCase>,
    process_refs: Vec<CheckedProcessRef>,
    authorities: Vec<CheckedAuthority>,
    spawn_sites: Vec<CheckedSpawnSite>,
    supervisor_plans: Vec<CheckedSupervisorPlan>,
    mailbox_bound: usize,
    init_state: CheckedStateId,
    transitions: Vec<CheckedTransition>,
}

impl CheckedProcess {
    #[cfg(test)]
    pub(in crate::language) fn new(parts: CheckedProcessParts) -> Self {
        Self::with_authority(parts, Vec::new(), Vec::new(), Vec::new())
    }

    pub(in crate::language) fn with_authority(
        parts: CheckedProcessParts,
        authorities: Vec<CheckedAuthority>,
        spawn_sites: Vec<CheckedSpawnSite>,
        supervisor_plans: Vec<CheckedSupervisorPlan>,
    ) -> Self {
        Self {
            debug_name: parts.debug_name,
            state_type: parts.state_type,
            state_values: parts.state_values,
            message_type: parts.message_type,
            message_cases: parts.message_cases,
            process_refs: parts.process_refs,
            authorities,
            spawn_sites,
            supervisor_plans,
            mailbox_bound: parts.mailbox_bound,
            init_state: parts.init_state,
            transitions: parts.transitions,
        }
    }

    pub(in crate::language) fn debug_name(&self) -> &Identifier {
        &self.debug_name
    }

    pub(in crate::language) fn state_type(&self) -> &CheckedTypeRef {
        &self.state_type
    }

    pub(in crate::language) fn state_values(&self) -> &[CheckedStateValue] {
        &self.state_values
    }

    pub(in crate::language) fn message_type(&self) -> &CheckedTypeRef {
        &self.message_type
    }

    pub(in crate::language) fn message_cases(&self) -> &[CheckedMessageCase] {
        &self.message_cases
    }

    pub(in crate::language) fn process_refs(&self) -> &[CheckedProcessRef] {
        &self.process_refs
    }

    pub(in crate::language) fn authorities(&self) -> &[CheckedAuthority] {
        &self.authorities
    }

    pub(in crate::language) fn spawn_sites(&self) -> &[CheckedSpawnSite] {
        &self.spawn_sites
    }

    pub(in crate::language) fn supervisor_plans(&self) -> &[CheckedSupervisorPlan] {
        &self.supervisor_plans
    }

    pub(in crate::language) fn mailbox_bound(&self) -> usize {
        self.mailbox_bound
    }

    pub(in crate::language) fn init_state(&self) -> CheckedStateId {
        self.init_state
    }

    pub(in crate::language) fn transitions(&self) -> &[CheckedTransition] {
        &self.transitions
    }
}

pub(in crate::language) struct CheckedProcessParts {
    pub debug_name: Identifier,
    pub state_type: CheckedTypeRef,
    pub state_values: Vec<CheckedStateValue>,
    pub message_type: CheckedTypeRef,
    pub message_cases: Vec<CheckedMessageCase>,
    pub process_refs: Vec<CheckedProcessRef>,
    pub mailbox_bound: usize,
    pub init_state: CheckedStateId,
    pub transitions: Vec<CheckedTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    module: Module,
    entry_process: CheckedProcessId,
    entry_message: CheckedMessageId,
    types: Vec<CheckedTypeRef>,
    outputs: Vec<String>,
    processes: Vec<CheckedProcess>,
}

impl CheckedProgram {
    pub(in crate::language) fn new(parts: CheckedProgramParts) -> Self {
        Self {
            module: parts.module,
            entry_process: parts.entry_process,
            entry_message: parts.entry_message,
            types: parts.types,
            outputs: parts.outputs,
            processes: parts.processes,
        }
    }

    pub fn module_name(&self) -> &str {
        self.module.name.as_str()
    }

    pub fn entry_process_label(&self) -> Result<&str> {
        self.processes
            .get(self.entry_process.index())
            .map(|process| process.debug_name.as_str())
            .ok_or_else(|| Error::new("checked entry process is not defined"))
    }

    pub(in crate::language) fn module(&self) -> &Module {
        &self.module
    }

    pub(in crate::language) fn entry_process(&self) -> CheckedProcessId {
        self.entry_process
    }

    pub(in crate::language) fn entry_message(&self) -> CheckedMessageId {
        self.entry_message
    }

    pub(in crate::language) fn types(&self) -> &[CheckedTypeRef] {
        &self.types
    }

    pub(in crate::language) fn outputs(&self) -> &[String] {
        &self.outputs
    }

    pub(in crate::language) fn processes(&self) -> &[CheckedProcess] {
        &self.processes
    }
}

pub(in crate::language) struct CheckedProgramParts {
    pub module: Module,
    pub entry_process: CheckedProcessId,
    pub entry_message: CheckedMessageId,
    pub types: Vec<CheckedTypeRef>,
    pub outputs: Vec<String>,
    pub processes: Vec<CheckedProcess>,
}
