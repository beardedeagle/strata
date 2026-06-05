use mantle_artifact::{
    MAX_AUTHORITIES_PER_PROCESS, MAX_COMPONENT_COUNT, MAX_EFFECTS_PER_TRANSITION,
    MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PORT_COUNT, MAX_PROCESS_COUNT, MAX_PROTOCOL_COUNT,
    MAX_SPAWN_SITES_PER_PROCESS, MAX_STATE_VALUES_PER_PROCESS,
    MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR, MAX_SUPERVISORS_PER_PROCESS,
    MAX_TRANSITIONS_PER_PROCESS,
};

use super::super::composition_artifact::codec::{JsonArray, JsonObject};
use super::super::diagnostic::{Error, Result};
use super::{
    ADMISSION_RESULT_ADMITTED, ARTIFACT_KIND, AUTHORITY_EFFECT_HASH_ALG,
    AUTHORITY_EFFECT_SCHEMA_ID, AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
    AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR, AuthorityEffectAdmissionResult,
    AuthorityEffectAdmissionSummary, SOURCE_FINGERPRINT_ALGORITHM, SOURCE_LANGUAGE,
};

mod validation;

use validation::{
    require_schema_version_eq, validate_count, validate_count_field, validate_empty_array,
    validate_exact_count, validate_existing_id, validate_existing_raw_id, validate_indexed_id,
    validate_metadata_string, validate_source_fingerprint, validate_spawn_kind,
};

const TOP_LEVEL_FIELDS: &[&str] = &[
    "schema_id",
    "schema_version_major",
    "schema_version_minor",
    "artifact_kind",
    "hash_alg",
    "source_language",
    "source_module",
    "source_path",
    "source_fingerprint",
    "source_fingerprint_algorithm",
    "protocol_count",
    "port_count",
    "component_count",
    "processes",
    "component_authority_surfaces",
    "policy_inputs",
    "admission_policy_hash",
    "admission_result",
    "diagnostic_set_hash",
    "extensions",
];
const PROCESS_FIELDS: &[&str] = &[
    "process_id",
    "process",
    "state_count",
    "message_count",
    "authorities",
    "spawn_sites",
    "supervisor_spawn_facts",
    "transition_effects",
];
const AUTHORITY_FIELDS: &[&str] = &["authority_id", "name", "descriptor"];
const SPAWN_SITE_FIELDS: &[&str] = &[
    "spawn_site_id",
    "kind",
    "target_process_id",
    "target_process",
    "authority_id",
    "supervisor_id",
    "supervisor_child_id",
];
const SUPERVISOR_SPAWN_FACT_FIELDS: &[&str] = &["supervisor_id", "children"];
const SUPERVISOR_CHILD_SPAWN_FACT_FIELDS: &[&str] = &[
    "child_id",
    "child",
    "target_process_id",
    "target_process",
    "spawn_site_id",
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
const FNV1A64_FINGERPRINT_HEX_LEN: usize = 16;

#[derive(Debug, Clone, Copy)]
struct ProcessCounts {
    authorities: usize,
    spawn_sites: usize,
    transition_effects: usize,
}

#[derive(Debug, Clone, Copy)]
struct TableCounts {
    protocols: usize,
    ports: usize,
    processes: usize,
    components: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpawnKindFact {
    DynamicLocal,
    LexicalSupervisorChild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DescriptorFact {
    Spawn { target_process_id: u32 },
    ProtocolBoundary { protocol_id: u32 },
    PortConnect { port_id: u32 },
    ComponentExport { component_id: u32 },
}

#[derive(Debug, Clone, Copy)]
struct SpawnSiteFact {
    kind: SpawnKindFact,
    target_process_id: u32,
    authority: Option<u32>,
    supervisor: Option<u32>,
    child: Option<u32>,
}

#[derive(Debug, Clone)]
struct SupervisorSpawnFact {
    children: Vec<SupervisorChildSpawnFact>,
}

#[derive(Debug, Clone, Copy)]
struct SupervisorChildSpawnFact {
    target_process_id: u32,
    spawn_site_id: u32,
}

pub(super) fn validate_authority_effect_artifact(
    text: &str,
) -> Result<AuthorityEffectAdmissionSummary> {
    let artifact = JsonObject::new(text, "authority/effect artifact")?;
    artifact.require_exact_fields(TOP_LEVEL_FIELDS)?;
    artifact.required_string_eq("schema_id", AUTHORITY_EFFECT_SCHEMA_ID)?;
    require_schema_version_eq(
        &artifact,
        "schema_version_major",
        AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
    )?;
    require_schema_version_eq(
        &artifact,
        "schema_version_minor",
        AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
    )?;
    artifact.required_string_eq("artifact_kind", ARTIFACT_KIND)?;
    artifact.required_string_eq("hash_alg", AUTHORITY_EFFECT_HASH_ALG)?;
    artifact.required_string_eq("source_language", SOURCE_LANGUAGE)?;
    validate_metadata_string(&artifact, "source_module")?;
    validate_metadata_string(&artifact, "source_path")?;
    validate_source_fingerprint(&artifact)?;
    artifact.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;

    let protocol_count = validate_count_field(&artifact, "protocol_count", 0, MAX_PROTOCOL_COUNT)?;
    let port_count = validate_count_field(&artifact, "port_count", 0, MAX_PORT_COUNT)?;
    let component_count =
        validate_count_field(&artifact, "component_count", 0, MAX_COMPONENT_COUNT)?;
    let process_count = artifact.required_array("processes")?.count_values()?;
    validate_count("process_count", process_count, 1, MAX_PROCESS_COUNT)?;
    let table_counts = TableCounts {
        protocols: protocol_count,
        ports: port_count,
        processes: process_count,
        components: component_count,
    };

    let process_counts = validate_processes(&artifact.required_array("processes")?, table_counts)?;
    let component_surface_count = validate_component_surfaces(
        &artifact.required_array("component_authority_surfaces")?,
        table_counts,
    )?;
    validate_empty_array(&artifact, "policy_inputs")?;
    artifact.required_null("admission_policy_hash")?;
    artifact.required_string_eq("admission_result", ADMISSION_RESULT_ADMITTED)?;
    artifact.required_null("diagnostic_set_hash")?;
    artifact.required_empty_object("extensions")?;

    Ok(AuthorityEffectAdmissionSummary {
        schema_id: AUTHORITY_EFFECT_SCHEMA_ID,
        schema_version_major: AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
        schema_version_minor: AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
        hash_alg: AUTHORITY_EFFECT_HASH_ALG,
        protocol_count,
        port_count,
        process_count: process_counts.len(),
        component_count,
        authority_count: process_counts.iter().map(|counts| counts.authorities).sum(),
        spawn_site_count: process_counts.iter().map(|counts| counts.spawn_sites).sum(),
        transition_effect_count: process_counts
            .iter()
            .map(|counts| counts.transition_effects)
            .sum(),
        component_authority_surface_count: component_surface_count,
        admission_result: AuthorityEffectAdmissionResult::Admitted,
    })
}

fn validate_processes(
    processes: &JsonArray<'_>,
    table_counts: TableCounts,
) -> Result<Vec<ProcessCounts>> {
    let count = processes.count_values()?;
    validate_exact_count("process_count", count, table_counts.processes)?;
    let mut counts = Vec::with_capacity(count);
    processes.for_each_object(|index, process| {
        process.require_exact_fields(PROCESS_FIELDS)?;
        validate_indexed_id(&process, "process_id", index)?;
        validate_metadata_string(&process, "process")?;
        let state_count =
            validate_count_field(&process, "state_count", 1, MAX_STATE_VALUES_PER_PROCESS)?;
        let message_count = validate_count_field(
            &process,
            "message_count",
            1,
            MAX_MESSAGE_VARIANTS_PER_PROCESS,
        )?;
        let authority_descriptors =
            validate_authorities(&process.required_array("authorities")?, table_counts)?;
        let supervisor_spawn_facts = validate_supervisor_spawn_facts(
            &process.required_array("supervisor_spawn_facts")?,
            count,
        )?;
        let spawn_sites = validate_spawn_sites(
            &process.required_array("spawn_sites")?,
            count,
            &authority_descriptors,
            &supervisor_spawn_facts,
        )?;
        validate_supervisor_spawn_site_backlinks(&supervisor_spawn_facts, &spawn_sites)?;
        let transition_effects = validate_transition_effects(
            &process.required_array("transition_effects")?,
            state_count,
            message_count,
        )?;
        counts.push(ProcessCounts {
            authorities: authority_descriptors.len(),
            spawn_sites: spawn_sites.len(),
            transition_effects,
        });
        Ok(())
    })?;
    Ok(counts)
}

fn validate_authorities(
    authorities: &JsonArray<'_>,
    table_counts: TableCounts,
) -> Result<Vec<DescriptorFact>> {
    let count = authorities.count_values()?;
    validate_count("authority_count", count, 0, MAX_AUTHORITIES_PER_PROCESS)?;
    let mut descriptors = Vec::with_capacity(count);
    authorities.for_each_object(|index, authority| {
        authority.require_exact_fields(AUTHORITY_FIELDS)?;
        validate_indexed_id(&authority, "authority_id", index)?;
        validate_metadata_string(&authority, "name")?;
        descriptors.push(validate_descriptor(
            &authority.required_object("descriptor")?,
            table_counts,
        )?);
        Ok(())
    })?;
    Ok(descriptors)
}

fn validate_supervisor_spawn_facts(
    supervisors: &JsonArray<'_>,
    process_count: usize,
) -> Result<Vec<SupervisorSpawnFact>> {
    let count = supervisors.count_values()?;
    validate_count(
        "supervisor_spawn_fact_count",
        count,
        0,
        MAX_SUPERVISORS_PER_PROCESS,
    )?;
    let mut facts = Vec::with_capacity(count);
    supervisors.for_each_object(|index, supervisor| {
        supervisor.require_exact_fields(SUPERVISOR_SPAWN_FACT_FIELDS)?;
        validate_indexed_id(&supervisor, "supervisor_id", index)?;
        facts.push(SupervisorSpawnFact {
            children: validate_supervisor_child_spawn_facts(
                &supervisor.required_array("children")?,
                process_count,
            )?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn validate_supervisor_child_spawn_facts(
    children: &JsonArray<'_>,
    process_count: usize,
) -> Result<Vec<SupervisorChildSpawnFact>> {
    let count = children.count_values()?;
    validate_count(
        "supervisor_child_spawn_fact_count",
        count,
        1,
        MAX_SUPERVISOR_CHILDREN_PER_SUPERVISOR,
    )?;
    let mut facts = Vec::with_capacity(count);
    children.for_each_object(|index, child| {
        child.require_exact_fields(SUPERVISOR_CHILD_SPAWN_FACT_FIELDS)?;
        validate_indexed_id(&child, "child_id", index)?;
        validate_metadata_string(&child, "child")?;
        let target_process_id =
            validate_existing_id(&child, "target_process_id", process_count, "process")?;
        validate_metadata_string(&child, "target_process")?;
        facts.push(SupervisorChildSpawnFact {
            target_process_id,
            spawn_site_id: child.required_u32("spawn_site_id")?,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn validate_spawn_sites(
    sites: &JsonArray<'_>,
    process_count: usize,
    authority_descriptors: &[DescriptorFact],
    supervisor_spawn_facts: &[SupervisorSpawnFact],
) -> Result<Vec<SpawnSiteFact>> {
    let count = sites.count_values()?;
    validate_count("spawn_site_count", count, 0, MAX_SPAWN_SITES_PER_PROCESS)?;
    let mut facts = Vec::with_capacity(count);
    sites.for_each_object(|index, site| {
        site.require_exact_fields(SPAWN_SITE_FIELDS)?;
        validate_indexed_id(&site, "spawn_site_id", index)?;
        let kind = validate_spawn_kind(&site, "kind")?;
        let target_process_id =
            validate_existing_id(&site, "target_process_id", process_count, "process")?;
        validate_metadata_string(&site, "target_process")?;
        let authority = site.required_optional_u32("authority_id")?;
        let supervisor = site.required_optional_u32("supervisor_id")?;
        let child = site.required_optional_u32("supervisor_child_id")?;
        let fact = SpawnSiteFact {
            kind,
            target_process_id,
            authority,
            supervisor,
            child,
        };
        validate_spawn_site_shape(index, &fact, authority_descriptors, supervisor_spawn_facts)?;
        facts.push(fact);
        Ok(())
    })?;
    Ok(facts)
}

fn validate_spawn_site_shape(
    index: usize,
    site: &SpawnSiteFact,
    authority_descriptors: &[DescriptorFact],
    supervisor_spawn_facts: &[SupervisorSpawnFact],
) -> Result<()> {
    match site.kind {
        SpawnKindFact::DynamicLocal => {
            if site.supervisor.is_some() || site.child.is_some() {
                return Err(Error::new(format!(
                    "dynamic_local spawn_site id {index} must not carry supervisor ids"
                )));
            }
            let authority_id = site.authority.ok_or_else(|| {
                Error::new(format!(
                    "dynamic_local spawn_site id {index} must carry authority_id"
                ))
            })?;
            validate_existing_raw_id(authority_id, authority_descriptors.len(), "authority")?;
            let authority_index = usize::try_from(authority_id)
                .map_err(|_| Error::new("authority id exceeds supported usize range"))?;
            match authority_descriptors.get(authority_index) {
                Some(DescriptorFact::Spawn {
                    target_process_id: authority_target,
                }) if *authority_target == site.target_process_id => Ok(()),
                Some(DescriptorFact::Spawn {
                    target_process_id: authority_target,
                }) => Err(Error::new(format!(
                    "dynamic_local spawn_site id {index} targets process id {}, but authority_id {authority_id} targets {authority_target}",
                    site.target_process_id
                ))),
                Some(_) => Err(Error::new(format!(
                    "dynamic_local spawn_site id {index} authority_id {authority_id} is not a spawn capability"
                ))),
                None => Err(Error::new(format!(
                    "references unknown authority id {authority_id}"
                ))),
            }
        }
        SpawnKindFact::LexicalSupervisorChild => {
            if site.authority.is_some() {
                return Err(Error::new(format!(
                    "lexical_supervisor_child spawn_site id {index} must not carry authority_id"
                )));
            }
            let supervisor_id = site.supervisor.ok_or_else(|| {
                Error::new(format!(
                    "lexical_supervisor_child spawn_site id {index} must carry supervisor_id"
                ))
            })?;
            let child_id = site.child.ok_or_else(|| {
                Error::new(format!(
                    "lexical_supervisor_child spawn_site id {index} must carry supervisor_child_id"
                ))
            })?;
            let child_fact =
                supervisor_child_spawn_fact(supervisor_spawn_facts, supervisor_id, child_id)?;
            if child_fact.target_process_id != site.target_process_id {
                return Err(Error::new(format!(
                    "lexical_supervisor_child spawn_site id {index} targets process id {}, but supervisor child targets {}",
                    site.target_process_id, child_fact.target_process_id
                )));
            }
            if usize::try_from(child_fact.spawn_site_id).ok() != Some(index) {
                return Err(Error::new(format!(
                    "lexical_supervisor_child spawn_site id {index} is not the declared spawn_site_id {} for supervisor child",
                    child_fact.spawn_site_id
                )));
            }
            Ok(())
        }
    }
}

fn validate_supervisor_spawn_site_backlinks(
    supervisors: &[SupervisorSpawnFact],
    spawn_sites: &[SpawnSiteFact],
) -> Result<()> {
    for (supervisor_index, supervisor) in supervisors.iter().enumerate() {
        let supervisor_id = u32::try_from(supervisor_index)
            .map_err(|_| Error::new("supervisor id exceeds supported range"))?;
        for (child_index, child) in supervisor.children.iter().enumerate() {
            let child_id = u32::try_from(child_index)
                .map_err(|_| Error::new("supervisor child id exceeds supported range"))?;
            let spawn_site_index = usize::try_from(child.spawn_site_id).map_err(|_| {
                Error::new("supervisor child spawn_site_id exceeds supported range")
            })?;
            let spawn_site = spawn_sites.get(spawn_site_index).ok_or_else(|| {
                Error::new(format!(
                    "supervisor id {supervisor_index} child id {child_index} references unknown spawn_site_id {}",
                    child.spawn_site_id
                ))
            })?;
            if spawn_site.kind != SpawnKindFact::LexicalSupervisorChild
                || spawn_site.target_process_id != child.target_process_id
                || spawn_site.authority.is_some()
                || spawn_site.supervisor != Some(supervisor_id)
                || spawn_site.child != Some(child_id)
            {
                return Err(Error::new(format!(
                    "supervisor id {supervisor_index} child id {child_index} does not match spawn_site_id {}",
                    child.spawn_site_id
                )));
            }
        }
    }
    Ok(())
}

fn supervisor_child_spawn_fact(
    supervisors: &[SupervisorSpawnFact],
    supervisor_id: u32,
    child_id: u32,
) -> Result<SupervisorChildSpawnFact> {
    let supervisor_index = usize::try_from(supervisor_id)
        .map_err(|_| Error::new("supervisor id exceeds supported range"))?;
    let supervisor = supervisors
        .get(supervisor_index)
        .ok_or_else(|| Error::new(format!("references unknown supervisor id {supervisor_id}")))?;
    let child_index = usize::try_from(child_id)
        .map_err(|_| Error::new("supervisor child id exceeds supported range"))?;
    supervisor
        .children
        .get(child_index)
        .copied()
        .ok_or_else(|| Error::new(format!("references unknown supervisor child id {child_id}")))
}

fn validate_transition_effects(
    transitions: &JsonArray<'_>,
    state_count: usize,
    message_count: usize,
) -> Result<usize> {
    let count = transitions.count_values()?;
    validate_count(
        "transition_effect_count",
        count,
        0,
        MAX_TRANSITIONS_PER_PROCESS,
    )?;
    transitions.for_each_object(|index, transition| {
        transition.require_exact_fields(TRANSITION_FIELDS)?;
        validate_indexed_id(&transition, "transition_id", index)?;
        validate_existing_id(&transition, "message_id", message_count, "message")?;
        if let Some(state_id) = transition.required_optional_u32("current_state_id")? {
            validate_existing_raw_id(state_id, state_count, "state")?;
        }
        validate_effects(&transition.required_array("effects")?)
    })?;
    Ok(count)
}

fn validate_effects(effects: &JsonArray<'_>) -> Result<()> {
    let count = effects.count_values()?;
    validate_count("effect_count", count, 0, MAX_EFFECTS_PER_TRANSITION)?;
    let mut seen_emit = false;
    let mut seen_spawn = false;
    let mut seen_send = false;
    effects.for_each_object(|index, effect| {
        effect.require_exact_fields(EFFECT_FIELDS)?;
        validate_indexed_id(&effect, "effect_id", index)?;
        let effect_name = effect.required_string("effect")?;
        match effect_name.as_ref() {
            "emit" => {
                if std::mem::replace(&mut seen_emit, true) {
                    return Err(Error::new("transition declares duplicate effect"));
                }
                Ok(())
            }
            "spawn" => {
                if std::mem::replace(&mut seen_spawn, true) {
                    return Err(Error::new("transition declares duplicate effect"));
                }
                Ok(())
            }
            "send" => {
                if std::mem::replace(&mut seen_send, true) {
                    return Err(Error::new("transition declares duplicate effect"));
                }
                Ok(())
            }
            other => Err(Error::new(format!("unsupported effect {other:?}"))),
        }
    })
}

fn validate_component_surfaces(
    surfaces: &JsonArray<'_>,
    table_counts: TableCounts,
) -> Result<usize> {
    let count = surfaces.count_values()?;
    validate_exact_count(
        "component_authority_surface_count",
        count,
        table_counts.components,
    )?;
    surfaces.for_each_object(|index, surface| {
        surface.require_exact_fields(COMPONENT_SURFACE_FIELDS)?;
        validate_indexed_id(&surface, "component_id", index)?;
        validate_metadata_string(&surface, "component")?;
        let export_port =
            validate_existing_id(&surface, "export_port_id", table_counts.ports, "port")?;
        validate_metadata_string(&surface, "export_port")?;
        let import_port_count =
            validate_count_field(&surface, "import_port_count", 0, MAX_PORT_COUNT)?;
        let component_authority = validate_descriptor(
            &surface.required_object("component_authority")?,
            table_counts,
        )?;
        validate_descriptor_targets_component(component_authority, index)?;
        let export_authority = validate_descriptor(
            &surface.required_object("export_port_authority")?,
            table_counts,
        )?;
        validate_descriptor_targets_port(export_authority, export_port)?;
        validate_import_port_authorities(
            &surface.required_array("import_port_authorities")?,
            import_port_count,
            table_counts,
        )
    })?;
    Ok(count)
}

fn validate_import_port_authorities(
    ports: &JsonArray<'_>,
    expected_count: usize,
    table_counts: TableCounts,
) -> Result<()> {
    let count = ports.count_values()?;
    validate_exact_count("component_import_port_count", count, expected_count)?;
    let mut seen_port_ids = [false; MAX_PORT_COUNT];
    ports.for_each_object(|_, port| {
        port.require_exact_fields(IMPORT_PORT_FIELDS)?;
        let port_id = validate_existing_id(&port, "port_id", table_counts.ports, "port")?;
        let port_index = usize::try_from(port_id)
            .map_err(|_| Error::new("port id exceeds supported usize range"))?;
        let seen = seen_port_ids
            .get_mut(port_index)
            .ok_or_else(|| Error::new(format!("references unknown port id {port_id}")))?;
        if std::mem::replace(seen, true) {
            return Err(Error::new(format!(
                "component surface imports port id {port_id} more than once"
            )));
        }
        validate_metadata_string(&port, "port")?;
        let authority =
            validate_descriptor(&port.required_object("port_authority")?, table_counts)?;
        validate_descriptor_targets_port(authority, port_id)
    })
}

fn validate_descriptor(
    descriptor: &JsonObject<'_>,
    table_counts: TableCounts,
) -> Result<DescriptorFact> {
    let kind = descriptor.required_string("kind")?;
    match kind.as_ref() {
        "spawn" => {
            descriptor.require_exact_fields(DESCRIPTOR_SPAWN_FIELDS)?;
            let target_process_id = validate_existing_id(
                descriptor,
                "target_process_id",
                table_counts.processes,
                "process",
            )?;
            validate_metadata_string(descriptor, "target_process")?;
            Ok(DescriptorFact::Spawn { target_process_id })
        }
        "protocol_boundary" => {
            descriptor.require_exact_fields(DESCRIPTOR_PROTOCOL_FIELDS)?;
            let protocol_id = validate_existing_id(
                descriptor,
                "protocol_id",
                table_counts.protocols,
                "protocol",
            )?;
            validate_metadata_string(descriptor, "protocol")?;
            Ok(DescriptorFact::ProtocolBoundary { protocol_id })
        }
        "port_connect" => {
            descriptor.require_exact_fields(DESCRIPTOR_PORT_FIELDS)?;
            let port_id = validate_existing_id(descriptor, "port_id", table_counts.ports, "port")?;
            validate_metadata_string(descriptor, "port")?;
            Ok(DescriptorFact::PortConnect { port_id })
        }
        "component_export" => {
            descriptor.require_exact_fields(DESCRIPTOR_COMPONENT_FIELDS)?;
            let component_id = validate_existing_id(
                descriptor,
                "component_id",
                table_counts.components,
                "component",
            )?;
            validate_metadata_string(descriptor, "component")?;
            Ok(DescriptorFact::ComponentExport { component_id })
        }
        other => Err(Error::new(format!(
            "unsupported authority descriptor kind {other:?}"
        ))),
    }
}

fn validate_descriptor_targets_port(descriptor: DescriptorFact, port_id: u32) -> Result<()> {
    let DescriptorFact::PortConnect {
        port_id: descriptor_port_id,
    } = descriptor
    else {
        return Err(Error::new("port authority descriptor must be port_connect"));
    };
    if descriptor_port_id != port_id {
        return Err(Error::new(format!(
            "port authority targets port id {descriptor_port_id}, expected {port_id}"
        )));
    }
    Ok(())
}

fn validate_descriptor_targets_component(
    descriptor: DescriptorFact,
    component_id: usize,
) -> Result<()> {
    let DescriptorFact::ComponentExport {
        component_id: descriptor_component_id,
    } = descriptor
    else {
        return Err(Error::new(
            "component authority descriptor must be component_export",
        ));
    };
    let expected = u32::try_from(component_id)
        .map_err(|_| Error::new("component id exceeds supported u32 range"))?;
    if descriptor_component_id != expected {
        return Err(Error::new(format!(
            "component authority targets component id {descriptor_component_id}, expected {expected}"
        )));
    }
    Ok(())
}
