use mantle_artifact::{AuthorityId, Error, MessageId, PortId, ProcessId, Result};

use super::{RuntimeHost, RuntimeRun};
use crate::event::{RuntimeAuthorityResult, RuntimeEvent};
use crate::program::LoadedCapabilityDescriptor;
use crate::run::model::ActiveStep;

#[derive(Clone, Copy)]
pub(super) struct BoundarySendContext<'a> {
    pub(super) step: &'a ActiveStep,
    pub(super) port_id: PortId,
}

pub(super) fn boundary_send_authority_denied_error(step: &ActiveStep, port_id: PortId) -> Error {
    Error::new(format!(
        "process {} boundary send authority denied for port id {}",
        step.process_id.as_u32(),
        port_id.as_u32()
    ))
}

#[derive(Clone, Copy)]
struct BoundaryAuthorityCheck {
    authority_id: AuthorityId,
    decision_id: Option<u32>,
    admitted: bool,
}

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    pub(super) fn record_boundary_send_checked(
        &mut self,
        step: &ActiveStep,
        port_id: PortId,
        target_process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<bool> {
        let check = self.boundary_send_authority_check(step.process_id, port_id)?;
        self.record_boundary_send_authority_checked(
            step,
            port_id,
            target_process_id,
            message_id,
            check,
        )?;
        Ok(check.admitted)
    }

    pub(super) fn record_denied_boundary_send_checked(
        &mut self,
        step: &ActiveStep,
        port_id: PortId,
        target_process_id: ProcessId,
        message_id: MessageId,
    ) -> Result<bool> {
        let check = self.boundary_send_authority_check(step.process_id, port_id)?;
        if check.admitted {
            return Ok(false);
        }
        self.record_boundary_send_authority_checked(
            step,
            port_id,
            target_process_id,
            message_id,
            check,
        )?;
        Ok(true)
    }

    fn boundary_send_authority_check(
        &self,
        process_id: ProcessId,
        port_id: PortId,
    ) -> Result<BoundaryAuthorityCheck> {
        let authority_id = self.port_connect_authority_id(process_id, port_id)?;
        let decision = self.authority_decision(process_id, authority_id)?;
        Ok(BoundaryAuthorityCheck {
            authority_id,
            decision_id: decision.decision_id,
            admitted: decision.decision.admits(),
        })
    }

    fn record_boundary_send_authority_checked(
        &mut self,
        step: &ActiveStep,
        port_id: PortId,
        target_process_id: ProcessId,
        message_id: MessageId,
        check: BoundaryAuthorityCheck,
    ) -> Result<()> {
        let boundary = self.program.boundary_for_port(port_id)?;
        self.record_event(RuntimeEvent::BoundarySendChecked {
            pid: step.pid,
            process_id: step.process_id,
            process: step.process_name.clone(),
            port_id: boundary.port_id,
            port: boundary.port.debug_name.clone(),
            protocol_id: boundary.protocol_id,
            protocol: boundary.protocol.debug_name.clone(),
            authority_id: check.authority_id,
            authority_policy_decision_id: check.decision_id,
            target_process_id,
            target_process: self.program.process_label(target_process_id)?.to_string(),
            message_id,
            message: self
                .program
                .message_label(target_process_id, message_id)?
                .to_string(),
            boundary_result: if check.admitted {
                RuntimeAuthorityResult::Accepted
            } else {
                RuntimeAuthorityResult::Denied
            },
        })
    }

    fn port_connect_authority_id(
        &self,
        process_id: ProcessId,
        port_id: PortId,
    ) -> Result<AuthorityId> {
        self.program
            .process(process_id)?
            .authorities
            .iter()
            .position(|authority| {
                matches!(
                    authority.descriptor,
                    LoadedCapabilityDescriptor::PortConnect { port } if port == port_id
                )
            })
            .map(AuthorityId::from_index)
            .transpose()?
            .ok_or_else(|| {
                Error::new(format!(
                    "process_id {} has no port_connect authority for port_id {}",
                    process_id.as_u32(),
                    port_id.as_u32()
                ))
            })
    }
}
