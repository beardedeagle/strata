use mantle_artifact::{
    ArtifactCapabilityDescriptor, ArtifactEffect, Error, MantleArtifact, Result,
};

use crate::limits::SpawnAuthorityPolicy;

use super::json::{JsonArray, JsonObject};

const PROCESS_FIELDS: &[&str] = &[
    "process_id",
    "authorities",
    "spawn_sites",
    "transition_effects",
];
const AUTHORITY_FIELDS: &[&str] = &["authority_id", "descriptor"];
const SPAWN_SITE_FIELDS: &[&str] = &[
    "spawn_site_id",
    "kind",
    "target_process_id",
    "authority_id",
    "supervisor_id",
    "supervisor_child_id",
];
const TRANSITION_FIELDS: &[&str] = &["transition_id", "message_id", "current_state_id", "effects"];
const EFFECT_FIELDS: &[&str] = &["effect_id", "effect"];
const COMPONENT_SURFACE_FIELDS: &[&str] = &[
    "component_id",
    "export_port_id",
    "component_authority",
    "export_port_authority",
    "import_port_authorities",
];
const IMPORT_PORT_FIELDS: &[&str] = &["port_id", "port_authority"];
const POLICY_FIELDS: &[&str] = &["spawn_authority_policy"];
const DESCRIPTOR_SPAWN_FIELDS: &[&str] = &["kind", "target_process_id"];
const DESCRIPTOR_PROTOCOL_FIELDS: &[&str] = &["kind", "protocol_id"];
const DESCRIPTOR_PORT_FIELDS: &[&str] = &["kind", "port_id"];
const DESCRIPTOR_COMPONENT_FIELDS: &[&str] = &["kind", "component_id"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorFact {
    Spawn { target_process_id: u32 },
    ProtocolBoundary { protocol_id: u32 },
    PortConnect { port_id: u32 },
    ComponentExport { component_id: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectFact {
    Emit,
    Spawn,
    Send,
}

pub(super) fn validate_processes(
    processes: &JsonArray<'_>,
    artifact: &MantleArtifact,
) -> Result<()> {
    let mut count = 0usize;
    processes.for_each_object(|index, process| {
        process.require_exact_fields(PROCESS_FIELDS)?;
        let process_id = process.required_u32("process_id")?;
        if usize::try_from(process_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "process_id {process_id} at array index {index} is not canonical"
            )));
        }
        let runtime_process = artifact.processes.get(index).ok_or_else(|| {
            Error::new(format!(
                "runtime authority/effect binding process_id {process_id} is out of bounds"
            ))
        })?;
        validate_authorities(
            &process.required_array("authorities")?,
            runtime_process,
            index,
        )?;
        validate_spawn_sites(
            &process.required_array("spawn_sites")?,
            runtime_process,
            index,
        )?;
        validate_transition_effects(
            &process.required_array("transition_effects")?,
            runtime_process,
            index,
        )?;
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("process count overflowed"))?;
        Ok(())
    })?;
    if count != artifact.processes.len() {
        return Err(Error::new(
            "runtime authority/effect binding process count does not match runtime artifact",
        ));
    }
    Ok(())
}

fn validate_authorities(
    authorities: &JsonArray<'_>,
    runtime_process: &mantle_artifact::ArtifactProcess,
    process_id: usize,
) -> Result<()> {
    let mut count = 0usize;
    authorities.for_each_object(|index, authority| {
        authority.require_exact_fields(AUTHORITY_FIELDS)?;
        let authority_id = authority.required_u32("authority_id")?;
        if usize::try_from(authority_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "authority_id {authority_id} at array index {index} is not canonical"
            )));
        }
        let runtime_authority = runtime_process.authorities.get(index).ok_or_else(|| {
            Error::new(format!(
                "process_id {process_id} authority_id {authority_id} is out of bounds"
            ))
        })?;
        if !descriptor_matches(
            descriptor_fact(&authority.required_object("descriptor")?)?,
            runtime_authority.descriptor,
        ) {
            return Err(Error::new(format!(
                "process_id {process_id} authority_id {authority_id} descriptor does not match runtime artifact"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("authority count overflowed"))?;
        Ok(())
    })?;
    if count != runtime_process.authorities.len() {
        return Err(Error::new(format!(
            "process_id {process_id} authority count does not match runtime artifact"
        )));
    }
    Ok(())
}

fn validate_spawn_sites(
    sites: &JsonArray<'_>,
    runtime_process: &mantle_artifact::ArtifactProcess,
    process_id: usize,
) -> Result<()> {
    let mut count = 0usize;
    sites.for_each_object(|index, site| {
        site.require_exact_fields(SPAWN_SITE_FIELDS)?;
        let spawn_site_id = site.required_u32("spawn_site_id")?;
        if usize::try_from(spawn_site_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "spawn_site_id {spawn_site_id} at array index {index} is not canonical"
            )));
        }
        let runtime_site = runtime_process.spawn_sites.get(index).ok_or_else(|| {
            Error::new(format!(
                "process_id {process_id} spawn_site_id {spawn_site_id} is out of bounds"
            ))
        })?;
        if site.required_string("kind")? != spawn_kind_str(runtime_site.kind)
            || site.required_u32("target_process_id")? != runtime_site.target.as_u32()
            || site.required_optional_u32("authority_id")?
                != runtime_site.authority.map(|id| id.as_u32())
            || site.required_optional_u32("supervisor_id")?
                != runtime_site.supervisor.map(|id| id.as_u32())
            || site.required_optional_u32("supervisor_child_id")?
                != runtime_site.child.map(|id| id.as_u32())
        {
            return Err(Error::new(format!(
                "process_id {process_id} spawn_site_id {spawn_site_id} does not match runtime artifact"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("spawn site count overflowed"))?;
        Ok(())
    })?;
    if count != runtime_process.spawn_sites.len() {
        return Err(Error::new(format!(
            "process_id {process_id} spawn site count does not match runtime artifact"
        )));
    }
    Ok(())
}

fn validate_transition_effects(
    transitions: &JsonArray<'_>,
    runtime_process: &mantle_artifact::ArtifactProcess,
    process_id: usize,
) -> Result<()> {
    let mut count = 0usize;
    transitions.for_each_object(|index, transition| {
        transition.require_exact_fields(TRANSITION_FIELDS)?;
        let transition_id = transition.required_u32("transition_id")?;
        if usize::try_from(transition_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "transition_id {transition_id} at array index {index} is not canonical"
            )));
        }
        let runtime_transition = runtime_process.transitions.get(index).ok_or_else(|| {
            Error::new(format!(
                "process_id {process_id} transition_id {transition_id} is out of bounds"
            ))
        })?;
        if transition.required_u32("message_id")? != runtime_transition.message.as_u32()
            || transition.required_optional_u32("current_state_id")?
                != runtime_transition.current_state.map(|id| id.as_u32())
            || !effect_array_matches(
                &transition.required_array("effects")?,
                &runtime_transition.effects,
            )?
        {
            return Err(Error::new(format!(
                "process_id {process_id} transition_id {transition_id} effects do not match runtime artifact"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("transition effect count overflowed"))?;
        Ok(())
    })?;
    if count != runtime_process.transitions.len() {
        return Err(Error::new(format!(
            "process_id {process_id} transition effect count does not match runtime artifact"
        )));
    }
    Ok(())
}

fn effect_array_matches(effects: &JsonArray<'_>, runtime: &[ArtifactEffect]) -> Result<bool> {
    let mut count = 0usize;
    let mut matches_runtime = true;
    effects.for_each_object(|index, effect| {
        effect.require_exact_fields(EFFECT_FIELDS)?;
        let effect_id = effect.required_u32("effect_id")?;
        if usize::try_from(effect_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "effect_id {effect_id} at array index {index} is not canonical"
            )));
        }
        let runtime_effect = runtime.get(index).ok_or_else(|| {
            Error::new(format!(
                "effect_id {effect_id} is out of bounds for runtime artifact"
            ))
        })?;
        let effect_fact = effect_fact(effect.required_string("effect")?)?;
        if effect_fact.as_str() != effect_str(*runtime_effect) {
            matches_runtime = false;
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("effect count overflowed"))?;
        Ok(())
    })?;
    Ok(matches_runtime && count == runtime.len())
}

pub(super) fn validate_component_surfaces(
    surfaces: &JsonArray<'_>,
    artifact: &MantleArtifact,
) -> Result<()> {
    let mut count = 0usize;
    surfaces.for_each_object(|index, surface| {
        surface.require_exact_fields(COMPONENT_SURFACE_FIELDS)?;
        let component_id = surface.required_u32("component_id")?;
        if usize::try_from(component_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "component_id {component_id} at array index {index} is not canonical"
            )));
        }
        let component = artifact.components.get(index).ok_or_else(|| {
            Error::new(format!(
                "component_id {component_id} is out of bounds for runtime artifact"
            ))
        })?;
        if surface.required_u32("export_port_id")? != component.export_port.as_u32()
            || !descriptor_matches(
                descriptor_fact(&surface.required_object("component_authority")?)?,
                component.required_authority,
            )
        {
            return Err(Error::new(format!(
                "component_id {component_id} authority surface does not match runtime artifact"
            )));
        }
        let export_port = artifact
            .ports
            .get(component.export_port.index())
            .ok_or_else(|| Error::new("component export port is out of bounds"))?;
        if !descriptor_matches(
            descriptor_fact(&surface.required_object("export_port_authority")?)?,
            export_port.required_authority,
        ) {
            return Err(Error::new(format!(
                "component_id {component_id} export port authority does not match runtime artifact"
            )));
        }
        validate_import_ports(
            &surface.required_array("import_port_authorities")?,
            component,
            artifact,
            component_id,
        )?;
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("component surface count overflowed"))?;
        Ok(())
    })?;
    if count != artifact.components.len() {
        return Err(Error::new(
            "runtime authority/effect binding component surface count does not match runtime artifact",
        ));
    }
    Ok(())
}

fn validate_import_ports(
    ports: &JsonArray<'_>,
    component: &mantle_artifact::ArtifactComponent,
    artifact: &MantleArtifact,
    component_id: u32,
) -> Result<()> {
    let mut count = 0usize;
    ports.for_each_object(|index, port| {
        port.require_exact_fields(IMPORT_PORT_FIELDS)?;
        let port_id = port.required_u32("port_id")?;
        let runtime_port_id = component.import_ports.get(index).ok_or_else(|| {
            Error::new(format!(
                "component_id {component_id} import port index {index} is out of bounds"
            ))
        })?;
        if port_id != runtime_port_id.as_u32() {
            return Err(Error::new(format!(
                "component_id {component_id} import port index {index} does not match runtime artifact"
            )));
        }
        let runtime_port = artifact
            .ports
            .get(runtime_port_id.index())
            .ok_or_else(|| Error::new("component import port is out of bounds"))?;
        if !descriptor_matches(
            descriptor_fact(&port.required_object("port_authority")?)?,
            runtime_port.required_authority,
        ) {
            return Err(Error::new(format!(
                "component_id {component_id} import port id {port_id} authority does not match runtime artifact"
            )));
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| Error::new("import port authority count overflowed"))?;
        Ok(())
    })?;
    if count != component.import_ports.len() {
        return Err(Error::new(format!(
            "component_id {component_id} import port authority count does not match runtime artifact"
        )));
    }
    Ok(())
}

pub(super) fn validate_policy(policy: &JsonObject<'_>) -> Result<SpawnAuthorityPolicy> {
    policy.require_exact_fields(POLICY_FIELDS)?;
    match policy.required_string("spawn_authority_policy")? {
        "admit_declared" => Ok(SpawnAuthorityPolicy::AdmitDeclared),
        "deny_declared" => Ok(SpawnAuthorityPolicy::DenyDeclared),
        other => Err(Error::new(format!(
            "unsupported spawn_authority_policy {other:?}"
        ))),
    }
}

fn descriptor_fact(descriptor: &JsonObject<'_>) -> Result<DescriptorFact> {
    match descriptor.required_string("kind")? {
        "spawn" => {
            descriptor.require_exact_fields(DESCRIPTOR_SPAWN_FIELDS)?;
            Ok(DescriptorFact::Spawn {
                target_process_id: descriptor.required_u32("target_process_id")?,
            })
        }
        "protocol_boundary" => {
            descriptor.require_exact_fields(DESCRIPTOR_PROTOCOL_FIELDS)?;
            Ok(DescriptorFact::ProtocolBoundary {
                protocol_id: descriptor.required_u32("protocol_id")?,
            })
        }
        "port_connect" => {
            descriptor.require_exact_fields(DESCRIPTOR_PORT_FIELDS)?;
            Ok(DescriptorFact::PortConnect {
                port_id: descriptor.required_u32("port_id")?,
            })
        }
        "component_export" => {
            descriptor.require_exact_fields(DESCRIPTOR_COMPONENT_FIELDS)?;
            Ok(DescriptorFact::ComponentExport {
                component_id: descriptor.required_u32("component_id")?,
            })
        }
        other => Err(Error::new(format!(
            "unsupported authority descriptor kind {other:?}"
        ))),
    }
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

fn effect_fact(value: &str) -> Result<EffectFact> {
    match value {
        "emit" => Ok(EffectFact::Emit),
        "spawn" => Ok(EffectFact::Spawn),
        "send" => Ok(EffectFact::Send),
        other => Err(Error::new(format!("unsupported effect {other:?}"))),
    }
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

fn effect_str(effect: ArtifactEffect) -> &'static str {
    match effect {
        ArtifactEffect::Emit => "emit",
        ArtifactEffect::Spawn => "spawn",
        ArtifactEffect::Send => "send",
    }
}

fn spawn_kind_str(kind: mantle_artifact::ArtifactSpawnKind) -> &'static str {
    match kind {
        mantle_artifact::ArtifactSpawnKind::DynamicLocal => "dynamic_local",
        mantle_artifact::ArtifactSpawnKind::LexicalSupervisorChild => "lexical_supervisor_child",
    }
}
