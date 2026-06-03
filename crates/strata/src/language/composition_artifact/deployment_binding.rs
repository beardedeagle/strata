use std::fmt::Write as _;

use mantle_artifact::{MantleArtifact, ProcessId};

use super::super::checked_render::push_json_field;
use super::super::diagnostic::{Error, Result};
use super::codec::JsonObject;
use super::{
    COMPONENT_COMPOSITION_SCHEMA_ID, COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR,
    COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR, SOURCE_FINGERPRINT_ALGORITHM,
    admit_component_composition_artifact,
};

pub const RUNTIME_COMPOSITION_BINDING_SCHEMA_ID: &str = "mantle.runtime_composition_binding";
pub const RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MAJOR: u32 = 1;
pub const RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MINOR: u32 = 0;
pub const RUNTIME_COMPOSITION_BINDING_ARTIFACT_EXTENSION: &str = "deployment-composition.json";

const RUNTIME_COMPOSITION_BINDING_KIND: &str = "runtime_composition_binding";
const RUNTIME_COMPOSITION_BINDING_ADMISSION_RESULT: &str = "admitted";
// Current bindings describe one explicit runtime correlation namespace. Do not
// treat this as a unique deployment allocator until the schema grows one.
const SINGLETON_DEPLOYMENT_ID: u32 = 0;
const RUNTIME_COMPOSITION_BINDING_INITIAL_CAPACITY: usize = 2 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedCompositionFacts {
    source_language: String,
    source_module: String,
    source_fingerprint: String,
    composition_id: u32,
    instances: Vec<CheckedComponentInstanceFact>,
    port_bindings: Vec<CheckedPortBindingFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedComponentInstanceFact {
    component_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedPortBindingFact {
    importer_instance_id: u32,
    imported_port_id: u32,
    exporter_instance_id: u32,
    exported_port_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessBindingFact {
    component_instance_id: u32,
    component_id: u32,
    process_id: ProcessId,
}

pub fn render_runtime_composition_binding(
    composition_text: &str,
    artifact: &MantleArtifact,
) -> Result<String> {
    let facts = admitted_composition_facts(composition_text)?;
    let process_bindings = validate_against_runtime_artifact(&facts, artifact)?;
    render_binding_json(&facts, artifact, &process_bindings)
}

fn admitted_composition_facts(text: &str) -> Result<CheckedCompositionFacts> {
    let summary = admit_component_composition_artifact(text)?;
    if summary.admission_result != super::ComponentCompositionAdmissionResult::Admitted {
        return Err(Error::new(
            "runtime composition binding requires an admitted checked composition artifact",
        ));
    }

    let artifact = JsonObject::new(text, "component composition artifact")?;
    artifact.required_string_eq("schema_id", COMPONENT_COMPOSITION_SCHEMA_ID)?;
    artifact.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
    Ok(CheckedCompositionFacts {
        source_language: artifact.required_string("source_language")?.into_owned(),
        source_module: artifact.required_string("source_module")?.into_owned(),
        source_fingerprint: artifact.required_string("source_fingerprint")?.into_owned(),
        composition_id: artifact.required_u32("composition_id")?,
        instances: component_instance_facts(&artifact.required_array("components")?)?,
        port_bindings: port_binding_facts(&artifact.required_array("port_bindings")?)?,
    })
}

fn component_instance_facts(
    instances: &super::codec::JsonArray<'_>,
) -> Result<Vec<CheckedComponentInstanceFact>> {
    let mut facts = Vec::with_capacity(instances.count_values()?);
    instances.for_each_object(|index, object| {
        let id = object.required_u32("component_instance_id")?;
        if usize::try_from(id).ok() != Some(index) {
            return Err(Error::new(format!(
                "component_instance_id {id} at array index {index} is not canonical"
            )));
        }
        facts.push(CheckedComponentInstanceFact {
            component_id: object.required_u32("component_id")?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn port_binding_facts(
    bindings: &super::codec::JsonArray<'_>,
) -> Result<Vec<CheckedPortBindingFact>> {
    let mut facts = Vec::with_capacity(bindings.count_values()?);
    bindings.for_each_object(|index, object| {
        let id = object.required_u32("port_binding_id")?;
        if usize::try_from(id).ok() != Some(index) {
            return Err(Error::new(format!(
                "port_binding_id {id} at array index {index} is not canonical"
            )));
        }
        facts.push(CheckedPortBindingFact {
            importer_instance_id: object.required_u32("importer_instance_id")?,
            imported_port_id: object.required_u32("imported_port_id")?,
            exporter_instance_id: object.required_u32("exporter_instance_id")?,
            exported_port_id: object.required_u32("exported_port_id")?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn validate_against_runtime_artifact(
    facts: &CheckedCompositionFacts,
    artifact: &MantleArtifact,
) -> Result<Vec<ProcessBindingFact>> {
    if artifact.source_language.as_ref() != facts.source_language {
        return Err(Error::new(format!(
            "runtime composition binding source_language {:?} does not match artifact source_language {:?}",
            facts.source_language, artifact.source_language
        )));
    }
    if artifact.module != facts.source_module {
        return Err(Error::new(format!(
            "runtime composition binding source_module {:?} does not match artifact module {:?}",
            facts.source_module, artifact.module
        )));
    }
    if artifact.source_hash_fnv1a64 != facts.source_fingerprint {
        return Err(Error::new(
            "runtime composition binding source fingerprint does not match artifact source hash",
        ));
    }
    let composition = artifact
        .compositions
        .get(
            usize::try_from(facts.composition_id)
                .map_err(|_| Error::new("composition_id does not fit into usize"))?,
        )
        .ok_or_else(|| {
            Error::new(format!(
                "runtime artifact has no composition id {}",
                facts.composition_id
            ))
        })?;
    if composition.component_instances.len() != facts.instances.len() {
        return Err(Error::new(
            "runtime artifact component instance count does not match checked composition",
        ));
    }
    if composition.port_bindings.len() != facts.port_bindings.len() {
        return Err(Error::new(
            "runtime artifact port binding count does not match checked composition",
        ));
    }

    let mut seen_process_ids = vec![false; artifact.processes.len()];
    let mut process_bindings = Vec::with_capacity(facts.instances.len());
    for (index, fact) in facts.instances.iter().enumerate() {
        let runtime_instance = &composition.component_instances[index];
        if runtime_instance.component.as_u32() != fact.component_id {
            return Err(Error::new(format!(
                "component instance {index} component id does not match runtime artifact"
            )));
        }
        let component = artifact
            .components
            .get(runtime_instance.component.index())
            .ok_or_else(|| Error::new("runtime artifact component instance is out of bounds"))?;
        let export_port = artifact
            .ports
            .get(component.export_port.index())
            .ok_or_else(|| Error::new("runtime artifact component export port is out of bounds"))?;
        let process_index = export_port.target_process.index();
        let Some(was_seen) = seen_process_ids.get_mut(process_index) else {
            return Err(Error::new(
                "runtime artifact component export target process is out of bounds",
            ));
        };
        if *was_seen {
            return Err(Error::new(
                "runtime composition binding cannot correlate duplicate component instances to the same process id",
            ));
        }
        *was_seen = true;
        process_bindings.push(ProcessBindingFact {
            component_instance_id: u32::try_from(index)
                .map_err(|_| Error::new("component instance index overflowed"))?,
            component_id: fact.component_id,
            process_id: export_port.target_process,
        });
    }
    if let Some((process_id, _)) = seen_process_ids
        .iter()
        .enumerate()
        .find(|(_, was_seen)| !**was_seen)
    {
        return Err(Error::new(format!(
            "runtime composition binding must correlate every runtime process; process_id {process_id} is unbound"
        )));
    }

    for (index, fact) in facts.port_bindings.iter().enumerate() {
        let runtime_binding = &composition.port_bindings[index];
        if runtime_binding.importer.as_u32() != fact.importer_instance_id
            || runtime_binding.imported_port.as_u32() != fact.imported_port_id
            || runtime_binding.exporter.as_u32() != fact.exporter_instance_id
            || runtime_binding.exported_port.as_u32() != fact.exported_port_id
        {
            return Err(Error::new(format!(
                "port binding {index} does not match runtime artifact composition graph"
            )));
        }
    }
    Ok(process_bindings)
}

fn render_binding_json(
    facts: &CheckedCompositionFacts,
    artifact: &MantleArtifact,
    process_bindings: &[ProcessBindingFact],
) -> Result<String> {
    let mut out = String::with_capacity(RUNTIME_COMPOSITION_BINDING_INITIAL_CAPACITY);
    out.push('{');
    push_json_field(&mut out, "schema_id", RUNTIME_COMPOSITION_BINDING_SCHEMA_ID);
    out.push_str(",\"schema_version_major\":");
    let _ = write!(out, "{RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(out, "{RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MINOR}");
    out.push(',');
    push_json_field(&mut out, "artifact_kind", RUNTIME_COMPOSITION_BINDING_KIND);
    out.push_str(",\"deployment_id\":");
    let _ = write!(out, "{SINGLETON_DEPLOYMENT_ID}");
    out.push(',');
    push_json_field(&mut out, "source_language", &facts.source_language);
    out.push(',');
    push_json_field(&mut out, "source_module", &facts.source_module);
    out.push(',');
    push_json_field(&mut out, "source_fingerprint", &facts.source_fingerprint);
    out.push(',');
    push_json_field(
        &mut out,
        "source_fingerprint_algorithm",
        SOURCE_FINGERPRINT_ALGORITHM,
    );
    out.push(',');
    push_json_field(&mut out, "mantle_artifact_format", artifact.format.as_ref());
    out.push(',');
    push_json_field(
        &mut out,
        "mantle_artifact_schema_version",
        artifact.schema_version.as_ref(),
    );
    out.push(',');
    push_json_field(&mut out, "mantle_artifact_module", &artifact.module);
    out.push(',');
    push_json_field(
        &mut out,
        "mantle_artifact_source_hash_fnv1a64",
        &artifact.source_hash_fnv1a64,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "composition_schema_id",
        COMPONENT_COMPOSITION_SCHEMA_ID,
    );
    out.push_str(",\"composition_schema_version_major\":");
    let _ = write!(out, "{COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"composition_schema_version_minor\":");
    let _ = write!(out, "{COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR}");
    out.push_str(",\"composition_id\":");
    let _ = write!(out, "{}", facts.composition_id);
    push_process_bindings_json(&mut out, process_bindings);
    push_port_bindings_json(&mut out, &facts.port_bindings);
    out.push(',');
    push_json_field(
        &mut out,
        "admission_result",
        RUNTIME_COMPOSITION_BINDING_ADMISSION_RESULT,
    );
    out.push_str(",\"extensions\":{}");
    out.push('}');
    Ok(out)
}

fn push_process_bindings_json(out: &mut String, process_bindings: &[ProcessBindingFact]) {
    out.push_str(",\"component_instances\":[");
    for (index, binding) in process_bindings.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"component_instance_id\":");
        let _ = write!(out, "{}", binding.component_instance_id);
        out.push_str(",\"component_id\":");
        let _ = write!(out, "{}", binding.component_id);
        out.push_str(",\"process_id\":");
        let _ = write!(out, "{}", binding.process_id.as_u32());
        out.push('}');
    }
    out.push(']');
}

fn push_port_bindings_json(out: &mut String, port_bindings: &[CheckedPortBindingFact]) {
    out.push_str(",\"port_bindings\":[");
    for (index, binding) in port_bindings.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"port_binding_id\":");
        let _ = write!(out, "{index}");
        out.push_str(",\"importer_instance_id\":");
        let _ = write!(out, "{}", binding.importer_instance_id);
        out.push_str(",\"imported_port_id\":");
        let _ = write!(out, "{}", binding.imported_port_id);
        out.push_str(",\"exporter_instance_id\":");
        let _ = write!(out, "{}", binding.exporter_instance_id);
        out.push_str(",\"exported_port_id\":");
        let _ = write!(out, "{}", binding.exported_port_id);
        out.push('}');
    }
    out.push(']');
}
