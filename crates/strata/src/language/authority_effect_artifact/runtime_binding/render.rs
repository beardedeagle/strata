use std::fmt::Write as _;

use mantle_artifact::MantleArtifact;

use super::super::super::checked_render::push_json_field;
use super::super::super::diagnostic::{Error, Result};
use super::super::{
    ADMISSION_RESULT_ADMITTED, AUTHORITY_EFFECT_SCHEMA_ID, AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
    AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR, SOURCE_FINGERPRINT_ALGORITHM,
};
use super::{
    CheckedAuthorityEffectFacts, DescriptorFact, MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_ID,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MAJOR,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MINOR, RuntimeSpawnAuthorityPolicy,
    SpawnSiteFact, TransitionEffectFact,
};

const RUNTIME_AUTHORITY_EFFECT_BINDING_KIND: &str = "runtime_authority_effect_binding";
const SINGLETON_DEPLOYMENT_ID: u32 = 0;
const RUNTIME_BINDING_INITIAL_CAPACITY: usize = 4 * 1024;

pub(super) fn render_binding_json(
    facts: &CheckedAuthorityEffectFacts<'_>,
    artifact: &MantleArtifact,
    spawn_authority_policy: RuntimeSpawnAuthorityPolicy,
) -> Result<String> {
    let mut out = String::with_capacity(RUNTIME_BINDING_INITIAL_CAPACITY);
    out.push('{');
    push_json_field(
        &mut out,
        "schema_id",
        RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_ID,
    );
    out.push_str(",\"schema_version_major\":");
    let _ = write!(
        out,
        "{RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MAJOR}"
    );
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(
        out,
        "{RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MINOR}"
    );
    out.push(',');
    push_json_field(
        &mut out,
        "artifact_kind",
        RUNTIME_AUTHORITY_EFFECT_BINDING_KIND,
    );
    out.push_str(",\"deployment_id\":");
    let _ = write!(out, "{SINGLETON_DEPLOYMENT_ID}");
    out.push(',');
    push_json_field(&mut out, "source_language", facts.source_language.as_ref());
    out.push(',');
    push_json_field(&mut out, "source_module", facts.source_module.as_ref());
    out.push(',');
    push_json_field(
        &mut out,
        "source_fingerprint",
        facts.source_fingerprint.as_ref(),
    );
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
        "authority_effect_schema_id",
        AUTHORITY_EFFECT_SCHEMA_ID,
    );
    out.push_str(",\"authority_effect_schema_version_major\":");
    let _ = write!(out, "{AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"authority_effect_schema_version_minor\":");
    let _ = write!(out, "{AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR}");
    push_processes_json(&mut out, facts);
    push_component_surfaces_json(&mut out, facts);
    out.push_str(",\"policy\":{");
    push_json_field(
        &mut out,
        "spawn_authority_policy",
        spawn_authority_policy.as_str(),
    );
    out.push('}');
    out.push(',');
    push_json_field(&mut out, "admission_result", ADMISSION_RESULT_ADMITTED);
    out.push_str(",\"extensions\":{}");
    out.push('}');
    if out.len() > MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES {
        return Err(Error::new(format!(
            "runtime authority/effect binding exceeds maximum size of {MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES} bytes"
        )));
    }
    Ok(out)
}

fn push_processes_json(out: &mut String, facts: &CheckedAuthorityEffectFacts<'_>) {
    out.push_str(",\"processes\":[");
    for (process_id, process) in facts.processes.iter().enumerate() {
        if process_id > 0 {
            out.push(',');
        }
        out.push_str("{\"process_id\":");
        let _ = write!(out, "{process_id}");
        push_authorities_json(out, &process.authorities);
        push_spawn_sites_json(out, &process.spawn_sites);
        push_transition_effects_json(out, &process.transitions);
        out.push('}');
    }
    out.push(']');
}

fn push_authorities_json(out: &mut String, authorities: &[DescriptorFact]) {
    out.push_str(",\"authorities\":[");
    for (authority_id, descriptor) in authorities.iter().copied().enumerate() {
        if authority_id > 0 {
            out.push(',');
        }
        out.push_str("{\"authority_id\":");
        let _ = write!(out, "{authority_id}");
        out.push_str(",\"descriptor\":");
        push_descriptor_json(out, descriptor);
        out.push('}');
    }
    out.push(']');
}

fn push_spawn_sites_json(out: &mut String, sites: &[SpawnSiteFact]) {
    out.push_str(",\"spawn_sites\":[");
    for (spawn_site_id, site) in sites.iter().enumerate() {
        if spawn_site_id > 0 {
            out.push(',');
        }
        out.push_str("{\"spawn_site_id\":");
        let _ = write!(out, "{spawn_site_id}");
        out.push(',');
        push_json_field(out, "kind", site.kind.as_str());
        out.push_str(",\"target_process_id\":");
        let _ = write!(out, "{}", site.target_process_id);
        push_optional_u32_field(out, "authority_id", site.authority_id);
        push_optional_u32_field(out, "supervisor_id", site.supervisor_id);
        push_optional_u32_field(out, "supervisor_child_id", site.supervisor_child_id);
        out.push('}');
    }
    out.push(']');
}

fn push_transition_effects_json(out: &mut String, transitions: &[TransitionEffectFact]) {
    out.push_str(",\"transition_effects\":[");
    for (transition_id, transition) in transitions.iter().enumerate() {
        if transition_id > 0 {
            out.push(',');
        }
        out.push_str("{\"transition_id\":");
        let _ = write!(out, "{transition_id}");
        out.push_str(",\"message_id\":");
        let _ = write!(out, "{}", transition.message_id);
        push_optional_u32_field(out, "current_state_id", transition.current_state_id);
        out.push_str(",\"effects\":[");
        for (effect_id, effect) in transition.effects.iter().copied().enumerate() {
            if effect_id > 0 {
                out.push(',');
            }
            out.push_str("{\"effect_id\":");
            let _ = write!(out, "{effect_id}");
            out.push(',');
            push_json_field(out, "effect", effect.as_str());
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
}

fn push_component_surfaces_json(out: &mut String, facts: &CheckedAuthorityEffectFacts<'_>) {
    out.push_str(",\"component_authority_surfaces\":[");
    for (component_id, surface) in facts.component_surfaces.iter().enumerate() {
        if component_id > 0 {
            out.push(',');
        }
        out.push_str("{\"component_id\":");
        let _ = write!(out, "{component_id}");
        out.push_str(",\"export_port_id\":");
        let _ = write!(out, "{}", surface.export_port_id);
        out.push_str(",\"component_authority\":");
        push_descriptor_json(out, surface.component_authority);
        out.push_str(",\"export_port_authority\":");
        push_descriptor_json(out, surface.export_port_authority);
        out.push_str(",\"import_port_authorities\":[");
        for (index, port) in surface.import_port_authorities.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"port_id\":");
            let _ = write!(out, "{}", port.port_id);
            out.push_str(",\"port_authority\":");
            push_descriptor_json(out, port.authority);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
}

fn push_descriptor_json(out: &mut String, descriptor: DescriptorFact) {
    match descriptor {
        DescriptorFact::Spawn { target_process_id } => {
            out.push_str("{\"kind\":\"spawn\",\"target_process_id\":");
            let _ = write!(out, "{target_process_id}");
            out.push('}');
        }
        DescriptorFact::ProtocolBoundary { protocol_id } => {
            out.push_str("{\"kind\":\"protocol_boundary\",\"protocol_id\":");
            let _ = write!(out, "{protocol_id}");
            out.push('}');
        }
        DescriptorFact::PortConnect { port_id } => {
            out.push_str("{\"kind\":\"port_connect\",\"port_id\":");
            let _ = write!(out, "{port_id}");
            out.push('}');
        }
        DescriptorFact::ComponentExport { component_id } => {
            out.push_str("{\"kind\":\"component_export\",\"component_id\":");
            let _ = write!(out, "{component_id}");
            out.push('}');
        }
    }
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
