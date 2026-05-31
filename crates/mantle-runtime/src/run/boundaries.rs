use mantle_artifact::{MessageId, PortId, ProcessId, Result};

use super::{RuntimeHost, RuntimeRun};
use crate::event::{RuntimeAuthorityResult, RuntimeEvent};
use crate::run::model::ActiveStep;

#[derive(Clone, Copy)]
pub(super) struct BoundarySendContext<'a> {
    pub(super) step: &'a ActiveStep,
    pub(super) port_id: PortId,
}

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    pub(super) fn record_boundary_send_checked(
        &mut self,
        step: &ActiveStep,
        port_id: PortId,
        target_process_id: ProcessId,
        message_id: MessageId,
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
            target_process_id,
            target_process: self.program.process_label(target_process_id)?.to_string(),
            message_id,
            message: self
                .program
                .message_label(target_process_id, message_id)?
                .to_string(),
            boundary_result: RuntimeAuthorityResult::Accepted,
        })
    }
}
