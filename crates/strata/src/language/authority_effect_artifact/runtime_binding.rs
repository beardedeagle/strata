use std::borrow::Cow;

use mantle_artifact::{ArtifactCapabilityDescriptor, ArtifactEffect, MantleArtifact};

use super::super::diagnostic::{Error, Result};
use super::{artifact_effect_str, artifact_spawn_kind_str};

pub const RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_ID: &str =
    "mantle.runtime_authority_effect_binding";
pub const RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MAJOR: u32 = 1;
pub const RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MINOR: u32 = 0;
pub const RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION: &str =
    "authority-effect-binding.json";
pub const MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES: usize = 1024 * 1024;

mod render;
mod source_facts;

use render::render_binding_json;
use source_facts::admitted_authority_effect_facts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSpawnAuthorityPolicy {
    AdmitDeclared,
    DenyDeclared,
}

impl RuntimeSpawnAuthorityPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdmitDeclared => "admit_declared",
            Self::DenyDeclared => "deny_declared",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedAuthorityEffectFacts<'a> {
    source_language: Cow<'a, str>,
    source_module: Cow<'a, str>,
    source_fingerprint: Cow<'a, str>,
    processes: Vec<CheckedProcessFacts>,
    component_surfaces: Vec<CheckedComponentSurfaceFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedProcessFacts {
    authorities: Vec<DescriptorFact>,
    spawn_sites: Vec<SpawnSiteFact>,
    transitions: Vec<TransitionEffectFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorFact {
    Spawn { target_process_id: u32 },
    ProtocolBoundary { protocol_id: u32 },
    PortConnect { port_id: u32 },
    ComponentExport { component_id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpawnSiteFact {
    kind: SpawnKindFact,
    target_process_id: u32,
    authority_id: Option<u32>,
    supervisor_id: Option<u32>,
    supervisor_child_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpawnKindFact {
    DynamicLocal,
    LexicalSupervisorChild,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionEffectFact {
    message_id: u32,
    current_state_id: Option<u32>,
    effects: Vec<EffectFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectFact {
    Emit,
    Spawn,
    Send,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedComponentSurfaceFact {
    export_port_id: u32,
    component_authority: DescriptorFact,
    export_port_authority: DescriptorFact,
    import_port_authorities: Vec<CheckedPortAuthorityFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckedPortAuthorityFact {
    port_id: u32,
    authority: DescriptorFact,
}

pub fn render_runtime_authority_effect_binding(
    authority_effect_text: &str,
    artifact: &MantleArtifact,
    spawn_authority_policy: RuntimeSpawnAuthorityPolicy,
) -> Result<String> {
    let facts = admitted_authority_effect_facts(authority_effect_text)?;
    validate_against_runtime_artifact(&facts, artifact)?;
    render_binding_json(&facts, artifact, spawn_authority_policy)
}

fn validate_against_runtime_artifact(
    facts: &CheckedAuthorityEffectFacts<'_>,
    artifact: &MantleArtifact,
) -> Result<()> {
    if artifact.source_language.as_ref() != facts.source_language {
        return Err(Error::new(
            "runtime authority/effect binding source_language does not match artifact source_language",
        ));
    }
    if artifact.module != facts.source_module {
        return Err(Error::new(
            "runtime authority/effect binding source_module does not match artifact module",
        ));
    }
    if artifact.source_hash_fnv1a64 != facts.source_fingerprint {
        return Err(Error::new(
            "runtime authority/effect binding source fingerprint does not match artifact source hash",
        ));
    }
    if artifact.processes.len() != facts.processes.len() {
        return Err(Error::new(
            "runtime artifact process count does not match checked authority/effect facts",
        ));
    }
    for (process_id, (checked, runtime)) in
        facts.processes.iter().zip(&artifact.processes).enumerate()
    {
        validate_process_facts(process_id, checked, runtime)?;
    }
    validate_component_surfaces(facts, artifact)
}

fn validate_process_facts(
    process_id: usize,
    checked: &CheckedProcessFacts,
    runtime: &mantle_artifact::ArtifactProcess,
) -> Result<()> {
    if runtime.authorities.len() != checked.authorities.len() {
        return Err(Error::new(format!(
            "process_id {process_id} authority count does not match runtime artifact"
        )));
    }
    for (authority_id, (checked, runtime)) in checked
        .authorities
        .iter()
        .zip(&runtime.authorities)
        .enumerate()
    {
        if !descriptor_matches(*checked, runtime.descriptor) {
            return Err(Error::new(format!(
                "process_id {process_id} authority_id {authority_id} descriptor does not match runtime artifact"
            )));
        }
    }
    if runtime.spawn_sites.len() != checked.spawn_sites.len() {
        return Err(Error::new(format!(
            "process_id {process_id} spawn site count does not match runtime artifact"
        )));
    }
    for (spawn_site_id, (checked, runtime)) in checked
        .spawn_sites
        .iter()
        .zip(&runtime.spawn_sites)
        .enumerate()
    {
        if checked.kind.as_str() != artifact_spawn_kind_str(runtime.kind)
            || checked.target_process_id != runtime.target.as_u32()
            || checked.authority_id != runtime.authority.map(|id| id.as_u32())
            || checked.supervisor_id != runtime.supervisor.map(|id| id.as_u32())
            || checked.supervisor_child_id != runtime.child.map(|id| id.as_u32())
        {
            return Err(Error::new(format!(
                "process_id {process_id} spawn_site_id {spawn_site_id} does not match runtime artifact"
            )));
        }
    }
    if runtime.transitions.len() != checked.transitions.len() {
        return Err(Error::new(format!(
            "process_id {process_id} transition effect count does not match runtime artifact"
        )));
    }
    for (transition_id, (checked, runtime)) in checked
        .transitions
        .iter()
        .zip(&runtime.transitions)
        .enumerate()
    {
        if checked.message_id != runtime.message.as_u32()
            || checked.current_state_id != runtime.current_state.map(|id| id.as_u32())
            || !effect_lists_match(&checked.effects, &runtime.effects)
        {
            return Err(Error::new(format!(
                "process_id {process_id} transition_id {transition_id} effects do not match runtime artifact"
            )));
        }
    }
    Ok(())
}

fn validate_component_surfaces(
    facts: &CheckedAuthorityEffectFacts<'_>,
    artifact: &MantleArtifact,
) -> Result<()> {
    if facts.component_surfaces.len() != artifact.components.len() {
        return Err(Error::new(
            "runtime artifact component count does not match checked authority/effect facts",
        ));
    }
    for (component_id, (checked, runtime)) in facts
        .component_surfaces
        .iter()
        .zip(&artifact.components)
        .enumerate()
    {
        if checked.export_port_id != runtime.export_port.as_u32()
            || !descriptor_matches(checked.component_authority, runtime.required_authority)
        {
            return Err(Error::new(format!(
                "component_id {component_id} authority surface does not match runtime artifact"
            )));
        }
        let export_port = artifact
            .ports
            .get(runtime.export_port.index())
            .ok_or_else(|| Error::new("runtime artifact component export port is out of bounds"))?;
        if !descriptor_matches(
            checked.export_port_authority,
            export_port.required_authority,
        ) {
            return Err(Error::new(format!(
                "component_id {component_id} export port authority does not match runtime artifact"
            )));
        }
        if checked.import_port_authorities.len() != runtime.import_ports.len() {
            return Err(Error::new(format!(
                "component_id {component_id} import port authority count does not match runtime artifact"
            )));
        }
        for (index, (checked_port, runtime_port)) in checked
            .import_port_authorities
            .iter()
            .zip(&runtime.import_ports)
            .enumerate()
        {
            if checked_port.port_id != runtime_port.as_u32() {
                return Err(Error::new(format!(
                    "component_id {component_id} import port index {index} does not match runtime artifact"
                )));
            }
            let port = artifact.ports.get(runtime_port.index()).ok_or_else(|| {
                Error::new("runtime artifact component import port is out of bounds")
            })?;
            if !descriptor_matches(checked_port.authority, port.required_authority) {
                return Err(Error::new(format!(
                    "component_id {component_id} import port id {} authority does not match runtime artifact",
                    runtime_port.as_u32()
                )));
            }
        }
    }
    Ok(())
}

fn descriptor_matches(checked: DescriptorFact, runtime: ArtifactCapabilityDescriptor) -> bool {
    match (checked, runtime) {
        (
            DescriptorFact::Spawn { target_process_id },
            ArtifactCapabilityDescriptor::Spawn { target },
        ) => target_process_id == target.as_u32(),
        (
            DescriptorFact::ProtocolBoundary { protocol_id },
            ArtifactCapabilityDescriptor::ProtocolBoundary { protocol },
        ) => protocol_id == protocol.as_u32(),
        (
            DescriptorFact::PortConnect { port_id },
            ArtifactCapabilityDescriptor::PortConnect { port },
        ) => port_id == port.as_u32(),
        (
            DescriptorFact::ComponentExport { component_id },
            ArtifactCapabilityDescriptor::ComponentExport { component },
        ) => component_id == component.as_u32(),
        _ => false,
    }
}

fn effect_lists_match(checked: &[EffectFact], runtime: &[ArtifactEffect]) -> bool {
    checked.len() == runtime.len()
        && checked
            .iter()
            .copied()
            .zip(runtime.iter().copied())
            .all(|(checked, runtime)| checked.as_str() == artifact_effect_str(runtime))
}

impl EffectFact {
    fn as_str(self) -> &'static str {
        match self {
            Self::Emit => "emit",
            Self::Spawn => "spawn",
            Self::Send => "send",
        }
    }
}

impl SpawnKindFact {
    fn as_str(self) -> &'static str {
        match self {
            Self::DynamicLocal => "dynamic_local",
            Self::LexicalSupervisorChild => "lexical_supervisor_child",
        }
    }
}
