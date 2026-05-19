use mantle_artifact::{Error, ProcessId, ProcessRefId, Result};

use super::RuntimeRun;
use super::model::{ActiveStep, RuntimeMessageEnvelope};
use crate::event::RuntimeProcessId;
use crate::host::RuntimeHost;
use crate::program::LoadedSendTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalProcessRefs {
    process_ref_count: usize,
    pids: Option<Vec<Option<RuntimeProcessId>>>,
}

impl LocalProcessRefs {
    pub(super) fn new(process_ref_count: usize) -> Self {
        Self {
            process_ref_count,
            pids: None,
        }
    }

    pub(super) fn empty() -> Self {
        Self::new(0)
    }

    pub(super) fn get(&self, process_ref: ProcessRefId) -> Option<RuntimeProcessId> {
        self.pids
            .as_ref()
            .and_then(|pids| pids.get(process_ref.index()).copied().flatten())
    }

    pub(super) fn is_bound(&self, process_ref: ProcessRefId) -> bool {
        self.get(process_ref).is_some()
    }

    pub(super) fn bind(
        &mut self,
        process_ref: ProcessRefId,
        pid: RuntimeProcessId,
    ) -> Result<bool> {
        let index = process_ref.index();
        if index >= self.process_ref_count {
            return Err(Error::new(format!(
                "process reference id {} is outside local process-ref table",
                process_ref.as_u32()
            )));
        }

        let pids = self
            .pids
            .get_or_insert_with(|| vec![None; self.process_ref_count]);
        let slot = &mut pids[index];
        if slot.is_some() {
            return Ok(false);
        }
        *slot = Some(pid);
        Ok(true)
    }
}

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
    pub(super) fn resolve_send_target(
        &self,
        local_process_refs: &LocalProcessRefs,
        step: &ActiveStep,
        target: &LoadedSendTarget,
    ) -> Result<RuntimeProcessId> {
        match target {
            LoadedSendTarget::ProcessRef(process_ref) => {
                self.resolve_process_ref(local_process_refs, step, *process_ref)
            }
            LoadedSendTarget::ReceivedPayload { ty, target_process } => {
                let payload = step.payload.as_ref().ok_or_else(|| {
                    Error::new("received process reference send target requires a payload")
                })?;
                if payload.ty != *ty {
                    return Err(Error::new(format!(
                        "received process reference send target has type id {}, expected {}",
                        payload.ty.as_u32(),
                        ty.as_u32()
                    )));
                }
                let process_ref = payload.process_ref.ok_or_else(|| {
                    Error::new("received payload is not a process reference value")
                })?;
                if process_ref.target_process != *target_process {
                    return Err(Error::new(format!(
                        "received process reference targets process id {}, expected {}",
                        process_ref.target_process.as_u32(),
                        target_process.as_u32()
                    )));
                }
                Ok(RuntimeProcessId::from_u64(process_ref.pid)?)
            }
        }
    }

    pub(super) fn process_ref_target(
        &self,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<ProcessId> {
        self.program
            .process(step.process_id)?
            .process_refs
            .get(process_ref.index())
            .map(|process_ref| process_ref.target)
            .ok_or_else(|| {
                Error::new(format!(
                    "process {} references undefined process reference id {}",
                    step.process_name,
                    process_ref.as_u32()
                ))
            })
    }

    pub(super) fn ensure_process_ref_unbound(
        &self,
        local_process_refs: &LocalProcessRefs,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<()> {
        self.process_ref_target(step, process_ref)?;
        if local_process_refs.is_bound(process_ref) {
            return Err(Error::new(format!(
                "process {} rebinds process reference id {}",
                step.process_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    pub(super) fn bind_process_ref(
        &self,
        local_process_refs: &mut LocalProcessRefs,
        step: &ActiveStep,
        process_ref: ProcessRefId,
        pid: RuntimeProcessId,
    ) -> Result<()> {
        self.process_ref_target(step, process_ref)?;
        if !local_process_refs.bind(process_ref, pid)? {
            return Err(Error::new(format!(
                "process {} rebinds process reference id {}",
                step.process_name,
                process_ref.as_u32()
            )));
        }
        Ok(())
    }

    fn resolve_process_ref(
        &self,
        local_process_refs: &LocalProcessRefs,
        step: &ActiveStep,
        process_ref: ProcessRefId,
    ) -> Result<RuntimeProcessId> {
        self.process_ref_target(step, process_ref)?;
        local_process_refs.get(process_ref).ok_or_else(|| {
            Error::new(format!(
                "process {} sends to unbound process reference id {}",
                step.process_name,
                process_ref.as_u32()
            ))
        })
    }

    pub(super) fn validate_envelope_process_ref(
        &self,
        envelope: &RuntimeMessageEnvelope,
    ) -> Result<()> {
        let Some(payload) = &envelope.payload else {
            return Ok(());
        };
        let expected_target = self
            .program
            .process_ref_target_for_type_id("payload type", payload.ty);
        let (expected_target, process_ref) = match (expected_target, payload.process_ref) {
            (Ok(expected_target), Some(process_ref)) => (expected_target, process_ref),
            (Ok(_), None) => {
                return Err(Error::new(format!(
                    "payload type id {} requires process reference runtime metadata",
                    payload.ty.as_u32()
                )));
            }
            (Err(_), Some(_)) => {
                return Err(Error::new(format!(
                    "payload type id {} must not carry process reference runtime metadata",
                    payload.ty.as_u32()
                )));
            }
            (Err(_), None) => {
                self.program
                    .validate_value_type("payload type", payload.ty)?;
                return Ok(());
            }
        };
        let process_index =
            self.process_index_for_pid(RuntimeProcessId::from_u64(process_ref.pid)?)?;
        let referenced = &self.processes[process_index];
        if referenced.process_id != process_ref.target_process {
            return Err(Error::new(format!(
                "payload process reference pid {} targets process id {}, but runtime pid has process id {}",
                process_ref.pid,
                process_ref.target_process.as_u32(),
                referenced.process_id.as_u32()
            )));
        }
        if process_ref.target_process != expected_target {
            return Err(Error::new(format!(
                "payload process reference metadata targets process id {}, expected {} for type id {}",
                process_ref.target_process.as_u32(),
                expected_target.as_u32(),
                payload.ty.as_u32()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_process_refs_allocate_only_on_bind() {
        let mut refs = LocalProcessRefs::new(2);

        assert!(refs.pids.is_none());
        assert_eq!(refs.get(ProcessRefId::new(0)), None);
        assert!(!refs.is_bound(ProcessRefId::new(1)));
        assert!(refs.pids.is_none());

        assert_eq!(
            refs.bind(ProcessRefId::new(1), RuntimeProcessId::FIRST),
            Ok(true)
        );
        assert!(refs.pids.is_some());
        assert_eq!(refs.get(ProcessRefId::new(0)), None);
        assert_eq!(
            refs.get(ProcessRefId::new(1)),
            Some(RuntimeProcessId::FIRST)
        );
    }

    #[test]
    fn local_process_refs_reject_invalid_bind_before_allocation() {
        let mut refs = LocalProcessRefs::new(1);

        let err = refs
            .bind(ProcessRefId::new(1), RuntimeProcessId::FIRST)
            .expect_err("out-of-range process ref should fail");

        assert!(refs.pids.is_none());
        assert!(
            err.to_string()
                .contains("process reference id 1 is outside local process-ref table")
        );
    }
}
