use mantle_artifact::{
    MAX_COMPONENT_COUNT, MAX_COMPONENT_INSTANCE_COUNT, MAX_FIELD_VALUE_BYTES,
    MAX_PORT_BINDING_COUNT, MAX_PORT_COUNT, MAX_PROTOCOL_COUNT,
};

use super::super::diagnostic::{Error, Result};
use super::{
    ADMISSION_RESULT_ADMITTED, ADMISSION_RESULT_REJECTED, ARTIFACT_KIND,
    COMPONENT_COMPOSITION_HASH_ALG, COMPONENT_COMPOSITION_SCHEMA_ID,
    COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR, COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR,
    SOURCE_FINGERPRINT_ALGORITHM, SOURCE_LANGUAGE,
};
use super::{ComponentCompositionAdmissionResult, ComponentCompositionAdmissionSummary};
use crate::language::composition_artifact::codec::{JsonArray, JsonObject};

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
    "composition_id",
    "composition_name",
    "components",
    "capability_bindings",
    "interface_bindings",
    "port_bindings",
    "runtime_feature_bindings",
    "archive_format_bindings",
    "crypto_policy_bindings",
    "cross_component_authority_edges",
    "unsatisfied_imports",
    "admission_policy_hash",
    "admission_result",
    "diagnostic_set_hash",
    "extensions",
];
const INSTANCE_FIELDS: &[&str] = &[
    "component_instance_id",
    "instance",
    "component_id",
    "component",
    "component_authority",
    "import_ports",
    "export_port",
];
const COMPONENT_PORT_FIELDS: &[&str] = &[
    "port_id",
    "port",
    "protocol_id",
    "protocol",
    "required_authority",
];
const PORT_BINDING_FIELDS: &[&str] = &[
    "port_binding_id",
    "importer_instance_id",
    "importer_instance",
    "imported_port_id",
    "imported_port",
    "exporter_instance_id",
    "exporter_instance",
    "exported_port_id",
    "exported_port",
    "protocol_id",
    "protocol",
    "binding_result",
    "rejection_reason",
    "imported_port_authority",
    "exported_port_authority",
];
const UNSATISFIED_IMPORT_FIELDS: &[&str] = &[
    "component_instance_id",
    "instance",
    "imported_port_id",
    "imported_port",
    "reason",
];
const AUTHORITY_EDGE_FIELDS: &[&str] = &[
    "port_binding_id",
    "edge_kind",
    "exporter_component_id",
    "exporter_component",
    "importer_component_id",
    "importer_component",
    "exported_port_id",
    "exported_port",
    "imported_port_id",
    "imported_port",
    "protocol_id",
    "protocol",
    "export_authority",
    "exported_port_authority",
    "imported_port_authority",
];
const COMPONENT_AUTHORITY_FIELDS: &[&str] = &["kind", "component_id", "component"];
const PORT_AUTHORITY_FIELDS: &[&str] = &["kind", "port_id", "port"];
const FNV1A64_FINGERPRINT_HEX_LEN: usize = 16;
const MAX_UNSATISFIED_IMPORT_COUNT: usize = MAX_COMPONENT_INSTANCE_COUNT * MAX_PORT_COUNT;

#[derive(Clone, Copy)]
struct PortFact {
    port_id: u32,
    protocol_id: u32,
}

#[derive(Clone)]
struct InstanceFact {
    component_id: u32,
    import_ports: Vec<PortFact>,
    export_port: PortFact,
}

#[derive(Clone, Copy)]
struct BindingFact {
    importer_instance_id: u32,
    imported_port_id: u32,
    exporter_instance_id: u32,
    exported_port_id: u32,
    protocol_id: u32,
}

pub(super) fn validate_component_composition_artifact(
    text: &str,
) -> Result<ComponentCompositionAdmissionSummary> {
    let artifact = JsonObject::new(text, "component composition artifact")?;
    artifact.require_exact_fields(TOP_LEVEL_FIELDS)?;
    artifact.required_string_eq("schema_id", COMPONENT_COMPOSITION_SCHEMA_ID)?;
    require_schema_version_eq(
        &artifact,
        "schema_version_major",
        COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR,
    )?;
    require_schema_version_eq(
        &artifact,
        "schema_version_minor",
        COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR,
    )?;
    artifact.required_string_eq("artifact_kind", ARTIFACT_KIND)?;
    artifact.required_string_eq("hash_alg", COMPONENT_COMPOSITION_HASH_ALG)?;
    artifact.required_string_eq("source_language", SOURCE_LANGUAGE)?;
    validate_metadata_string(&artifact, "source_module")?;
    validate_metadata_string(&artifact, "source_path")?;
    validate_source_fingerprint(&artifact)?;
    artifact.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
    validate_metadata_string(&artifact, "composition_name")?;
    let composition_id = artifact.required_u32("composition_id")?;

    let instances = validate_component_instances(&artifact.required_array("components")?)?;
    validate_empty_array(&artifact, "capability_bindings")?;
    validate_empty_array(&artifact, "interface_bindings")?;
    let (bindings, rejected_binding_count) =
        validate_port_bindings(&artifact.required_array("port_bindings")?, &instances)?;
    validate_empty_array(&artifact, "runtime_feature_bindings")?;
    validate_empty_array(&artifact, "archive_format_bindings")?;
    validate_empty_array(&artifact, "crypto_policy_bindings")?;
    let unsatisfied_imports =
        validate_unsatisfied_imports(&artifact.required_array("unsatisfied_imports")?, &instances)?;
    validate_import_coverage(&instances, &bindings, &unsatisfied_imports)?;
    validate_authority_edges(
        &artifact.required_array("cross_component_authority_edges")?,
        &instances,
        &bindings,
    )?;
    artifact.required_null("admission_policy_hash")?;
    let admission_result = admission_result(&artifact, "admission_result")?;
    validate_global_admission_result(
        admission_result,
        rejected_binding_count,
        unsatisfied_imports.len(),
    )?;
    artifact.required_null("diagnostic_set_hash")?;
    artifact.required_empty_object("extensions")?;

    Ok(ComponentCompositionAdmissionSummary {
        schema_id: COMPONENT_COMPOSITION_SCHEMA_ID,
        schema_version_major: COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR,
        schema_version_minor: COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR,
        hash_alg: COMPONENT_COMPOSITION_HASH_ALG,
        composition_id,
        component_instance_count: instances.len(),
        port_binding_count: bindings.len(),
        authority_edge_count: bindings.len(),
        unsatisfied_import_count: unsatisfied_imports.len(),
        rejected_binding_count,
        rejection_reason_count: rejected_binding_count + unsatisfied_imports.len(),
        admission_result,
    })
}

fn validate_component_instances(instances: &JsonArray<'_>) -> Result<Vec<InstanceFact>> {
    let count = instances.count_values()?;
    validate_count(
        "component_instance_count",
        count,
        1,
        MAX_COMPONENT_INSTANCE_COUNT,
    )?;
    let mut facts: Vec<InstanceFact> = Vec::with_capacity(count);
    instances.for_each_object(|index, object| {
        object.require_exact_fields(INSTANCE_FIELDS)?;
        validate_indexed_id(&object, "component_instance_id", index)?;
        validate_metadata_string(&object, "instance")?;
        validate_metadata_string(&object, "component")?;
        let component_id = bounded_id(&object, "component_id", MAX_COMPONENT_COUNT)?;
        validate_component_authority(
            &object.required_object("component_authority")?,
            component_id,
        )?;
        let import_ports =
            validate_component_import_ports(index, &object.required_array("import_ports")?)?;
        let export_port = validate_component_port(&object.required_object("export_port")?)?;
        facts.push(InstanceFact {
            component_id,
            import_ports,
            export_port,
        });
        Ok(())
    })?;
    Ok(facts)
}

fn validate_component_import_ports(
    instance_id: usize,
    ports: &JsonArray<'_>,
) -> Result<Vec<PortFact>> {
    let count = ports.count_values()?;
    validate_count("component_import_port_count", count, 0, MAX_PORT_COUNT)?;
    let mut facts: Vec<PortFact> = Vec::with_capacity(count);
    ports.for_each_object(|_, object| {
        let port = validate_component_port(&object)?;
        if facts.iter().any(|seen| seen.port_id == port.port_id) {
            return Err(Error::new(format!(
                "component instance id {instance_id} imports port id {} more than once",
                port.port_id
            )));
        }
        facts.push(port);
        Ok(())
    })?;
    Ok(facts)
}

fn validate_component_port(object: &JsonObject<'_>) -> Result<PortFact> {
    object.require_exact_fields(COMPONENT_PORT_FIELDS)?;
    validate_metadata_string(object, "port")?;
    validate_metadata_string(object, "protocol")?;
    let port_id = bounded_id(object, "port_id", MAX_PORT_COUNT)?;
    let protocol_id = bounded_id(object, "protocol_id", MAX_PROTOCOL_COUNT)?;
    validate_port_authority(&object.required_object("required_authority")?, port_id)?;
    Ok(PortFact {
        port_id,
        protocol_id,
    })
}

fn validate_port_bindings(
    bindings: &JsonArray<'_>,
    instances: &[InstanceFact],
) -> Result<(Vec<BindingFact>, usize)> {
    let count = bindings.count_values()?;
    validate_count("port_binding_count", count, 0, MAX_PORT_BINDING_COUNT)?;
    let mut facts: Vec<BindingFact> = Vec::with_capacity(count);
    let mut rejected = 0usize;
    bindings.for_each_object(|index, object| {
        object.require_exact_fields(PORT_BINDING_FIELDS)?;
        validate_indexed_id(&object, "port_binding_id", index)?;
        validate_metadata_string(&object, "importer_instance")?;
        validate_metadata_string(&object, "imported_port")?;
        validate_metadata_string(&object, "exporter_instance")?;
        validate_metadata_string(&object, "exported_port")?;
        validate_metadata_string(&object, "protocol")?;
        let importer_instance_id = bounded_existing_index(
            &object,
            "importer_instance_id",
            instances.len(),
            "component instance",
        )?;
        let exporter_instance_id = bounded_existing_index(
            &object,
            "exporter_instance_id",
            instances.len(),
            "component instance",
        )?;
        if importer_instance_id == exporter_instance_id {
            return Err(Error::new(format!(
                "port binding id {index} binds instance {importer_instance_id} to itself"
            )));
        }
        let imported_port_id = bounded_id(&object, "imported_port_id", MAX_PORT_COUNT)?;
        let exported_port_id = bounded_id(&object, "exported_port_id", MAX_PORT_COUNT)?;
        let protocol_id = bounded_id(&object, "protocol_id", MAX_PROTOCOL_COUNT)?;
        let importer =
            instance_fact(instances, importer_instance_id, "importer component instance")?;
        let exporter =
            instance_fact(instances, exporter_instance_id, "exporter component instance")?;
        let imported_port =
            import_port_fact(importer, importer_instance_id, imported_port_id)?;
        let export_port = exporter.export_port;
        if export_port.port_id != exported_port_id {
            return Err(Error::new(format!(
                "port binding id {index} exports port id {exported_port_id} but exporter instance {exporter_instance_id} exports port id {}",
                export_port.port_id
            )));
        }
        if imported_port.protocol_id != protocol_id {
            return Err(Error::new(format!(
                "port binding id {index} protocol id {protocol_id} does not match imported port protocol id {}",
                imported_port.protocol_id
            )));
        }
        if export_port.protocol_id != protocol_id {
            return Err(Error::new(format!(
                "port binding id {index} protocol id {protocol_id} does not match exported port protocol id {}",
                export_port.protocol_id
            )));
        }
        if binding_covers_import(&facts, importer_instance_id, imported_port_id) {
            return Err(Error::new(format!(
                "component composition artifact binds importer instance id {importer_instance_id} port id {imported_port_id} more than once"
            )));
        }
        let result = admission_result(&object, "binding_result")?;
        let reason = object.required_string("rejection_reason")?;
        validate_metadata_len("rejection_reason", reason.as_ref())?;
        match result {
            ComponentCompositionAdmissionResult::Admitted if !reason.is_empty() => {
                return Err(Error::new(format!(
                    "port binding id {index} is admitted but carries rejection evidence"
                )));
            }
            ComponentCompositionAdmissionResult::Admitted => {}
            ComponentCompositionAdmissionResult::Rejected if reason.is_empty() => {
                return Err(Error::new(format!(
                    "port binding id {index} is rejected without rejection evidence"
                )));
            }
            ComponentCompositionAdmissionResult::Rejected => {
                rejected = rejected
                    .checked_add(1)
                    .ok_or_else(|| Error::new("rejected binding count overflowed"))?;
            }
        }
        validate_port_authority(
            &object.required_object("imported_port_authority")?,
            imported_port_id,
        )?;
        validate_port_authority(
            &object.required_object("exported_port_authority")?,
            exported_port_id,
        )?;
        facts.push(BindingFact {
            importer_instance_id,
            imported_port_id,
            exporter_instance_id,
            exported_port_id,
            protocol_id,
        });
        Ok(())
    })?;
    Ok((facts, rejected))
}

fn validate_unsatisfied_imports(
    imports: &JsonArray<'_>,
    instances: &[InstanceFact],
) -> Result<Vec<(u32, u32)>> {
    let count = imports.count_values()?;
    validate_count(
        "unsatisfied_import_count",
        count,
        0,
        MAX_UNSATISFIED_IMPORT_COUNT,
    )?;
    let mut facts: Vec<(u32, u32)> = Vec::with_capacity(count);
    imports.for_each_object(|_, object| {
        object.require_exact_fields(UNSATISFIED_IMPORT_FIELDS)?;
        let instance_id = bounded_existing_index(
            &object,
            "component_instance_id",
            instances.len(),
            "component instance",
        )?;
        let imported_port_id = bounded_id(&object, "imported_port_id", MAX_PORT_COUNT)?;
        let instance = instance_fact(instances, instance_id, "component instance")?;
        let _ = import_port_fact(instance, instance_id, imported_port_id)?;
        let key = (instance_id, imported_port_id);
        if facts.contains(&key) {
            return Err(Error::new(format!(
                "component composition artifact reports unsatisfied importer instance id {instance_id} port id {imported_port_id} more than once"
            )));
        }
        facts.push(key);
        validate_metadata_string(&object, "instance")?;
        validate_metadata_string(&object, "imported_port")?;
        let reason = object.required_string("reason")?;
        validate_metadata_len("reason", reason.as_ref())?;
        if reason.is_empty() {
            return Err(Error::new(
                "unsatisfied import must explain why it is rejected",
            ));
        }
        Ok(())
    })?;
    Ok(facts)
}

fn validate_import_coverage(
    instances: &[InstanceFact],
    bindings: &[BindingFact],
    unsatisfied_imports: &[(u32, u32)],
) -> Result<()> {
    for (instance_index, instance) in instances.iter().enumerate() {
        let instance_id = u32::try_from(instance_index).map_err(|_| {
            Error::new(format!(
                "component instance index {instance_index} is too large"
            ))
        })?;
        for imported_port in &instance.import_ports {
            let key = (instance_id, imported_port.port_id);
            if binding_covers_import(bindings, instance_id, imported_port.port_id) {
                if unsatisfied_imports.contains(&key) {
                    return Err(Error::new(format!(
                        "component composition artifact marks importer instance id {instance_id} port id {} both bound and unsatisfied",
                        imported_port.port_id
                    )));
                }
                continue;
            }
            if unsatisfied_imports.contains(&key) {
                continue;
            }
            return Err(Error::new(format!(
                "component composition artifact omits binding or unsatisfied-import evidence for importer instance id {instance_id} port id {}",
                imported_port.port_id
            )));
        }
    }
    Ok(())
}

fn binding_covers_import(
    bindings: &[BindingFact],
    importer_instance_id: u32,
    imported_port_id: u32,
) -> bool {
    bindings.iter().any(|binding| {
        binding.importer_instance_id == importer_instance_id
            && binding.imported_port_id == imported_port_id
    })
}

fn validate_authority_edges(
    edges: &JsonArray<'_>,
    instances: &[InstanceFact],
    bindings: &[BindingFact],
) -> Result<()> {
    if edges.count_values()? != bindings.len() {
        return Err(Error::new(format!(
            "cross_component_authority_edges count must match port_binding_count {}",
            bindings.len()
        )));
    }
    edges.for_each_object(|index, object| {
        object.require_exact_fields(AUTHORITY_EDGE_FIELDS)?;
        validate_indexed_id(&object, "port_binding_id", index)?;
        object.required_string_eq("edge_kind", "port_binding")?;
        validate_metadata_string(&object, "exporter_component")?;
        validate_metadata_string(&object, "importer_component")?;
        validate_metadata_string(&object, "exported_port")?;
        validate_metadata_string(&object, "imported_port")?;
        validate_metadata_string(&object, "protocol")?;
        let binding = bindings
            .get(index)
            .ok_or_else(|| Error::new(format!("authority edge {index} has no binding")))?;
        let exporter_component_id = instances[binding.exporter_instance_id as usize].component_id;
        let importer_component_id = instances[binding.importer_instance_id as usize].component_id;
        require_u32_eq(&object, "exporter_component_id", exporter_component_id)?;
        require_u32_eq(&object, "importer_component_id", importer_component_id)?;
        require_u32_eq(&object, "exported_port_id", binding.exported_port_id)?;
        require_u32_eq(&object, "imported_port_id", binding.imported_port_id)?;
        require_u32_eq(&object, "protocol_id", binding.protocol_id)?;
        validate_component_authority(
            &object.required_object("export_authority")?,
            exporter_component_id,
        )?;
        validate_port_authority(
            &object.required_object("exported_port_authority")?,
            binding.exported_port_id,
        )?;
        validate_port_authority(
            &object.required_object("imported_port_authority")?,
            binding.imported_port_id,
        )?;
        Ok(())
    })
}

fn instance_fact<'a>(
    instances: &'a [InstanceFact],
    id: u32,
    label: &str,
) -> Result<&'a InstanceFact> {
    usize::try_from(id)
        .ok()
        .and_then(|index| instances.get(index))
        .ok_or_else(|| Error::new(format!("field references unknown {label} id {id}")))
}

fn import_port_fact(instance: &InstanceFact, instance_id: u32, port_id: u32) -> Result<PortFact> {
    instance
        .import_ports
        .iter()
        .copied()
        .find(|port| port.port_id == port_id)
        .ok_or_else(|| {
            Error::new(format!(
                "component instance id {instance_id} does not import port id {port_id}"
            ))
        })
}

fn validate_global_admission_result(
    result: ComponentCompositionAdmissionResult,
    rejected_binding_count: usize,
    unsatisfied_import_count: usize,
) -> Result<()> {
    match result {
        ComponentCompositionAdmissionResult::Admitted => {
            if unsatisfied_import_count != 0 {
                return Err(Error::new(
                    "admitted component composition artifact has unsatisfied_imports",
                ));
            }
            if rejected_binding_count != 0 {
                return Err(Error::new(
                    "admitted component composition artifact has rejected port_bindings",
                ));
            }
            Ok(())
        }
        ComponentCompositionAdmissionResult::Rejected => {
            if unsatisfied_import_count == 0 && rejected_binding_count == 0 {
                return Err(Error::new(
                    "rejected component composition artifact must explain why it is rejected",
                ));
            }
            Ok(())
        }
    }
}

fn validate_empty_array(artifact: &JsonObject<'_>, field: &str) -> Result<()> {
    let count = artifact.required_array(field)?.count_values()?;
    if count == 0 {
        Ok(())
    } else {
        Err(Error::new(format!(
            "component composition field {field:?} is not implemented in this source subset and must be empty"
        )))
    }
}

fn validate_component_authority(authority: &JsonObject<'_>, component_id: u32) -> Result<()> {
    authority.require_exact_fields(COMPONENT_AUTHORITY_FIELDS)?;
    authority.required_string_eq("kind", "component_export")?;
    require_u32_eq(authority, "component_id", component_id)?;
    validate_metadata_string(authority, "component")
}

fn validate_port_authority(authority: &JsonObject<'_>, port_id: u32) -> Result<()> {
    authority.require_exact_fields(PORT_AUTHORITY_FIELDS)?;
    authority.required_string_eq("kind", "port_connect")?;
    require_u32_eq(authority, "port_id", port_id)?;
    validate_metadata_string(authority, "port")
}

fn admission_result(
    object: &JsonObject<'_>,
    field: &str,
) -> Result<ComponentCompositionAdmissionResult> {
    match object.required_string(field)?.as_ref() {
        ADMISSION_RESULT_ADMITTED => Ok(ComponentCompositionAdmissionResult::Admitted),
        ADMISSION_RESULT_REJECTED => Ok(ComponentCompositionAdmissionResult::Rejected),
        other => Err(Error::new(format!(
            "field {field:?} must be {ADMISSION_RESULT_ADMITTED:?} or {ADMISSION_RESULT_REJECTED:?}, got {other:?}"
        ))),
    }
}

fn validate_indexed_id(object: &JsonObject<'_>, field: &str, expected: usize) -> Result<u32> {
    let actual = object.required_u32(field)?;
    let expected_u32 = u32::try_from(expected)
        .map_err(|_| Error::new(format!("{field} index {expected} is too large")))?;
    if actual == expected_u32 {
        Ok(actual)
    } else {
        Err(Error::new(format!(
            "{field} {actual} at array index {expected} is not canonical"
        )))
    }
}

fn bounded_existing_index(
    object: &JsonObject<'_>,
    field: &str,
    upper_bound: usize,
    label: &str,
) -> Result<u32> {
    let value = object.required_u32(field)?;
    if usize::try_from(value).is_ok_and(|index| index < upper_bound) {
        Ok(value)
    } else {
        Err(Error::new(format!(
            "field {field:?} references unknown {label} id {value}"
        )))
    }
}

fn bounded_id(object: &JsonObject<'_>, field: &str, max_count: usize) -> Result<u32> {
    let value = object.required_u32(field)?;
    let max = u32::try_from(max_count.saturating_sub(1))
        .map_err(|_| Error::new(format!("{field} bound is too large")))?;
    if value <= max {
        Ok(value)
    } else {
        Err(Error::new(format!(
            "field {field:?} id {value} exceeds maximum {max}"
        )))
    }
}

fn require_u32_eq(object: &JsonObject<'_>, field: &str, expected: u32) -> Result<()> {
    let actual = object.required_u32(field)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::new(format!(
            "field {field:?} must reference typed id {expected}, got {actual}"
        )))
    }
}

fn require_schema_version_eq(object: &JsonObject<'_>, field: &str, expected: u32) -> Result<()> {
    let actual = object.required_u32(field)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::new(format!(
            "field {field:?} must be schema version {expected}, got {actual}"
        )))
    }
}

fn validate_metadata_string(object: &JsonObject<'_>, field: &str) -> Result<()> {
    validate_metadata_len(field, object.required_string(field)?.as_ref())
}

fn validate_source_fingerprint(object: &JsonObject<'_>) -> Result<()> {
    let fingerprint = object.required_string("source_fingerprint")?;
    let fingerprint = fingerprint.as_ref();
    validate_metadata_len("source_fingerprint", fingerprint)?;
    if fingerprint.len() == FNV1A64_FINGERPRINT_HEX_LEN
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(Error::new(format!(
            "field \"source_fingerprint\" must be a {FNV1A64_FINGERPRINT_HEX_LEN}-character lowercase hexadecimal {SOURCE_FINGERPRINT_ALGORITHM} fingerprint"
        )))
    }
}

fn validate_metadata_len(field: &str, value: &str) -> Result<()> {
    if value.len() <= MAX_FIELD_VALUE_BYTES {
        Ok(())
    } else {
        Err(Error::new(format!(
            "metadata field {field:?} exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )))
    }
}

fn validate_count(name: &str, value: usize, min: usize, max: usize) -> Result<()> {
    if value < min {
        return Err(Error::new(format!("{name} must be at least {min}")));
    }
    if value > max {
        return Err(Error::new(format!("{name} must be no greater than {max}")));
    }
    Ok(())
}
