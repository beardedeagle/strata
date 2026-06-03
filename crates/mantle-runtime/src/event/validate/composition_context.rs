use mantle_artifact::Result;

use super::json::JsonLine;
use super::{optional_trace_u32, required_trace_u32, validate_trace_u32_value};
use crate::event::RuntimeTraceEventKind;

const OPTIONAL_COMPOSITION_TRACE_FIELDS: &[&str] =
    &["deployment_id", "composition_id", "component_instance_id"];
const SINGLETON_DEPLOYMENT_ID: u32 = 0;

pub(super) fn is_optional_composition_field(field: &str) -> bool {
    OPTIONAL_COMPOSITION_TRACE_FIELDS.contains(&field)
}

#[derive(Debug, Default)]
pub(super) struct RuntimeTraceCompositionTable {
    identity: Option<RuntimeTraceCompositionIdentity>,
    process_component_instances: Vec<Option<u32>>,
    component_instance_processes: Vec<Option<u32>>,
}

impl RuntimeTraceCompositionTable {
    pub(super) fn validate_context(
        &mut self,
        kind: RuntimeTraceEventKind,
        line: &JsonLine<'_>,
        identity: Option<RuntimeTraceCompositionIdentity>,
    ) -> Result<()> {
        if kind == RuntimeTraceEventKind::ArtifactLoaded {
            let process_count = usize::try_from(required_trace_u32(line, "process_count")?)
                .map_err(|_| line.error("runtime trace process_count does not fit into usize"))?;
            self.identity = identity;
            self.process_component_instances = vec![None; process_count];
            self.component_instance_processes = vec![None; process_count];
            return Ok(());
        }

        match (self.identity, identity) {
            (None, None) => {}
            (None, Some(_)) => {
                return Err(line.error(
                    "runtime trace composition context must be absent after unbound artifact_loaded",
                ));
            }
            (Some(_), None) => {
                return Err(line.error(
                    "runtime trace composition context must appear on every event once established",
                ));
            }
            (Some(expected), Some(actual)) if expected != actual => {
                return Err(
                    line.error("runtime trace composition context changed after artifact_loaded")
                );
            }
            (Some(_), Some(_)) => {}
        }

        if identity.is_some() && kind.contract().required_fields().contains(&"process_id") {
            self.validate_process_component_instance(line)?;
        }
        Ok(())
    }

    fn validate_process_component_instance(&mut self, line: &JsonLine<'_>) -> Result<()> {
        let process_id = required_trace_u32(line, "process_id")?;
        let process_index = usize::try_from(process_id)
            .map_err(|_| line.error("runtime trace process_id does not fit into usize"))?;
        let component_instance_id = optional_trace_u32(line, "component_instance_id")?
            .ok_or_else(|| {
                line.error(format!(
                    "runtime trace process_id {process_id} requires component_instance_id under bound composition"
                ))
            })?;
        let component_index = usize::try_from(component_instance_id).map_err(|_| {
            line.error("runtime trace component_instance_id does not fit into usize")
        })?;
        if process_index >= self.process_component_instances.len() {
            return Err(line.error(format!(
                "process_id {process_id} is outside runtime trace composition process table"
            )));
        }
        if component_index >= self.component_instance_processes.len() {
            return Err(line.error(format!(
                "component_instance_id {component_instance_id} is outside runtime trace composition component table"
            )));
        }

        match self.process_component_instances[process_index] {
            Some(expected) if expected != component_instance_id => Err(line.error(format!(
                "runtime trace component_instance_id changed for process_id {process_id}"
            ))),
            Some(_) => self.validate_component_process_slot(line, component_index, process_id),
            None => {
                self.process_component_instances[process_index] = Some(component_instance_id);
                self.validate_component_process_slot(line, component_index, process_id)
            }
        }
    }

    fn validate_component_process_slot(
        &mut self,
        line: &JsonLine<'_>,
        component_index: usize,
        process_id: u32,
    ) -> Result<()> {
        match self.component_instance_processes[component_index] {
            Some(expected) if expected != process_id => Err(line.error(format!(
                "runtime trace component_instance_id {component_index} is already correlated with process_id {expected}"
            ))),
            Some(_) => Ok(()),
            None => {
                self.component_instance_processes[component_index] = Some(process_id);
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeTraceCompositionIdentity {
    deployment_id: u32,
    composition_id: u32,
}

pub(super) fn validate_optional_composition_fields(
    kind: RuntimeTraceEventKind,
    line: &JsonLine<'_>,
) -> Result<Option<RuntimeTraceCompositionIdentity>> {
    for field in OPTIONAL_COMPOSITION_TRACE_FIELDS {
        line.require_unique_optional_field(field)?;
        if let Some(value) = line.optional_u64(field)? {
            validate_trace_u32_value(line, field, value)?;
        }
    }

    let has_deployment = line.value("deployment_id")?.is_some();
    let has_composition = line.value("composition_id")?.is_some();
    let has_component_instance = line.value("component_instance_id")?.is_some();
    if has_deployment != has_composition {
        return Err(line.error(
            "runtime trace composition context must include both deployment_id and composition_id",
        ));
    }
    if has_component_instance && !(has_deployment && has_composition) {
        return Err(line.error(
            "runtime trace component_instance_id requires deployment_id and composition_id",
        ));
    }
    if has_component_instance && !kind.contract().required_fields().contains(&"process_id") {
        return Err(
            line.error("runtime trace component_instance_id requires a process-scoped event")
        );
    }
    if has_deployment {
        let deployment_id = required_trace_u32(line, "deployment_id")?;
        if deployment_id != SINGLETON_DEPLOYMENT_ID {
            return Err(line.error(format!(
                "runtime trace deployment_id must be {SINGLETON_DEPLOYMENT_ID}"
            )));
        }
        Ok(Some(RuntimeTraceCompositionIdentity {
            deployment_id,
            composition_id: required_trace_u32(line, "composition_id")?,
        }))
    } else {
        Ok(None)
    }
}
