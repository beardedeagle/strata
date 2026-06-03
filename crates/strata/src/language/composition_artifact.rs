use std::fmt::Write as _;

use super::checked::{CheckedComponentInstance, CheckedPortBinding, CheckedProgram};
use super::checked_render::{
    checked_component_authority, checked_component_label, checked_port_authority,
    checked_port_label, port_protocol, push_checked_descriptor_json,
    push_component_instance_ref_json, push_component_ref_json, push_json_field, push_port_ref_json,
    push_protocol_ref_json,
};
use super::diagnostic::{Error, Result};
use super::source_program::SourceProvenanceHash;

mod admission;
mod codec;

pub const COMPONENT_COMPOSITION_SCHEMA_ID: &str = "strata.checked_component_composition";
pub const COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR: u32 = 1;
pub const COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR: u32 = 0;
pub const COMPONENT_COMPOSITION_HASH_ALG: &str = SOURCE_FINGERPRINT_ALGORITHM;
pub const COMPONENT_COMPOSITION_ARTIFACT_EXTENSION: &str = "component-composition.json";
pub const MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES: usize = 1024 * 1024;
const COMPONENT_COMPOSITION_ARTIFACT_INITIAL_CAPACITY: usize = 3 * 1024;
pub const ARTIFACT_KIND: &str = "checked_component_composition";
pub const SOURCE_LANGUAGE: &str = "strata";
pub const SOURCE_FINGERPRINT_ALGORITHM: &str = "fnv1a64-diagnostic";
pub(super) const ADMISSION_RESULT_ADMITTED: &str = "admitted";
pub(super) const ADMISSION_RESULT_REJECTED: &str = "rejected";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCompositionArtifactAdmitFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCompositionAdmissionResult {
    Admitted,
    Rejected,
}

impl ComponentCompositionAdmissionResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => ADMISSION_RESULT_ADMITTED,
            Self::Rejected => ADMISSION_RESULT_REJECTED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentCompositionAdmissionSummary {
    pub schema_id: &'static str,
    pub schema_version_major: u32,
    pub schema_version_minor: u32,
    pub hash_alg: &'static str,
    pub composition_id: u32,
    pub component_instance_count: usize,
    pub port_binding_count: usize,
    pub authority_edge_count: usize,
    pub unsatisfied_import_count: usize,
    pub rejected_binding_count: usize,
    pub rejection_reason_count: usize,
    pub admission_result: ComponentCompositionAdmissionResult,
}

pub fn render_component_composition_artifact(
    program: &CheckedProgram,
    source_path: &str,
    source_hash: &SourceProvenanceHash,
    composition_name: Option<&str>,
) -> Result<String> {
    let (composition_index, composition) = select_composition(program, composition_name)?;
    let mut out = String::with_capacity(COMPONENT_COMPOSITION_ARTIFACT_INITIAL_CAPACITY);
    out.push('{');
    push_json_field(&mut out, "schema_id", COMPONENT_COMPOSITION_SCHEMA_ID);
    out.push_str(",\"schema_version_major\":");
    let _ = write!(out, "{COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(out, "{COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR}");
    out.push(',');
    push_json_field(&mut out, "artifact_kind", ARTIFACT_KIND);
    out.push(',');
    push_json_field(&mut out, "hash_alg", COMPONENT_COMPOSITION_HASH_ALG);
    out.push(',');
    push_json_field(&mut out, "source_language", SOURCE_LANGUAGE);
    out.push(',');
    push_json_field(&mut out, "source_module", program.module_name());
    out.push(',');
    push_json_field(&mut out, "source_path", source_path);
    out.push(',');
    push_json_field(&mut out, "source_fingerprint", source_hash.fnv1a64());
    out.push(',');
    push_json_field(
        &mut out,
        "source_fingerprint_algorithm",
        SOURCE_FINGERPRINT_ALGORITHM,
    );
    out.push_str(",\"composition_id\":");
    let _ = write!(out, "{composition_index}");
    out.push(',');
    push_json_field(
        &mut out,
        "composition_name",
        composition.debug_name().as_str(),
    );
    push_component_instances_json(&mut out, program, composition.component_instances());
    out.push_str(",\"capability_bindings\":[]");
    out.push_str(",\"interface_bindings\":[]");
    push_port_bindings_json(
        &mut out,
        program,
        composition.component_instances(),
        composition.port_bindings(),
    );
    out.push_str(",\"runtime_feature_bindings\":[]");
    out.push_str(",\"archive_format_bindings\":[]");
    out.push_str(",\"crypto_policy_bindings\":[]");
    push_authority_edges_json(
        &mut out,
        program,
        composition.component_instances(),
        composition.port_bindings(),
    );
    out.push_str(",\"unsatisfied_imports\":[]");
    out.push_str(",\"admission_policy_hash\":null");
    out.push(',');
    push_json_field(&mut out, "admission_result", ADMISSION_RESULT_ADMITTED);
    out.push_str(",\"diagnostic_set_hash\":null");
    out.push_str(",\"extensions\":{}");
    out.push('}');
    if out.len() > MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "component composition artifact exceeds maximum size of {MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES} bytes"
        )));
    }
    Ok(out)
}

pub fn admit_component_composition_artifact(
    text: &str,
) -> Result<ComponentCompositionAdmissionSummary> {
    if text.len() > MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "component composition artifact exceeds maximum size of {MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES} bytes"
        )));
    }
    admission::validate_component_composition_artifact(text)
}

pub fn render_component_composition_admission_summary(
    summary: &ComponentCompositionAdmissionSummary,
    artifact_path: &str,
    format: ComponentCompositionArtifactAdmitFormat,
) -> String {
    match format {
        ComponentCompositionArtifactAdmitFormat::Text => {
            render_summary_text(summary, artifact_path)
        }
        ComponentCompositionArtifactAdmitFormat::Json => {
            render_summary_json(summary, artifact_path)
        }
    }
}

fn select_composition<'a>(
    program: &'a CheckedProgram,
    composition_name: Option<&str>,
) -> Result<(usize, &'a super::checked::CheckedComposition)> {
    if let Some(name) = composition_name {
        return program
            .compositions()
            .iter()
            .enumerate()
            .find(|(_, composition)| composition.debug_name().as_str() == name)
            .ok_or_else(|| Error::new(format!("source program declares no composition {name}")));
    }
    match program.compositions() {
        [] => Err(Error::new(
            "source program declares no component composition",
        )),
        [composition] => Ok((0, composition)),
        compositions => Err(Error::new(format!(
            "source program declares {} compositions; pass --composition <name>",
            compositions.len()
        ))),
    }
}

fn push_component_instances_json(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
) {
    out.push_str(",\"components\":[");
    for (index, instance) in instances.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"component_instance_id\":");
        let _ = write!(out, "{index}");
        out.push(',');
        push_json_field(out, "instance", instance.debug_name().as_str());
        out.push_str(",\"component_id\":");
        let _ = write!(out, "{}", instance.component().as_u32());
        out.push(',');
        push_json_field(
            out,
            "component",
            checked_component_label(program, instance.component()),
        );
        out.push_str(",\"component_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_component_authority(program, instance.component()),
        );
        push_component_import_ports_json(out, program, instance);
        out.push_str(",\"export_port\":");
        let component = &program.components()[instance.component().index()];
        push_component_port_json(out, program, component.export_port());
        out.push('}');
    }
    out.push(']');
}

fn push_component_import_ports_json(
    out: &mut String,
    program: &CheckedProgram,
    instance: &CheckedComponentInstance,
) {
    let component = &program.components()[instance.component().index()];
    out.push_str(",\"import_ports\":[");
    for (index, port) in component.import_ports().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_component_port_json(out, program, *port);
    }
    out.push(']');
}

fn push_component_port_json(
    out: &mut String,
    program: &CheckedProgram,
    port: super::checked::CheckedPortId,
) {
    out.push_str("{\"port_id\":");
    let _ = write!(out, "{}", port.as_u32());
    out.push(',');
    push_json_field(out, "port", checked_port_label(program, port));
    push_protocol_ref_json(out, program, port_protocol(program, port));
    out.push_str(",\"required_authority\":");
    push_checked_descriptor_json(out, program, checked_port_authority(program, port));
    out.push('}');
}

fn push_port_bindings_json(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    bindings: &[CheckedPortBinding],
) {
    out.push_str(",\"port_bindings\":[");
    for (index, binding) in bindings.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let protocol = port_protocol(program, binding.imported_port());
        out.push('{');
        out.push_str("\"port_binding_id\":");
        let _ = write!(out, "{}", binding.id().as_u32());
        push_component_instance_ref_json(out, "importer", instances, binding.importer());
        push_port_ref_json(out, "imported_port", program, binding.imported_port());
        push_component_instance_ref_json(out, "exporter", instances, binding.exporter());
        push_port_ref_json(out, "exported_port", program, binding.exported_port());
        push_protocol_ref_json(out, program, protocol);
        out.push(',');
        push_json_field(out, "binding_result", ADMISSION_RESULT_ADMITTED);
        out.push(',');
        push_json_field(out, "rejection_reason", "");
        out.push_str(",\"imported_port_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_port_authority(program, binding.imported_port()),
        );
        out.push_str(",\"exported_port_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_port_authority(program, binding.exported_port()),
        );
        out.push('}');
    }
    out.push(']');
}

fn push_authority_edges_json(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    bindings: &[CheckedPortBinding],
) {
    out.push_str(",\"cross_component_authority_edges\":[");
    for (index, binding) in bindings.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let importer = &instances[binding.importer().index()];
        let exporter = &instances[binding.exporter().index()];
        let protocol = port_protocol(program, binding.imported_port());
        out.push('{');
        out.push_str("\"port_binding_id\":");
        let _ = write!(out, "{}", binding.id().as_u32());
        out.push(',');
        push_json_field(out, "edge_kind", "port_binding");
        push_component_ref_json(out, "exporter_component", program, exporter.component());
        push_component_ref_json(out, "importer_component", program, importer.component());
        push_port_ref_json(out, "exported_port", program, binding.exported_port());
        push_port_ref_json(out, "imported_port", program, binding.imported_port());
        push_protocol_ref_json(out, program, protocol);
        out.push_str(",\"export_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_component_authority(program, exporter.component()),
        );
        out.push_str(",\"exported_port_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_port_authority(program, binding.exported_port()),
        );
        out.push_str(",\"imported_port_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_port_authority(program, binding.imported_port()),
        );
        out.push('}');
    }
    out.push(']');
}

fn render_summary_text(
    summary: &ComponentCompositionAdmissionSummary,
    artifact_path: &str,
) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("strata component composition admission ");
    out.push_str(artifact_path);
    out.push('\n');
    out.push_str("schema_id: ");
    out.push_str(summary.schema_id);
    out.push('\n');
    let _ = writeln!(
        out,
        "schema_version: {}.{}",
        summary.schema_version_major, summary.schema_version_minor
    );
    out.push_str("hash_alg: ");
    out.push_str(summary.hash_alg);
    out.push('\n');
    let _ = writeln!(out, "composition_id: {}", summary.composition_id);
    out.push_str("admission_result: ");
    out.push_str(summary.admission_result.as_str());
    out.push('\n');
    let _ = writeln!(
        out,
        "component_instances: {}",
        summary.component_instance_count
    );
    let _ = writeln!(out, "port_bindings: {}", summary.port_binding_count);
    let _ = writeln!(out, "authority_edges: {}", summary.authority_edge_count);
    let _ = writeln!(
        out,
        "unsatisfied_imports: {}",
        summary.unsatisfied_import_count
    );
    let _ = writeln!(out, "rejected_bindings: {}", summary.rejected_binding_count);
    let _ = writeln!(out, "rejection_reasons: {}", summary.rejection_reason_count);
    out
}

fn render_summary_json(
    summary: &ComponentCompositionAdmissionSummary,
    artifact_path: &str,
) -> String {
    let mut out = String::with_capacity(256);
    out.push('{');
    push_json_field(&mut out, "schema_id", summary.schema_id);
    out.push_str(",\"schema_version_major\":");
    let _ = write!(out, "{}", summary.schema_version_major);
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(out, "{}", summary.schema_version_minor);
    out.push(',');
    push_json_field(&mut out, "hash_alg", summary.hash_alg);
    out.push(',');
    push_json_field(&mut out, "artifact", artifact_path);
    out.push_str(",\"composition_id\":");
    let _ = write!(out, "{}", summary.composition_id);
    out.push(',');
    push_json_field(
        &mut out,
        "admission_result",
        summary.admission_result.as_str(),
    );
    out.push_str(",\"component_instance_count\":");
    let _ = write!(out, "{}", summary.component_instance_count);
    out.push_str(",\"port_binding_count\":");
    let _ = write!(out, "{}", summary.port_binding_count);
    out.push_str(",\"authority_edge_count\":");
    let _ = write!(out, "{}", summary.authority_edge_count);
    out.push_str(",\"unsatisfied_import_count\":");
    let _ = write!(out, "{}", summary.unsatisfied_import_count);
    out.push_str(",\"rejected_binding_count\":");
    let _ = write!(out, "{}", summary.rejected_binding_count);
    out.push_str(",\"rejection_reason_count\":");
    let _ = write!(out, "{}", summary.rejection_reason_count);
    out.push('}');
    out
}
