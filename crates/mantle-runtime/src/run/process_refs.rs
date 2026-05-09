use mantle_artifact::{Error, Result};

use super::RuntimeRun;
use super::model::RuntimeMessageEnvelope;
use crate::event::RuntimeProcessId;
use crate::host::RuntimeHost;

impl<'program, 'host, H: RuntimeHost> RuntimeRun<'program, 'host, H> {
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
