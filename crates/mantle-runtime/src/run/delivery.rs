use super::boundaries::BoundarySendContext;
use super::model::RuntimeMailboxState;
use super::*;
use crate::event::RuntimeStopReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeliveryPreflightFailure {
    Full,
    Stopped,
    Crashed,
    MailboxClosed,
}

impl DeliveryPreflightFailure {
    pub(super) const fn send_error_variant(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Stopped => "Stopped",
            Self::Crashed => "Crashed",
            Self::MailboxClosed => "MailboxClosed",
        }
    }
}

enum DeliveryPreflight {
    Accepted(usize),
    Failed(DeliveryPreflightFailure),
}

impl<'program, 'plan, 'host, H: RuntimeHost> RuntimeRun<'program, 'plan, 'host, H> {
    pub(super) fn send_message(
        &mut self,
        target: RuntimeProcessId,
        envelope: RuntimeMessageEnvelope,
        sender_pid: Option<RuntimeProcessId>,
    ) -> Result<()> {
        self.send_message_inner(target, envelope, sender_pid, None)
    }

    pub(super) fn send_message_with_boundary(
        &mut self,
        target: RuntimeProcessId,
        envelope: RuntimeMessageEnvelope,
        sender_pid: Option<RuntimeProcessId>,
        boundary: BoundarySendContext<'_>,
    ) -> Result<()> {
        self.send_message_inner(target, envelope, sender_pid, Some(boundary))
    }

    fn send_message_inner(
        &mut self,
        target: RuntimeProcessId,
        envelope: RuntimeMessageEnvelope,
        sender_pid: Option<RuntimeProcessId>,
        boundary: Option<BoundarySendContext<'_>>,
    ) -> Result<()> {
        let process_index = self.preflight_delivery_target(target)?;
        let process = &self.processes[process_index];
        let process_id = process.process_id;
        let process_label = self.program.process_label(process_id)?;
        envelope.validate_for_process(self.program, process_id)?;
        self.validate_envelope_process_ref(&envelope)?;
        let pid = process.pid;
        let queue_depth = process.mailbox.len() + 1;
        let message_label = self
            .program
            .message_label(process_id, envelope.message)?
            .to_string();
        let process_label = process_label.to_string();

        if let Some(boundary) = boundary {
            self.record_boundary_send_checked(
                boundary.step,
                boundary.port_id,
                process_id,
                envelope.message,
            )?;
        }
        self.record_event(RuntimeEvent::MessageAccepted {
            pid,
            process_id,
            process: process_label.clone(),
            message_id: envelope.message,
            message: message_label.clone(),
            payload: envelope.payload.clone(),
            queue_depth,
            sender_pid,
        })?;
        let delivered_message = envelope.display_label(&message_label);
        self.processes[process_index].mailbox.push_back(envelope);
        self.delivered_messages.push(MessageDelivery {
            pid,
            process: process_label,
            message: delivered_message,
        });
        Ok(())
    }

    pub(super) fn preflight_delivery_target(&self, target: RuntimeProcessId) -> Result<usize> {
        let process_index = self.process_index_for_pid(target)?;
        match self.preflight_delivery_target_index(process_index, 0)? {
            DeliveryPreflight::Accepted(process_index) => Ok(process_index),
            DeliveryPreflight::Failed(failure) => {
                let process = &self.processes[process_index];
                let process_label = self.program.process_label(process.process_id)?;
                Err(delivery_preflight_error(process_label, failure))
            }
        }
    }

    pub(super) fn preflight_delivery_target_outcome(
        &self,
        target: RuntimeProcessId,
    ) -> Result<std::result::Result<usize, DeliveryPreflightFailure>> {
        let process_index = self.process_index_for_pid(target)?;
        Ok(
            match self.preflight_delivery_target_index(process_index, 0)? {
                DeliveryPreflight::Accepted(process_index) => Ok(process_index),
                DeliveryPreflight::Failed(failure) => Err(failure),
            },
        )
    }

    pub(super) fn preflight_delivery_target_with_queued_messages(
        &self,
        target: RuntimeProcessId,
        queued_mailbox_messages: &[usize],
    ) -> Result<usize> {
        let process_index = self.process_index_for_pid(target)?;
        let queued_messages = queued_mailbox_messages
            .get(process_index)
            .copied()
            .ok_or_else(|| {
                Error::new("runtime loop preflight mailbox accounting is inconsistent")
            })?;
        match self.preflight_delivery_target_index(process_index, queued_messages)? {
            DeliveryPreflight::Accepted(process_index) => Ok(process_index),
            DeliveryPreflight::Failed(failure) => {
                let process = &self.processes[process_index];
                let process_label = self.program.process_label(process.process_id)?;
                Err(delivery_preflight_error(process_label, failure))
            }
        }
    }

    fn preflight_delivery_target_index(
        &self,
        process_index: usize,
        queued_messages: usize,
    ) -> Result<DeliveryPreflight> {
        let process = self.processes.get(process_index).ok_or_else(|| {
            Error::new(format!(
                "runtime process index {process_index} is not available for mailbox preflight"
            ))
        })?;
        match process.status {
            ProcessStatus::Running => {}
            ProcessStatus::Stopped => {
                return Ok(DeliveryPreflight::Failed(stopped_process_failure(
                    process.stop_reason,
                    process.mailbox_state,
                )));
            }
            ProcessStatus::Failed => {
                return Ok(DeliveryPreflight::Failed(DeliveryPreflightFailure::Crashed));
            }
        }
        if process.mailbox_state == RuntimeMailboxState::Closed {
            return Ok(DeliveryPreflight::Failed(
                DeliveryPreflightFailure::MailboxClosed,
            ));
        }
        let projected_depth = process
            .mailbox
            .len()
            .checked_add(queued_messages)
            .ok_or_else(|| Error::new("runtime mailbox preflight depth overflowed"))?;
        if projected_depth >= process.mailbox_bound {
            return Ok(DeliveryPreflight::Failed(DeliveryPreflightFailure::Full));
        }
        Ok(DeliveryPreflight::Accepted(process_index))
    }

    pub(super) fn process_index_for_pid(&self, pid: RuntimeProcessId) -> Result<usize> {
        let raw_index = pid
            .as_u64()
            .checked_sub(1)
            .ok_or_else(|| Error::new("runtime process id index underflowed"))?;
        let process_index = usize::try_from(raw_index).map_err(|_| {
            Error::new(format!(
                "runtime process {pid} cannot be indexed on this platform"
            ))
        })?;
        let process = self
            .processes
            .get(process_index)
            .ok_or_else(|| Error::new(format!("runtime process {pid} is not spawned")))?;
        if process.pid != pid {
            return Err(Error::new(format!(
                "runtime process index for pid {pid} is inconsistent"
            )));
        }
        Ok(process_index)
    }

    pub(super) fn drain_mailboxes(&mut self, max_dispatches: usize) -> Result<()> {
        let mut dispatches = 0usize;
        while let Some(process_index) = self.next_runnable_process() {
            if dispatches >= max_dispatches {
                return Err(Error::new(format!(
                    "runtime dispatch budget exceeded after {max_dispatches} process step(s)"
                )));
            }
            let dequeued = self.processes[process_index].dequeue(self.program)?;
            self.record_event(RuntimeEvent::MessageDequeued {
                pid: dequeued.pid,
                process_id: dequeued.process_id,
                process: self.program.process_label(dequeued.process_id)?.to_string(),
                message_id: dequeued.envelope.message,
                message: self
                    .program
                    .message_label(dequeued.process_id, dequeued.envelope.message)?
                    .to_string(),
                payload: dequeued.envelope.payload.clone(),
                queue_depth: dequeued.queue_depth,
            })?;
            self.step_process(process_index, dequeued.envelope)?;
            dispatches += 1;
        }
        Ok(())
    }

    fn next_runnable_process(&self) -> Option<usize> {
        self.processes.iter().position(|process| {
            process.status == ProcessStatus::Running && !process.mailbox.is_empty()
        })
    }
}

pub(super) fn stopped_process_failure(
    stop_reason: Option<RuntimeStopReason>,
    mailbox_state: RuntimeMailboxState,
) -> DeliveryPreflightFailure {
    match stop_reason {
        Some(RuntimeStopReason::Normal) => DeliveryPreflightFailure::Stopped,
        Some(RuntimeStopReason::SupervisorShutdown | RuntimeStopReason::SupervisorFailure) => {
            DeliveryPreflightFailure::MailboxClosed
        }
        None if mailbox_state == RuntimeMailboxState::Closed => {
            DeliveryPreflightFailure::MailboxClosed
        }
        None => DeliveryPreflightFailure::Stopped,
    }
}

fn delivery_preflight_error(process_label: &str, failure: DeliveryPreflightFailure) -> Error {
    match failure {
        DeliveryPreflightFailure::Full => Error::new(format!(
            "mailbox for process {} is full; message was not accepted",
            process_label
        )),
        DeliveryPreflightFailure::Stopped => Error::new(format!(
            "send to process {process_label} failed because it is stopped"
        )),
        DeliveryPreflightFailure::Crashed => Error::new(format!(
            "send to process {process_label} failed because it has failed"
        )),
        DeliveryPreflightFailure::MailboxClosed => Error::new(format!(
            "mailbox for process {} is closed; message was not accepted",
            process_label
        )),
    }
}
