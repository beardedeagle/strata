use super::super::super::composition_artifact::codec::{JsonArray, JsonObject};
use super::super::super::diagnostic::{Error, Result};
use super::super::{
    AUTHORITY_EFFECT_SCHEMA_ID, AuthorityEffectAdmissionResult, SOURCE_FINGERPRINT_ALGORITHM,
    admit_authority_effect_artifact,
};
use super::{
    CheckedAuthorityEffectFacts, CheckedComponentSurfaceFact, CheckedPortAuthorityFact,
    CheckedProcessFacts, DescriptorFact, EffectFact, SpawnKindFact, SpawnSiteFact,
    TransitionEffectFact,
};

const SOURCE_PROCESS_FIELDS: &[&str] = &[
    "process_id",
    "process",
    "state_count",
    "message_count",
    "authorities",
    "spawn_sites",
    "supervisor_spawn_facts",
    "transition_effects",
];
const SOURCE_AUTHORITY_FIELDS: &[&str] = &["authority_id", "name", "descriptor"];
const SOURCE_SPAWN_SITE_FIELDS: &[&str] = &[
    "spawn_site_id",
    "kind",
    "target_process_id",
    "target_process",
    "authority_id",
    "supervisor_id",
    "supervisor_child_id",
];
const TRANSITION_FIELDS: &[&str] = &["transition_id", "message_id", "current_state_id", "effects"];
const EFFECT_FIELDS: &[&str] = &["effect_id", "effect"];
const COMPONENT_SURFACE_FIELDS: &[&str] = &[
    "component_id",
    "component",
    "export_port_id",
    "export_port",
    "import_port_count",
    "component_authority",
    "export_port_authority",
    "import_port_authorities",
];
const IMPORT_PORT_FIELDS: &[&str] = &["port_id", "port", "port_authority"];
const DESCRIPTOR_SPAWN_FIELDS: &[&str] = &["kind", "target_process_id", "target_process"];
const DESCRIPTOR_PROTOCOL_FIELDS: &[&str] = &["kind", "protocol_id", "protocol"];
const DESCRIPTOR_PORT_FIELDS: &[&str] = &["kind", "port_id", "port"];
const DESCRIPTOR_COMPONENT_FIELDS: &[&str] = &["kind", "component_id", "component"];

pub(super) fn admitted_authority_effect_facts(
    text: &str,
) -> Result<CheckedAuthorityEffectFacts<'_>> {
    let summary = admit_authority_effect_artifact(text)?;
    if summary.admission_result != AuthorityEffectAdmissionResult::Admitted {
        return Err(Error::new(
            "runtime authority/effect binding requires an admitted checked authority/effect artifact",
        ));
    }
    let artifact = JsonObject::new(text, "authority/effect artifact")?;
    artifact.required_string_eq("schema_id", AUTHORITY_EFFECT_SCHEMA_ID)?;
    artifact.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
    Ok(CheckedAuthorityEffectFacts {
        source_language: artifact.required_string("source_language")?,
        source_module: artifact.required_string("source_module")?,
        source_fingerprint: artifact.required_string("source_fingerprint")?,
        processes: process_facts(&artifact.required_array("processes")?)?,
        component_surfaces: component_surface_facts(
            &artifact.required_array("component_authority_surfaces")?,
        )?,
    })
}

fn process_facts(processes: &JsonArray<'_>) -> Result<Vec<CheckedProcessFacts>> {
    let mut facts = Vec::with_capacity(processes.count_values()?);
    processes.for_each_object(|index, process| {
        process.require_exact_fields(SOURCE_PROCESS_FIELDS)?;
        validate_indexed_id(&process, "process_id", index)?;
        facts.push(CheckedProcessFacts {
            authorities: authority_facts(&process.required_array("authorities")?)?,
            spawn_sites: spawn_site_facts(&process.required_array("spawn_sites")?)?,
            transitions: transition_facts(&process.required_array("transition_effects")?)?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn authority_facts(authorities: &JsonArray<'_>) -> Result<Vec<DescriptorFact>> {
    let mut facts = Vec::with_capacity(authorities.count_values()?);
    authorities.for_each_object(|index, authority| {
        authority.require_exact_fields(SOURCE_AUTHORITY_FIELDS)?;
        validate_indexed_id(&authority, "authority_id", index)?;
        facts.push(descriptor_fact(&authority.required_object("descriptor")?)?);
        Ok(())
    })?;
    Ok(facts)
}

fn spawn_site_facts(sites: &JsonArray<'_>) -> Result<Vec<SpawnSiteFact>> {
    let mut facts = Vec::with_capacity(sites.count_values()?);
    sites.for_each_object(|index, site| {
        site.require_exact_fields(SOURCE_SPAWN_SITE_FIELDS)?;
        validate_indexed_id(&site, "spawn_site_id", index)?;
        facts.push(SpawnSiteFact {
            kind: spawn_kind_fact(site.required_string("kind")?.as_ref())?,
            target_process_id: site.required_u32("target_process_id")?,
            authority_id: site.required_optional_u32("authority_id")?,
            supervisor_id: site.required_optional_u32("supervisor_id")?,
            supervisor_child_id: site.required_optional_u32("supervisor_child_id")?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn transition_facts(transitions: &JsonArray<'_>) -> Result<Vec<TransitionEffectFact>> {
    let mut facts = Vec::with_capacity(transitions.count_values()?);
    transitions.for_each_object(|index, transition| {
        transition.require_exact_fields(TRANSITION_FIELDS)?;
        validate_indexed_id(&transition, "transition_id", index)?;
        facts.push(TransitionEffectFact {
            message_id: transition.required_u32("message_id")?,
            current_state_id: transition.required_optional_u32("current_state_id")?,
            effects: effect_facts(&transition.required_array("effects")?)?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn effect_facts(effects: &JsonArray<'_>) -> Result<Vec<EffectFact>> {
    let mut facts = Vec::with_capacity(effects.count_values()?);
    effects.for_each_object(|index, effect| {
        effect.require_exact_fields(EFFECT_FIELDS)?;
        validate_indexed_id(&effect, "effect_id", index)?;
        let effect = match effect.required_string("effect")?.as_ref() {
            "emit" => EffectFact::Emit,
            "spawn" => EffectFact::Spawn,
            "send" => EffectFact::Send,
            other => return Err(Error::new(format!("unsupported effect {other:?}"))),
        };
        facts.push(effect);
        Ok(())
    })?;
    Ok(facts)
}

fn spawn_kind_fact(kind: &str) -> Result<SpawnKindFact> {
    match kind {
        "dynamic_local" => Ok(SpawnKindFact::DynamicLocal),
        "lexical_supervisor_child" => Ok(SpawnKindFact::LexicalSupervisorChild),
        other => Err(Error::new(format!("unsupported spawn site kind {other:?}"))),
    }
}

fn component_surface_facts(surfaces: &JsonArray<'_>) -> Result<Vec<CheckedComponentSurfaceFact>> {
    let mut facts = Vec::with_capacity(surfaces.count_values()?);
    surfaces.for_each_object(|index, surface| {
        surface.require_exact_fields(COMPONENT_SURFACE_FIELDS)?;
        validate_indexed_id(&surface, "component_id", index)?;
        facts.push(CheckedComponentSurfaceFact {
            export_port_id: surface.required_u32("export_port_id")?,
            component_authority: descriptor_fact(&surface.required_object("component_authority")?)?,
            export_port_authority: descriptor_fact(
                &surface.required_object("export_port_authority")?,
            )?,
            import_port_authorities: import_port_authority_facts(
                &surface.required_array("import_port_authorities")?,
            )?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn import_port_authority_facts(ports: &JsonArray<'_>) -> Result<Vec<CheckedPortAuthorityFact>> {
    let mut facts = Vec::with_capacity(ports.count_values()?);
    ports.for_each_object(|_, port| {
        port.require_exact_fields(IMPORT_PORT_FIELDS)?;
        facts.push(CheckedPortAuthorityFact {
            port_id: port.required_u32("port_id")?,
            authority: descriptor_fact(&port.required_object("port_authority")?)?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn descriptor_fact(descriptor: &JsonObject<'_>) -> Result<DescriptorFact> {
    match descriptor.required_string("kind")?.as_ref() {
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

fn validate_indexed_id(object: &JsonObject<'_>, field: &str, expected_index: usize) -> Result<()> {
    let actual = object.required_u32(field)?;
    if usize::try_from(actual).ok() == Some(expected_index) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} {actual} at array index {expected_index} is not canonical"
        )))
    }
}
