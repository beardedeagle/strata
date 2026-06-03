use mantle_artifact::{
    ArtifactComposition, ComponentInstanceId, Error, MantleArtifact, ProcessId, Result,
    read_text_artifact,
};
use std::path::Path;

mod json;

use json::{JsonArray, JsonObject};

use crate::event::{RuntimeEvent, RuntimeEventCompositionContext};

pub(crate) const MAX_RUNTIME_COMPOSITION_BINDING_BYTES: usize = 1024 * 1024;

const BINDING_SCHEMA_ID: &str = "mantle.runtime_composition_binding";
const BINDING_SCHEMA_VERSION_MAJOR: u32 = 1;
const BINDING_SCHEMA_VERSION_MINOR: u32 = 0;
const BINDING_KIND: &str = "runtime_composition_binding";
const BINDING_ADMISSION_RESULT: &str = "admitted";
// Current bindings describe one explicit runtime correlation namespace. Reject
// any other value rather than implying a deployment-ID allocator exists.
const DEPLOYMENT_ID: u32 = 0;
const SOURCE_FINGERPRINT_ALGORITHM: &str = "fnv1a64-diagnostic";
const COMPOSITION_SCHEMA_SUFFIX: &str = ".checked_component_composition";
const COMPOSITION_SCHEMA_VERSION_MAJOR: u32 = 1;
const COMPOSITION_SCHEMA_VERSION_MINOR: u32 = 0;

const TOP_LEVEL_FIELDS: &[&str] = &[
    "schema_id",
    "schema_version_major",
    "schema_version_minor",
    "artifact_kind",
    "deployment_id",
    "source_language",
    "source_module",
    "source_fingerprint",
    "source_fingerprint_algorithm",
    "mantle_artifact_format",
    "mantle_artifact_schema_version",
    "mantle_artifact_module",
    "mantle_artifact_source_hash_fnv1a64",
    "composition_schema_id",
    "composition_schema_version_major",
    "composition_schema_version_minor",
    "composition_id",
    "component_instances",
    "port_bindings",
    "admission_result",
    "extensions",
];
const COMPONENT_INSTANCE_FIELDS: &[&str] = &["component_instance_id", "component_id", "process_id"];
const PORT_BINDING_FIELDS: &[&str] = &[
    "port_binding_id",
    "importer_instance_id",
    "imported_port_id",
    "exporter_instance_id",
    "exported_port_id",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeCompositionBinding {
    deployment_id: u32,
    composition_id: u32,
    process_component_instances: Vec<Option<ComponentInstanceId>>,
}

impl RuntimeCompositionBinding {
    pub(crate) fn read_path(path: &Path, artifact: &MantleArtifact) -> Result<Self> {
        let text = read_text_artifact(path, MAX_RUNTIME_COMPOSITION_BINDING_BYTES)?;
        Self::decode_and_validate(&text, artifact)
    }

    #[cfg(test)]
    pub(crate) fn decode_for_test(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        Self::decode_and_validate(text, artifact)
    }

    pub(crate) fn component_instance_for_process(
        &self,
        process_id: ProcessId,
    ) -> Option<ComponentInstanceId> {
        self.process_component_instances
            .get(process_id.index())
            .and_then(|value| *value)
    }

    pub(crate) fn trace_context_for_event(
        &self,
        event: &RuntimeEvent,
    ) -> Result<RuntimeEventCompositionContext> {
        let component_instance_id = match event.primary_process_id() {
            Some(process_id) => Some(self.component_instance_for_process(process_id).ok_or_else(
                || {
                    Error::new(format!(
                        "runtime composition binding has no component instance for process_id {}",
                        process_id.as_u32()
                    ))
                },
            )?),
            None => None,
        };
        Ok(RuntimeEventCompositionContext {
            deployment_id: self.deployment_id,
            composition_id: self.composition_id,
            component_instance_id,
        })
    }

    fn decode_and_validate(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        if text.len() > MAX_RUNTIME_COMPOSITION_BINDING_BYTES {
            return Err(Error::new(format!(
                "runtime composition binding exceeds maximum size of {MAX_RUNTIME_COMPOSITION_BINDING_BYTES} bytes"
            )));
        }
        let object = JsonObject::new(text, "runtime composition binding")?;
        object.require_exact_fields(TOP_LEVEL_FIELDS)?;
        object.required_string_eq("schema_id", BINDING_SCHEMA_ID)?;
        object.required_u32_eq("schema_version_major", BINDING_SCHEMA_VERSION_MAJOR)?;
        object.required_u32_eq("schema_version_minor", BINDING_SCHEMA_VERSION_MINOR)?;
        object.required_string_eq("artifact_kind", BINDING_KIND)?;
        object.required_string_eq("admission_result", BINDING_ADMISSION_RESULT)?;
        object.required_empty_object("extensions")?;

        object.required_string_eq("source_language", artifact.source_language.as_ref())?;
        object.required_string_eq("source_module", &artifact.module)?;
        object.required_string_eq("source_fingerprint", &artifact.source_hash_fnv1a64)?;
        object.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
        object.required_string_eq("mantle_artifact_format", artifact.format.as_ref())?;
        object.required_string_eq(
            "mantle_artifact_schema_version",
            artifact.schema_version.as_ref(),
        )?;
        object.required_string_eq("mantle_artifact_module", &artifact.module)?;
        object.required_string_eq(
            "mantle_artifact_source_hash_fnv1a64",
            &artifact.source_hash_fnv1a64,
        )?;
        object.required_composition_schema_id(
            "composition_schema_id",
            artifact.source_language.as_ref(),
        )?;
        object.required_u32_eq(
            "composition_schema_version_major",
            COMPOSITION_SCHEMA_VERSION_MAJOR,
        )?;
        object.required_u32_eq(
            "composition_schema_version_minor",
            COMPOSITION_SCHEMA_VERSION_MINOR,
        )?;

        object.required_u32_eq("deployment_id", DEPLOYMENT_ID)?;
        let composition_id = object.required_u32("composition_id")?;
        let composition = artifact
            .compositions
            .get(
                usize::try_from(composition_id)
                    .map_err(|_| Error::new("composition_id does not fit into usize"))?,
            )
            .ok_or_else(|| {
                Error::new(format!(
                    "runtime composition binding references missing composition id {composition_id}"
                ))
            })?;
        let process_component_instances = validate_component_instances(
            &object.required_array("component_instances")?,
            artifact,
            composition,
        )?;
        validate_port_bindings(&object.required_array("port_bindings")?, composition)?;
        Ok(Self {
            deployment_id: DEPLOYMENT_ID,
            composition_id,
            process_component_instances,
        })
    }
}

pub fn validate_runtime_composition_binding_text(
    text: &str,
    artifact: &MantleArtifact,
) -> Result<()> {
    RuntimeCompositionBinding::decode_and_validate(text, artifact).map(|_| ())
}

fn validate_component_instances(
    instances: &JsonArray<'_>,
    artifact: &MantleArtifact,
    composition: &ArtifactComposition,
) -> Result<Vec<Option<ComponentInstanceId>>> {
    let process_count = artifact.processes.len();
    let mut by_process = vec![None; process_count];
    let mut instance_count = 0usize;
    instances.for_each_object(|index, object| {
        object.require_exact_fields(COMPONENT_INSTANCE_FIELDS)?;
        let component_instance_id = object.required_u32("component_instance_id")?;
        if usize::try_from(component_instance_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "component_instance_id {component_instance_id} at array index {index} is not canonical"
            )));
        }
        let runtime_instance = composition
            .component_instances
            .get(index)
            .ok_or_else(|| {
                Error::new(format!(
                    "component_instance_id {component_instance_id} is out of bounds"
                ))
            })?;
        let component_id = object.required_u32("component_id")?;
        if runtime_instance.component.as_u32() != component_id {
            return Err(Error::new(format!(
                "component_instance_id {component_instance_id} component_id does not match runtime artifact"
            )));
        }
        let component = artifact
            .components
            .get(runtime_instance.component.index())
            .ok_or_else(|| Error::new(format!("component_id {component_id} is out of bounds")))?;
        let export_port = artifact
            .ports
            .get(component.export_port.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "component_instance_id {component_instance_id} export port is out of bounds"
                ))
            })?;
        let process_id = object.required_u32("process_id")?;
        if export_port.target_process.as_u32() != process_id {
            return Err(Error::new(format!(
                "component_instance_id {component_instance_id} process_id does not match runtime artifact"
            )));
        }
        let process_index = usize::try_from(process_id)
            .map_err(|_| Error::new(format!("process_id {process_id} is out of bounds")))?;
        if process_index >= process_count {
            return Err(Error::new(format!("process_id {process_id} is out of bounds")));
        }
        if by_process[process_index].is_some() {
            return Err(Error::new(format!("process_id {process_id} is duplicated")));
        }
        by_process[process_index] = Some(ComponentInstanceId::new(component_instance_id));
        instance_count = instance_count
            .checked_add(1)
            .ok_or_else(|| Error::new("component instance count overflowed"))?;
        Ok(())
    })?;
    if instance_count != composition.component_instances.len() {
        return Err(Error::new(
            "runtime composition binding component instance count does not match runtime artifact",
        ));
    }
    if let Some((process_id, _)) = by_process
        .iter()
        .enumerate()
        .find(|(_, component_instance)| component_instance.is_none())
    {
        return Err(Error::new(format!(
            "runtime composition binding must correlate every runtime process; process_id {process_id} is unbound"
        )));
    }
    Ok(by_process)
}

fn validate_port_bindings(
    bindings: &JsonArray<'_>,
    composition: &ArtifactComposition,
) -> Result<()> {
    let mut count = 0usize;
    bindings.for_each_object(|index, object| {
        object.require_exact_fields(PORT_BINDING_FIELDS)?;
        let port_binding_id = object.required_u32("port_binding_id")?;
        if usize::try_from(port_binding_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "port_binding_id {port_binding_id} at array index {index} is not canonical"
            )));
        }
        let runtime_binding = composition.port_bindings.get(index).ok_or_else(|| {
            Error::new(format!(
                "port_binding_id {port_binding_id} is out of bounds"
            ))
        })?;
        if runtime_binding.importer.as_u32() != object.required_u32("importer_instance_id")?
            || runtime_binding.imported_port.as_u32() != object.required_u32("imported_port_id")?
            || runtime_binding.exporter.as_u32() != object.required_u32("exporter_instance_id")?
            || runtime_binding.exported_port.as_u32() != object.required_u32("exported_port_id")?
        {
            return Err(Error::new(format!(
                "port_binding_id {port_binding_id} does not match runtime artifact"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("port binding count overflowed"))?;
        Ok(())
    })?;
    if count != composition.port_bindings.len() {
        return Err(Error::new(
            "runtime composition binding port binding count does not match runtime artifact",
        ));
    }
    Ok(())
}
