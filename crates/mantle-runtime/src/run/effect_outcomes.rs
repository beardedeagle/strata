use mantle_artifact::{
    ArtifactProcessRefPayload, ArtifactValue, ArtifactValueShape, EffectOutcomeId, Error,
    MessageId, PortId, ProcessId, Result, SpawnSiteId, TypeId,
};

use super::RuntimeRun;
use super::boundaries::{BoundarySendContext, boundary_send_authority_denied_error};
use super::delivery::DeliveryPreflightFailure;
use super::model::{ActiveStep, RuntimeMessageEnvelope};
use super::process_refs::{LocalProcessRefs, SendOutcomeTarget};
use super::templates::{RuntimeTemplateContext, evaluate_runtime_template};
use crate::event::{
    RuntimeEffectOutcomeAction, RuntimeEffectOutcomeResult, RuntimeEvent, RuntimeProcessId,
};
use crate::executable::{
    ExecutableActionPlan, ExecutableSendTarget, ExecutableSpawnSite, ExecutableTemplateProgram,
    ExecutableValueTemplateRef,
};
use crate::host::RuntimeHost;
#[cfg(test)]
use crate::program::LoadedAction;
use crate::program::RuntimePayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeEffectOutcome {
    pub(super) id: EffectOutcomeId,
    pub(super) payload: RuntimePayload,
}

struct SendOutcomeExecution<'a, 'program> {
    local_process_refs: &'a LocalProcessRefs,
    step: &'a ActiveStep,
    outcome_ty: TypeId,
    target: &'a ExecutableSendTarget,
    port: Option<PortId>,
    message: MessageId,
    payload: Option<ExecutableValueTemplateRef>,
    executable_templates: &'a ExecutableTemplateProgram<'program>,
    effect_outcomes: &'a [RuntimeEffectOutcome],
}

struct RuntimeEffectOutcomeExecution {
    payload: RuntimePayload,
    trace: RuntimeEffectOutcomeTrace,
}

struct RuntimeEffectOutcomeTrace {
    action: RuntimeEffectOutcomeAction,
    target_process_id: ProcessId,
    spawn_site_id: Option<SpawnSiteId>,
    message_id: Option<MessageId>,
    port_id: Option<PortId>,
    result: RuntimeEffectOutcomeResult,
}

impl RuntimeEffectOutcomeTrace {
    const fn spawn(
        target_process_id: ProcessId,
        spawn_site_id: SpawnSiteId,
        result: RuntimeEffectOutcomeResult,
    ) -> Self {
        Self {
            action: RuntimeEffectOutcomeAction::Spawn,
            target_process_id,
            spawn_site_id: Some(spawn_site_id),
            message_id: None,
            port_id: None,
            result,
        }
    }

    const fn send(
        target_process_id: ProcessId,
        message_id: MessageId,
        port_id: Option<PortId>,
        result: RuntimeEffectOutcomeResult,
    ) -> Self {
        Self {
            action: RuntimeEffectOutcomeAction::Send,
            target_process_id,
            spawn_site_id: None,
            message_id: Some(message_id),
            port_id,
            result,
        }
    }
}

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    #[cfg(test)]
    pub(super) fn execute_prestate_action(
        &mut self,
        local_process_refs: &mut LocalProcessRefs,
        step: &ActiveStep,
        action: &LoadedAction,
        effect_outcomes: &mut Vec<RuntimeEffectOutcome>,
    ) -> Result<bool> {
        let process = self.program.process(step.process_id)?;
        let spawned_refs = local_process_refs.binding_flags();
        let plan = ExecutableActionPlan::from_loaded_for_test_with_spawned_refs(
            self.program,
            process,
            action,
            &spawned_refs,
        )?;
        self.execute_prestate_plan_action(
            local_process_refs,
            step,
            plan.action(),
            plan.templates(),
            effect_outcomes,
        )
    }

    pub(super) fn execute_prestate_plan_action<'template>(
        &mut self,
        local_process_refs: &mut LocalProcessRefs,
        step: &ActiveStep,
        action: &ExecutableActionPlan<'template>,
        executable_templates: &ExecutableTemplateProgram<'template>,
        effect_outcomes: &mut Vec<RuntimeEffectOutcome>,
    ) -> Result<bool> {
        match action {
            ExecutableActionPlan::Spawn {
                target,
                process_ref,
                spawn,
            } => {
                if process_ref.target_process != *target {
                    return Err(Error::new(format!(
                        "process {} executable process reference id {} targets process id {}, expected {}",
                        step.process_name,
                        process_ref.id.as_u32(),
                        process_ref.target_process.as_u32(),
                        target.as_u32()
                    )));
                }
                if !self.record_spawn_authority(step, *target, *spawn)? {
                    return Err(Error::new(format!(
                        "process {} spawn authority denied for process id {}",
                        step.process_name,
                        target.as_u32()
                    )));
                }
                self.ensure_local_spawn_backend_available(&step.process_name, *target)?;
                self.ensure_process_ref_unbound(local_process_refs, step, process_ref.id)?;
                let pid = self.spawn_process(*target, Some(step.pid))?;
                self.bind_process_ref(local_process_refs, step, process_ref.id, pid)?;
                Ok(true)
            }
            ExecutableActionPlan::SpawnOutcome {
                outcome,
                outcome_ty,
                target,
                spawn,
            } => {
                let execution = self.execute_spawn_outcome(step, *outcome_ty, *target, *spawn)?;
                bind_effect_outcome(effect_outcomes, *outcome, execution.payload)?;
                self.record_effect_outcome_bound(step, *outcome, execution.trace)?;
                Ok(true)
            }
            ExecutableActionPlan::SendOutcome {
                outcome,
                outcome_ty,
                target,
                port,
                message,
                payload,
            } => {
                let execution = self.execute_send_outcome(SendOutcomeExecution {
                    local_process_refs,
                    step,
                    outcome_ty: *outcome_ty,
                    target,
                    port: *port,
                    message: *message,
                    payload: *payload,
                    executable_templates,
                    effect_outcomes,
                })?;
                bind_effect_outcome(effect_outcomes, *outcome, execution.payload)?;
                self.record_effect_outcome_bound(step, *outcome, execution.trace)?;
                Ok(true)
            }
            ExecutableActionPlan::IfElse { .. } | ExecutableActionPlan::ForEach { .. } => Ok(false),
            ExecutableActionPlan::Emit { .. } | ExecutableActionPlan::Send { .. } => Ok(false),
        }
    }

    fn execute_spawn_outcome(
        &mut self,
        step: &ActiveStep,
        outcome_ty: TypeId,
        target: ProcessId,
        spawn: ExecutableSpawnSite,
    ) -> Result<RuntimeEffectOutcomeExecution> {
        self.program.process(target)?;
        if !self.record_spawn_authority(step, target, spawn)? {
            return self
                .spawn_error_outcome(outcome_ty, "Denied")
                .map(|payload| RuntimeEffectOutcomeExecution {
                    payload,
                    trace: RuntimeEffectOutcomeTrace::spawn(
                        target,
                        spawn.id,
                        RuntimeEffectOutcomeResult::Denied,
                    ),
                });
        }
        if !self.is_local_spawn_backend_available() {
            return self
                .spawn_error_outcome(outcome_ty, "BackendUnavailable")
                .map(|payload| RuntimeEffectOutcomeExecution {
                    payload,
                    trace: RuntimeEffectOutcomeTrace::spawn(
                        target,
                        spawn.id,
                        RuntimeEffectOutcomeResult::BackendUnavailable,
                    ),
                });
        }
        if !self.spawn_capacity_available(target)? {
            return self
                .spawn_error_outcome(outcome_ty, "Exhausted")
                .map(|payload| RuntimeEffectOutcomeExecution {
                    payload,
                    trace: RuntimeEffectOutcomeTrace::spawn(
                        target,
                        spawn.id,
                        RuntimeEffectOutcomeResult::Exhausted,
                    ),
                });
        }
        let pid = self.spawn_process(target, Some(step.pid))?;
        self.ok_process_ref_outcome(outcome_ty, target, pid)
            .map(|payload| RuntimeEffectOutcomeExecution {
                payload,
                trace: RuntimeEffectOutcomeTrace::spawn(
                    target,
                    spawn.id,
                    RuntimeEffectOutcomeResult::Ok,
                ),
            })
    }

    fn execute_send_outcome<'template>(
        &mut self,
        request: SendOutcomeExecution<'_, 'template>,
    ) -> Result<RuntimeEffectOutcomeExecution> {
        let target = self.resolve_send_outcome_target(
            request.local_process_refs,
            request.step,
            request.target,
        )?;
        let target_process_id = match target {
            SendOutcomeTarget::Active(pid) => {
                let target_process_index = self.process_index_for_pid(pid)?;
                self.processes[target_process_index].process_id
            }
            SendOutcomeTarget::InactiveSupervisorChild { target_process, .. } => target_process,
        };
        let expected_payload_type = self
            .program
            .message_payload_type(target_process_id, request.message)?;
        if let Some(port) = request.port {
            self.program.validate_boundary_send(
                request.step.process_name.as_str(),
                port,
                target_process_id,
                request.message,
            )?;
            let denied = self.record_denied_boundary_send_checked(
                request.step,
                port,
                target_process_id,
                request.message,
            )?;
            if denied {
                return Err(boundary_send_authority_denied_error(request.step, port));
            }
        }
        let prepared_payload = match (expected_payload_type, request.payload) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(Error::new(format!(
                    "process {} send outcome payload targets message id {}, which does not accept one",
                    request.step.process_name,
                    request.message.as_u32()
                )));
            }
            (Some(_), None) => {
                return Err(Error::new(format!(
                    "process {} send outcome message id {} requires a payload",
                    request.step.process_name,
                    request.message.as_u32()
                )));
            }
            (Some(payload_type), Some(payload)) => {
                let payload = evaluate_runtime_template(
                    RuntimeTemplateContext {
                        program: self.program,
                        templates: request.executable_templates,
                        received_payload: request.step.payload.as_ref(),
                        step: request.step,
                        process_refs: request.local_process_refs,
                        loop_elements: &[],
                        effect_outcomes: request.effect_outcomes,
                    },
                    payload,
                )?;
                self.program.validate_runtime_payload_matches_type(
                    "send outcome payload",
                    payload_type,
                    &payload,
                )?;
                Some(payload)
            }
        };
        match target {
            SendOutcomeTarget::Active(pid) => match self.preflight_delivery_target_outcome(pid)? {
                Ok(_) => {
                    let envelope = RuntimeMessageEnvelope::new(request.message, prepared_payload);
                    match request.port {
                        Some(port) => self.send_message_with_boundary(
                            pid,
                            envelope,
                            Some(request.step.pid),
                            BoundarySendContext {
                                step: request.step,
                                port_id: port,
                            },
                        )?,
                        None => self.send_message(pid, envelope, Some(request.step.pid))?,
                    }
                    self.ok_unit_outcome(request.outcome_ty).map(|payload| {
                        RuntimeEffectOutcomeExecution {
                            payload,
                            trace: RuntimeEffectOutcomeTrace::send(
                                target_process_id,
                                request.message,
                                request.port,
                                RuntimeEffectOutcomeResult::Ok,
                            ),
                        }
                    })
                }
                Err(failure) => self.send_failure_outcome(
                    request.outcome_ty,
                    target_process_id,
                    request.message,
                    request.port,
                    prepared_payload.as_ref(),
                    failure,
                ),
            },
            SendOutcomeTarget::InactiveSupervisorChild { failure, .. } => self
                .send_failure_outcome(
                    request.outcome_ty,
                    target_process_id,
                    request.message,
                    request.port,
                    prepared_payload.as_ref(),
                    failure,
                ),
        }
    }

    fn send_failure_outcome(
        &self,
        outcome_ty: TypeId,
        target_process_id: ProcessId,
        message: MessageId,
        port: Option<PortId>,
        payload: Option<&RuntimePayload>,
        failure: DeliveryPreflightFailure,
    ) -> Result<RuntimeEffectOutcomeExecution> {
        let original_message = self.runtime_message_payload(target_process_id, message, payload)?;
        self.send_error_outcome(outcome_ty, failure.send_error_variant(), original_message)
            .map(|payload| RuntimeEffectOutcomeExecution {
                payload,
                trace: RuntimeEffectOutcomeTrace::send(
                    target_process_id,
                    message,
                    port,
                    effect_outcome_result_for_delivery_failure(failure),
                ),
            })
    }

    fn record_effect_outcome_bound(
        &mut self,
        step: &ActiveStep,
        outcome: EffectOutcomeId,
        trace: RuntimeEffectOutcomeTrace,
    ) -> Result<()> {
        self.record_event(RuntimeEvent::EffectOutcomeBound {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            outcome_id: outcome,
            action: trace.action,
            target_process_id: trace.target_process_id,
            spawn_site_id: trace.spawn_site_id,
            message_id: trace.message_id,
            port_id: trace.port_id,
            outcome_result: trace.result,
        })
    }

    fn ok_unit_outcome(&self, outcome_ty: TypeId) -> Result<RuntimePayload> {
        let ok_ty = self.result_payload_type(outcome_ty, "Ok")?;
        let unit = self.unit_payload(ok_ty)?;
        self.result_variant_payload(outcome_ty, "Ok", unit)
    }

    fn ok_process_ref_outcome(
        &self,
        outcome_ty: TypeId,
        target: ProcessId,
        pid: RuntimeProcessId,
    ) -> Result<RuntimePayload> {
        let ok_ty = self.result_payload_type(outcome_ty, "Ok")?;
        self.program.validate_process_ref_type_id_target(
            "spawn outcome success type",
            ok_ty,
            target,
        )?;
        RuntimePayload::value_with_embedded_process_ref(
            outcome_ty,
            ArtifactValue::EnumVariant {
                variant: "Ok".to_string(),
                payload: Box::new(ArtifactValue::process_ref(ok_ty, pid.as_u64())),
            },
            target,
            pid,
        )
    }

    fn send_error_outcome(
        &self,
        outcome_ty: TypeId,
        error_variant: &str,
        original_message: RuntimePayload,
    ) -> Result<RuntimePayload> {
        let error_ty = self.result_payload_type(outcome_ty, "Err")?;
        let expected_message_ty = self.enum_payload_type(error_ty, error_variant)?;
        if expected_message_ty != original_message.ty {
            return Err(Error::new(format!(
                "send outcome error variant {error_variant} payload type id {}, expected original message type id {}",
                expected_message_ty.as_u32(),
                original_message.ty.as_u32()
            )));
        }
        let error_payload = self.enum_variant_payload_preserving_process_ref(
            error_ty,
            error_variant,
            original_message,
            "send outcome error",
        )?;
        self.result_variant_payload(outcome_ty, "Err", error_payload)
    }

    fn enum_variant_payload_preserving_process_ref(
        &self,
        enum_ty: TypeId,
        variant: &str,
        payload: RuntimePayload,
        field: &str,
    ) -> Result<RuntimePayload> {
        let value = ArtifactValue::EnumVariant {
            variant: variant.to_string(),
            payload: Box::new(payload.value),
        };
        match payload.process_ref {
            Some(process_ref) => {
                runtime_payload_value_with_process_ref(enum_ty, value, process_ref)
            }
            None => self.program.runtime_payload_value(field, enum_ty, value),
        }
    }

    fn spawn_error_outcome(
        &self,
        outcome_ty: TypeId,
        error_variant: &str,
    ) -> Result<RuntimePayload> {
        let error_ty = self.result_payload_type(outcome_ty, "Err")?;
        let unit_ty = self.enum_payload_type(error_ty, error_variant)?;
        let unit = self.unit_payload(unit_ty)?;
        let error_payload = self.program.runtime_payload_value(
            "spawn outcome error",
            error_ty,
            ArtifactValue::EnumVariant {
                variant: error_variant.to_string(),
                payload: Box::new(unit.value),
            },
        )?;
        self.result_variant_payload(outcome_ty, "Err", error_payload)
    }

    fn result_variant_payload(
        &self,
        outcome_ty: TypeId,
        variant: &str,
        payload: RuntimePayload,
    ) -> Result<RuntimePayload> {
        let expected = self.result_payload_type(outcome_ty, variant)?;
        if payload.ty != expected {
            return Err(Error::new(format!(
                "effect outcome result variant {variant} has payload type id {}, expected {}",
                payload.ty.as_u32(),
                expected.as_u32()
            )));
        }
        self.enum_variant_payload_preserving_process_ref(
            outcome_ty,
            variant,
            payload,
            "effect outcome result",
        )
    }

    fn runtime_message_payload(
        &self,
        target_process_id: ProcessId,
        message: MessageId,
        payload: Option<&RuntimePayload>,
    ) -> Result<RuntimePayload> {
        let process = self.program.process(target_process_id)?;
        let message_label = self.program.message_label(target_process_id, message)?;
        let value = match payload {
            Some(payload) => ArtifactValue::EnumVariant {
                variant: message_label.to_string(),
                payload: Box::new(payload.value.clone()),
            },
            None => ArtifactValue::Atom(message_label.to_string()),
        };
        match payload.and_then(|payload| payload.process_ref) {
            Some(process_ref) => {
                runtime_payload_value_with_process_ref(process.message_type, value, process_ref)
            }
            None => self.program.runtime_payload_value(
                "send outcome original message",
                process.message_type,
                value,
            ),
        }
    }

    fn unit_payload(&self, ty: TypeId) -> Result<RuntimePayload> {
        let type_entry = self.program.type_entry(ty)?;
        if type_entry.label != "Unit" {
            return Err(Error::new(format!(
                "effect outcome unit payload type id {} must be Unit",
                ty.as_u32()
            )));
        }
        self.program.runtime_payload_value(
            "effect outcome unit payload",
            ty,
            ArtifactValue::Atom("Unit".to_string()),
        )
    }

    fn result_payload_type(&self, result_ty: TypeId, variant: &str) -> Result<TypeId> {
        self.enum_payload_type(result_ty, variant)
    }

    fn enum_payload_type(&self, enum_ty: TypeId, variant: &str) -> Result<TypeId> {
        let type_entry = self.program.type_entry(enum_ty)?;
        let ArtifactValueShape::Enum { variants } = type_entry.value_shape()? else {
            return Err(Error::new(format!(
                "effect outcome type id {} must be an enum value type",
                enum_ty.as_u32()
            )));
        };
        let entry = variants
            .iter()
            .find(|entry| entry.label == variant)
            .ok_or_else(|| {
                Error::new(format!(
                    "effect outcome type id {} is missing variant {variant}",
                    enum_ty.as_u32()
                ))
            })?;
        entry.payload_type.ok_or_else(|| {
            Error::new(format!(
                "effect outcome type id {} variant {variant} must carry a payload",
                enum_ty.as_u32()
            ))
        })
    }
}

const fn effect_outcome_result_for_delivery_failure(
    failure: super::delivery::DeliveryPreflightFailure,
) -> RuntimeEffectOutcomeResult {
    match failure {
        super::delivery::DeliveryPreflightFailure::Full => RuntimeEffectOutcomeResult::Full,
        super::delivery::DeliveryPreflightFailure::Stopped => RuntimeEffectOutcomeResult::Stopped,
        super::delivery::DeliveryPreflightFailure::Crashed => RuntimeEffectOutcomeResult::Crashed,
        super::delivery::DeliveryPreflightFailure::MailboxClosed => {
            RuntimeEffectOutcomeResult::MailboxClosed
        }
    }
}

fn runtime_payload_value_with_process_ref(
    ty: TypeId,
    value: ArtifactValue,
    process_ref: ArtifactProcessRefPayload,
) -> Result<RuntimePayload> {
    RuntimePayload::value_with_embedded_process_ref(
        ty,
        value,
        process_ref.target_process,
        RuntimeProcessId::from_u64(process_ref.pid)?,
    )
}

fn bind_effect_outcome(
    effect_outcomes: &mut Vec<RuntimeEffectOutcome>,
    id: EffectOutcomeId,
    payload: RuntimePayload,
) -> Result<()> {
    if effect_outcomes.iter().any(|binding| binding.id == id) {
        return Err(Error::new(format!(
            "effect outcome id {} is bound more than once",
            id.as_u32()
        )));
    }
    effect_outcomes.push(RuntimeEffectOutcome { id, payload });
    Ok(())
}
