use std::{borrow::Cow, fmt::Write as _};

use super::ast::Effect;
use super::checked::{
    CheckedComponentId, CheckedPortId, CheckedProcessId, CheckedProgram, CheckedSpawnKind,
};
use super::checked_render::{
    checked_component_authority, checked_component_label, checked_port_authority,
    checked_port_label, checked_process_label, push_checked_descriptor_json, push_json_field,
};
use super::diagnostic::{Error, Result};
use super::source_program::SourceProvenanceHash;

mod admission;
mod policy;
mod runtime_binding;
mod source_facts;

/// Strata's frontend-owned checked authority/effect fact schema.
///
/// Mantle runtime bindings validate this name by combining the loaded `.mta`
/// artifact's source language with the language-neutral
/// `.checked_authority_effects` suffix.
pub const AUTHORITY_EFFECT_SCHEMA_ID: &str = "strata.checked_authority_effects";
pub const AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR: u32 = 1;
pub const AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR: u32 = 0;
pub const AUTHORITY_EFFECT_HASH_ALG: &str = SOURCE_FINGERPRINT_ALGORITHM;
pub const AUTHORITY_EFFECT_ARTIFACT_EXTENSION: &str = "authority-effect.json";
pub const MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES: usize = 1024 * 1024;
const AUTHORITY_EFFECT_ARTIFACT_INITIAL_CAPACITY: usize = 4 * 1024;
const ARTIFACT_KIND: &str = "checked_authority_effects";
const SOURCE_LANGUAGE: &str = "strata";
const SOURCE_FINGERPRINT_ALGORITHM: &str = "fnv1a64-diagnostic";
const ADMISSION_RESULT_ADMITTED: &str = "admitted";

pub use policy::{
    AUTHORITY_POLICY_ARTIFACT_EXTENSION, AUTHORITY_POLICY_SCHEMA_ID,
    AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR, AUTHORITY_POLICY_SCHEMA_VERSION_MINOR,
    AuthorityPolicyAdmissionResult, AuthorityPolicyAdmissionSummary, AuthorityPolicyBuildOptions,
    AuthorityPolicyDecision, MAX_AUTHORITY_POLICY_ARTIFACT_BYTES, admit_authority_policy_artifact,
    render_authority_policy_admission_summary, render_authority_policy_artifact,
};
pub use runtime_binding::{
    RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_ID,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MAJOR,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MINOR, render_runtime_authority_effect_binding,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityEffectArtifactAdmitFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityEffectAdmissionResult {
    Admitted,
}

impl AuthorityEffectAdmissionResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => ADMISSION_RESULT_ADMITTED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEffectAdmissionSummary {
    pub schema_id: &'static str,
    pub schema_version_major: u32,
    pub schema_version_minor: u32,
    pub hash_alg: &'static str,
    pub protocol_count: usize,
    pub port_count: usize,
    pub process_count: usize,
    pub component_count: usize,
    pub authority_count: usize,
    pub spawn_site_count: usize,
    pub transition_effect_count: usize,
    pub component_authority_surface_count: usize,
    pub admission_result: AuthorityEffectAdmissionResult,
}

pub fn render_authority_effect_artifact(
    program: &CheckedProgram,
    source_path: &str,
    source_hash: &SourceProvenanceHash,
) -> Result<String> {
    let source_path = portable_source_path_metadata(source_path);
    let mut out = String::with_capacity(AUTHORITY_EFFECT_ARTIFACT_INITIAL_CAPACITY);
    out.push('{');
    push_json_field(&mut out, "schema_id", AUTHORITY_EFFECT_SCHEMA_ID);
    out.push_str(",\"schema_version_major\":");
    let _ = write!(out, "{AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(out, "{AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR}");
    out.push(',');
    push_json_field(&mut out, "artifact_kind", ARTIFACT_KIND);
    out.push(',');
    push_json_field(&mut out, "hash_alg", AUTHORITY_EFFECT_HASH_ALG);
    out.push(',');
    push_json_field(&mut out, "source_language", SOURCE_LANGUAGE);
    out.push(',');
    push_json_field(&mut out, "source_module", program.module_name());
    out.push(',');
    push_json_field(&mut out, "source_path", source_path.as_ref());
    out.push(',');
    push_json_field(&mut out, "source_fingerprint", source_hash.fnv1a64());
    out.push(',');
    push_json_field(
        &mut out,
        "source_fingerprint_algorithm",
        SOURCE_FINGERPRINT_ALGORITHM,
    );
    push_table_counts_json(&mut out, program);
    push_processes_json(&mut out, program)?;
    push_component_surfaces_json(&mut out, program)?;
    out.push_str(",\"policy_inputs\":[]");
    out.push_str(",\"admission_policy_hash\":null");
    out.push(',');
    push_json_field(&mut out, "admission_result", ADMISSION_RESULT_ADMITTED);
    out.push_str(",\"diagnostic_set_hash\":null");
    out.push_str(",\"extensions\":{}");
    out.push('}');
    if out.len() > MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "authority/effect artifact exceeds maximum size of {MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES} bytes"
        )));
    }
    Ok(out)
}

fn portable_source_path_metadata(source_path: &str) -> Cow<'_, str> {
    portable_source_path_metadata_for_separator(source_path, std::path::MAIN_SEPARATOR)
}

fn portable_source_path_metadata_for_separator(source_path: &str, separator: char) -> Cow<'_, str> {
    if separator == '\\' && source_path.as_bytes().contains(&b'\\') {
        Cow::Owned(source_path.replace('\\', "/"))
    } else {
        Cow::Borrowed(source_path)
    }
}

pub fn admit_authority_effect_artifact(text: &str) -> Result<AuthorityEffectAdmissionSummary> {
    if text.len() > MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "authority/effect artifact exceeds maximum size of {MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES} bytes"
        )));
    }
    admission::validate_authority_effect_artifact(text)
}

pub fn render_authority_effect_admission_summary(
    summary: &AuthorityEffectAdmissionSummary,
    artifact_path: &str,
    format: AuthorityEffectArtifactAdmitFormat,
) -> String {
    match format {
        AuthorityEffectArtifactAdmitFormat::Text => render_summary_text(summary, artifact_path),
        AuthorityEffectArtifactAdmitFormat::Json => render_summary_json(summary, artifact_path),
    }
}

fn push_table_counts_json(out: &mut String, program: &CheckedProgram) {
    out.push_str(",\"protocol_count\":");
    let _ = write!(out, "{}", program.protocols().len());
    out.push_str(",\"port_count\":");
    let _ = write!(out, "{}", program.ports().len());
    out.push_str(",\"component_count\":");
    let _ = write!(out, "{}", program.components().len());
}

fn push_processes_json(out: &mut String, program: &CheckedProgram) -> Result<()> {
    out.push_str(",\"processes\":[");
    for (process_index, process) in program.processes().iter().enumerate() {
        if process_index > 0 {
            out.push(',');
        }
        out.push_str("{\"process_id\":");
        let process_id = CheckedProcessId::from_index(process_index)?;
        let _ = write!(out, "{}", process_id.as_u32());
        out.push(',');
        push_json_field(out, "process", process.debug_name().as_str());
        out.push_str(",\"state_count\":");
        let _ = write!(out, "{}", process.state_values().len());
        out.push_str(",\"message_count\":");
        let _ = write!(out, "{}", process.message_cases().len());
        out.push_str(",\"authorities\":[");
        for (authority_index, authority) in process.authorities().iter().enumerate() {
            if authority_index > 0 {
                out.push(',');
            }
            out.push_str("{\"authority_id\":");
            let _ = write!(out, "{authority_index}");
            out.push(',');
            push_json_field(out, "name", authority.debug_name().as_str());
            out.push_str(",\"descriptor\":");
            push_checked_descriptor_json(out, program, authority.descriptor());
            out.push('}');
        }
        out.push_str("],\"spawn_sites\":[");
        for (site_index, site) in process.spawn_sites().iter().enumerate() {
            if site_index > 0 {
                out.push(',');
            }
            out.push_str("{\"spawn_site_id\":");
            let _ = write!(out, "{site_index}");
            out.push(',');
            push_json_field(out, "kind", spawn_kind_str(site.kind()));
            out.push_str(",\"target_process_id\":");
            let _ = write!(out, "{}", site.target().as_u32());
            out.push(',');
            push_json_field(
                out,
                "target_process",
                checked_process_label(program, site.target()),
            );
            push_optional_u32_field(out, "authority_id", site.authority().map(|id| id.as_u32()));
            push_optional_u32_field(
                out,
                "supervisor_id",
                site.supervisor().map(|id| id.as_u32()),
            );
            push_optional_u32_field(
                out,
                "supervisor_child_id",
                site.child().map(|id| id.as_u32()),
            );
            out.push('}');
        }
        out.push_str("],\"supervisor_spawn_facts\":[");
        for (supervisor_index, supervisor) in process.supervisor_plans().iter().enumerate() {
            if supervisor_index > 0 {
                out.push(',');
            }
            out.push_str("{\"supervisor_id\":");
            let _ = write!(out, "{supervisor_index}");
            out.push_str(",\"children\":[");
            for (child_index, child) in supervisor.children().iter().enumerate() {
                if child_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"child_id\":");
                let _ = write!(out, "{child_index}");
                out.push(',');
                push_json_field(out, "child", child.debug_name().as_str());
                out.push_str(",\"target_process_id\":");
                let _ = write!(out, "{}", child.target().as_u32());
                out.push(',');
                push_json_field(
                    out,
                    "target_process",
                    checked_process_label(program, child.target()),
                );
                out.push_str(",\"spawn_site_id\":");
                let _ = write!(out, "{}", child.spawn_site().as_u32());
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("],\"transition_effects\":[");
        for (transition_index, transition) in process.transitions().iter().enumerate() {
            if transition_index > 0 {
                out.push(',');
            }
            out.push_str("{\"transition_id\":");
            let _ = write!(out, "{transition_index}");
            out.push_str(",\"message_id\":");
            let _ = write!(out, "{}", transition.message().as_u32());
            push_optional_u32_field(
                out,
                "current_state_id",
                transition.current_state().map(|id| id.as_u32()),
            );
            out.push_str(",\"effects\":[");
            for (effect_index, effect) in transition.effects().iter().copied().enumerate() {
                if effect_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"effect_id\":");
                let _ = write!(out, "{effect_index}");
                out.push(',');
                push_json_field(out, "effect", effect_str(effect));
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }
    out.push(']');
    Ok(())
}

fn push_component_surfaces_json(out: &mut String, program: &CheckedProgram) -> Result<()> {
    out.push_str(",\"component_authority_surfaces\":[");
    for (component_index, component) in program.components().iter().enumerate() {
        if component_index > 0 {
            out.push(',');
        }
        let component_id = CheckedComponentId::from_index(component_index)?;
        let export_port = component.export_port();
        out.push_str("{\"component_id\":");
        let _ = write!(out, "{}", component_id.as_u32());
        out.push(',');
        push_json_field(
            out,
            "component",
            checked_component_label(program, component_id),
        );
        out.push_str(",\"export_port_id\":");
        let _ = write!(out, "{}", export_port.as_u32());
        out.push(',');
        push_json_field(out, "export_port", checked_port_label(program, export_port));
        out.push_str(",\"import_port_count\":");
        let _ = write!(out, "{}", component.import_ports().len());
        out.push_str(",\"component_authority\":");
        push_checked_descriptor_json(
            out,
            program,
            checked_component_authority(program, component_id),
        );
        out.push_str(",\"export_port_authority\":");
        push_checked_descriptor_json(out, program, checked_port_authority(program, export_port));
        out.push_str(",\"import_port_authorities\":[");
        for (port_index, import_port) in component.import_ports().iter().copied().enumerate() {
            if port_index > 0 {
                out.push(',');
            }
            push_port_authority_json(out, program, import_port);
        }
        out.push_str("]}");
    }
    out.push(']');
    Ok(())
}

fn push_port_authority_json(out: &mut String, program: &CheckedProgram, port: CheckedPortId) {
    out.push_str("{\"port_id\":");
    let _ = write!(out, "{}", port.as_u32());
    out.push(',');
    push_json_field(out, "port", checked_port_label(program, port));
    out.push_str(",\"port_authority\":");
    push_checked_descriptor_json(out, program, checked_port_authority(program, port));
    out.push('}');
}

fn push_optional_u32_field(out: &mut String, field: &str, value: Option<u32>) {
    out.push_str(",\"");
    out.push_str(field);
    out.push_str("\":");
    match value {
        Some(value) => {
            let _ = write!(out, "{value}");
        }
        None => out.push_str("null"),
    }
}

fn render_summary_text(summary: &AuthorityEffectAdmissionSummary, artifact_path: &str) -> String {
    let mut out = String::new();
    out.push_str("strata authority/effect admission ");
    out.push_str(artifact_path);
    out.push('\n');
    out.push_str("schema_id: ");
    out.push_str(summary.schema_id);
    out.push('\n');
    out.push_str("schema_version: ");
    let _ = writeln!(
        out,
        "{}.{}",
        summary.schema_version_major, summary.schema_version_minor
    );
    out.push_str("hash_alg: ");
    out.push_str(summary.hash_alg);
    out.push('\n');
    let _ = writeln!(out, "protocols: {}", summary.protocol_count);
    let _ = writeln!(out, "ports: {}", summary.port_count);
    let _ = writeln!(out, "processes: {}", summary.process_count);
    let _ = writeln!(out, "components: {}", summary.component_count);
    let _ = writeln!(out, "authorities: {}", summary.authority_count);
    let _ = writeln!(out, "spawn_sites: {}", summary.spawn_site_count);
    let _ = writeln!(
        out,
        "transition_effects: {}",
        summary.transition_effect_count
    );
    let _ = writeln!(
        out,
        "component_authority_surfaces: {}",
        summary.component_authority_surface_count
    );
    out.push_str("admission_result: ");
    out.push_str(summary.admission_result.as_str());
    out.push('\n');
    out
}

fn render_summary_json(summary: &AuthorityEffectAdmissionSummary, artifact_path: &str) -> String {
    let mut out = String::new();
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
    out.push_str(",\"protocol_count\":");
    let _ = write!(out, "{}", summary.protocol_count);
    out.push_str(",\"port_count\":");
    let _ = write!(out, "{}", summary.port_count);
    out.push_str(",\"process_count\":");
    let _ = write!(out, "{}", summary.process_count);
    out.push_str(",\"component_count\":");
    let _ = write!(out, "{}", summary.component_count);
    out.push_str(",\"authority_count\":");
    let _ = write!(out, "{}", summary.authority_count);
    out.push_str(",\"spawn_site_count\":");
    let _ = write!(out, "{}", summary.spawn_site_count);
    out.push_str(",\"transition_effect_count\":");
    let _ = write!(out, "{}", summary.transition_effect_count);
    out.push_str(",\"component_authority_surface_count\":");
    let _ = write!(out, "{}", summary.component_authority_surface_count);
    out.push(',');
    push_json_field(
        &mut out,
        "admission_result",
        summary.admission_result.as_str(),
    );
    out.push('}');
    out
}

fn effect_str(effect: Effect) -> &'static str {
    match effect {
        Effect::Emit => "emit",
        Effect::Spawn => "spawn",
        Effect::Send => "send",
    }
}

fn spawn_kind_str(kind: CheckedSpawnKind) -> &'static str {
    match kind {
        CheckedSpawnKind::DynamicLocal => "dynamic_local",
        CheckedSpawnKind::LexicalSupervisorChild => "lexical_supervisor_child",
    }
}

fn artifact_effect_str(effect: mantle_artifact::ArtifactEffect) -> &'static str {
    match effect {
        mantle_artifact::ArtifactEffect::Emit => "emit",
        mantle_artifact::ArtifactEffect::Spawn => "spawn",
        mantle_artifact::ArtifactEffect::Send => "send",
    }
}

fn artifact_spawn_kind_str(kind: mantle_artifact::ArtifactSpawnKind) -> &'static str {
    match kind {
        mantle_artifact::ArtifactSpawnKind::DynamicLocal => "dynamic_local",
        mantle_artifact::ArtifactSpawnKind::LexicalSupervisorChild => "lexical_supervisor_child",
    }
}

#[cfg(test)]
mod tests {
    use super::portable_source_path_metadata_for_separator;

    #[test]
    fn artifact_normalizes_source_path_metadata_to_portable_separators() {
        assert_eq!(
            portable_source_path_metadata_for_separator("examples\\authority_effect.str", '\\')
                .as_ref(),
            "examples/authority_effect.str"
        );
        assert_eq!(
            portable_source_path_metadata_for_separator("examples\\authority_effect.str", '/')
                .as_ref(),
            "examples\\authority_effect.str"
        );
    }
}
