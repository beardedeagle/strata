use std::fmt::Write as _;

use mantle_artifact::{
    ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, MantleArtifact, Result, RuntimeFeature,
};

use crate::target_profile::{LOCAL_RUNTIME_TARGET_PROFILE, UnsupportedRuntimeBoundary};

pub const RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION: &str = "mantle.feature_declaration.v6";
const SOURCE_LANGUAGE_SUPPORT: SourceLanguageSupport = SourceLanguageSupport::ArtifactMetadata;
const MANTLE_VERSION: &str = env!("CARGO_PKG_VERSION");
const STRATA_VERSION: &str = "0.16.0";
const NON_PROGRESS_CONTAINMENT: &str = "none";
const MAILBOX_LOGICAL_CAPACITY_MODEL: &str = "message_count";
const SCHEDULER_REDUCTION_WINDOW: u32 = 2_000;
const WIRE_FORMAT_VERSION: &str = "mantle.archive.v1";
const MESSAGE_OBSERVATION_CAPTURE_MODEL: &str = "mantle.message_observation.v1";
const ALLOCATION_MODEL: &str = "host_process_allocator";
const VALIDITY_MEMBER_NONE_MAX_MS: u64 = 900_000;
const VALIDITY_MEMBER_HEARTBEAT_FORMULA: &str = "max(30min, 4*t)";
const VALIDITY_MEMBER_ACTIVE_RENEWAL_MS: u64 = 86_400_000;
const VALIDITY_ISSUANCE_DEFAULT_MS: u64 = 28_800_000;
const VALIDITY_CROSS_CLUSTER_DEFAULT_MS: u64 = 3_600_000;
const VALIDITY_REPOSITORY_AUTHORITY_DEFAULT_MS: u64 = 604_800_000;
const CONFORMANCE_CORPUS_VERSION: &str = "bounded_local_current";
const BACKEND_IDENTITY: &str = "mantle-runtime-rust";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceLanguageSupport {
    ArtifactMetadata,
}

impl SourceLanguageSupport {
    fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactMetadata => "artifact_declared_metadata",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFeatureDeclarationFormat {
    Text,
    Json,
}

pub fn render_runtime_feature_declaration(format: RuntimeFeatureDeclarationFormat) -> String {
    match format {
        RuntimeFeatureDeclarationFormat::Text => render_text(),
        RuntimeFeatureDeclarationFormat::Json => render_json(),
    }
}

pub(crate) fn validate_artifact_runtime_requirements(artifact: &MantleArtifact) -> Result<()> {
    artifact.validate()?;
    validate_runtime_features_supported(&artifact.target_requirements.features)
}

pub(crate) fn validate_runtime_features_supported(features: &[RuntimeFeature]) -> Result<()> {
    LOCAL_RUNTIME_TARGET_PROFILE.validate_features(features)
}

fn render_text() -> String {
    let target_profile = LOCAL_RUNTIME_TARGET_PROFILE;
    let mut out = String::new();
    out.push_str("mantle runtime feature declaration\n");
    writeln!(
        out,
        "declaration_schema_version: {RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION}"
    )
    .expect("writing to a String cannot fail");
    writeln!(out, "artifact_format: {ARTIFACT_FORMAT}").expect("writing to a String cannot fail");
    writeln!(out, "artifact_schema_version: {ARTIFACT_SCHEMA_VERSION}")
        .expect("writing to a String cannot fail");
    writeln!(out, "mantle_version: {MANTLE_VERSION}").expect("writing to a String cannot fail");
    writeln!(out, "strata_version: {STRATA_VERSION}").expect("writing to a String cannot fail");
    out.push_str("source_language_support: ");
    out.push_str(SOURCE_LANGUAGE_SUPPORT.as_str());
    out.push('\n');
    writeln!(out, "target_profile: {}", target_profile.id)
        .expect("writing to a String cannot fail");
    writeln!(out, "runtime_scope: {}", target_profile.execution_scope)
        .expect("writing to a String cannot fail");
    writeln!(out, "host_authority: {}", target_profile.host_authority)
        .expect("writing to a String cannot fail");
    writeln!(
        out,
        "network_authority: {}",
        target_profile.network_authority
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "transport_authority: {}",
        target_profile.transport_authority
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "cluster_membership: {}",
        target_profile.cluster_membership
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "remote_operations.policy: {}",
        target_profile.remote_operation_policy
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "remote_operations.fail_stage: {}",
        target_profile.unsupported_fail_stage
    )
    .expect("writing to a String cannot fail");
    out.push_str("optional_strata_profiles: []\n");
    out.push_str("mantle_profiles:\n");
    out.push_str("  - ");
    out.push_str(target_profile.id);
    out.push('\n');
    writeln!(out, "non_progress_containment: {NON_PROGRESS_CONTAINMENT}")
        .expect("writing to a String cannot fail");
    writeln!(
        out,
        "mailbox.logical_capacity_model: {MAILBOX_LOGICAL_CAPACITY_MODEL}"
    )
    .expect("writing to a String cannot fail");
    out.push_str("mailbox.closed_outcome_supported: true\n");
    writeln!(
        out,
        "scheduler.reduction_window: {SCHEDULER_REDUCTION_WINDOW}"
    )
    .expect("writing to a String cannot fail");
    writeln!(out, "wire_format.version: {WIRE_FORMAT_VERSION}")
        .expect("writing to a String cannot fail");
    out.push_str("strata.exact_effects_supported: [emit, spawn, send]\n");
    out.push_str("strata.determinism_sources_supported: []\n");
    writeln!(
        out,
        "message_observation.capture_model: {MESSAGE_OBSERVATION_CAPTURE_MODEL}"
    )
    .expect("writing to a String cannot fail");
    out.push_str("message_observation.payload_capture_supported: false\n");
    out.push_str("message_observation.order_capture_supported: false\n");
    out.push_str("message_observation.redaction_supported: false\n");
    out.push_str("allocation.model: ");
    out.push_str(ALLOCATION_MODEL);
    out.push('\n');
    out.push_str("allocation.failure_modes_declared: false\n");
    out.push_str("allocation.safepoint_on_allocation: false\n");
    out.push_str("component_composition.observability_supported: true\n");
    out.push_str("spawn_observability.kind_supported: true\n");
    out.push_str("spawn_observability.authority_result_supported: true\n");
    out.push_str("spawn_observability.backend_unavailable_supported: true\n");
    out.push_str("spawn_observability.placement_supported: false\n");
    out.push_str("distributed.itc_retirement.enabled: false\n");
    out.push_str("distributed.itc_retirement.evidence_retention_ms: 0\n");
    out.push_str("distributed.cross_cluster.supported: false\n");
    out.push_str("transport.features_supported: []\n");
    out.push_str("repository.features_supported: []\n");
    out.push_str("archive_validation.features_supported: []\n");
    out.push_str("causality.primitives_supported: []\n");
    out.push_str("port_authority.enforcement_model: mantle.port_authority.v1\n");
    out.push_str("port_authority.denial_classes_declared: true\n");
    out.push_str("family_protocol.compatibility_model: unsupported\n");
    writeln!(
        out,
        "validity_window.defaults.member_none_max_ms: {VALIDITY_MEMBER_NONE_MAX_MS}"
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "validity_window.defaults.member_heartbeat_formula: {VALIDITY_MEMBER_HEARTBEAT_FORMULA}"
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "validity_window.defaults.member_active_renewal_ms: {VALIDITY_MEMBER_ACTIVE_RENEWAL_MS}"
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "validity_window.defaults.issuance_default_ms: {VALIDITY_ISSUANCE_DEFAULT_MS}"
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "validity_window.defaults.cross_cluster_default_ms: {VALIDITY_CROSS_CLUSTER_DEFAULT_MS}"
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "validity_window.defaults.repository_authority_default_ms: {VALIDITY_REPOSITORY_AUTHORITY_DEFAULT_MS}"
    )
    .expect("writing to a String cannot fail");
    out.push_str("performance.claims_published: false\n");
    out.push_str("performance.envelope_id: none\n");
    writeln!(
        out,
        "supported_conformance_corpus_version: {CONFORMANCE_CORPUS_VERSION}"
    )
    .expect("writing to a String cannot fail");
    writeln!(out, "backend_identity: {BACKEND_IDENTITY}").expect("writing to a String cannot fail");
    out.push_str("supported_runtime_features:\n");
    for feature in target_profile.supported_features() {
        out.push_str("  - ");
        out.push_str(feature.as_str());
        out.push('\n');
    }
    out.push_str("implementation_limits:\n");
    for boundary in target_profile.unsupported_boundaries() {
        out.push_str("  - unsupported ");
        out.push_str(boundary.feature.as_str());
        out.push_str(" boundary=");
        out.push_str(boundary.boundary);
        out.push_str(" required_authority=");
        out.push_str(boundary.required_authority);
        out.push_str(" fail_stage=");
        out.push_str(boundary.fail_stage);
        out.push('\n');
    }
    out
}

fn render_json() -> String {
    let target_profile = LOCAL_RUNTIME_TARGET_PROFILE;
    let mut out = String::new();
    out.push('{');
    push_json_field(
        &mut out,
        "declaration_schema_version",
        RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION,
    );
    out.push(',');
    push_json_field(&mut out, "artifact_format", ARTIFACT_FORMAT);
    out.push(',');
    push_json_field(&mut out, "artifact_schema_version", ARTIFACT_SCHEMA_VERSION);
    out.push(',');
    push_json_field(&mut out, "mantle_version", MANTLE_VERSION);
    out.push(',');
    push_json_field(&mut out, "strata_version", STRATA_VERSION);
    out.push(',');
    push_json_field(
        &mut out,
        "source_language_support",
        SOURCE_LANGUAGE_SUPPORT.as_str(),
    );
    out.push(',');
    push_json_field(&mut out, "target_profile", target_profile.id);
    out.push(',');
    push_json_field(&mut out, "runtime_scope", target_profile.execution_scope);
    out.push(',');
    push_json_field(&mut out, "host_authority", target_profile.host_authority);
    out.push(',');
    push_json_field(
        &mut out,
        "network_authority",
        target_profile.network_authority,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "transport_authority",
        target_profile.transport_authority,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "cluster_membership",
        target_profile.cluster_membership,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "remote_operations.policy",
        target_profile.remote_operation_policy,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "remote_operations.fail_stage",
        target_profile.unsupported_fail_stage,
    );
    out.push(',');
    push_json_string_array_field(&mut out, "optional_strata_profiles", &[]);
    out.push(',');
    push_json_string_array_field(&mut out, "mantle_profiles", &[target_profile.id]);
    out.push(',');
    push_json_field(
        &mut out,
        "non_progress_containment",
        NON_PROGRESS_CONTAINMENT,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "mailbox.logical_capacity_model",
        MAILBOX_LOGICAL_CAPACITY_MODEL,
    );
    out.push(',');
    push_json_bool_field(&mut out, "mailbox.closed_outcome_supported", true);
    out.push(',');
    push_json_u32_field(
        &mut out,
        "scheduler.reduction_window",
        SCHEDULER_REDUCTION_WINDOW,
    );
    out.push(',');
    push_json_field(&mut out, "wire_format.version", WIRE_FORMAT_VERSION);
    out.push(',');
    push_json_string_array_field(
        &mut out,
        "strata.exact_effects_supported",
        &["emit", "spawn", "send"],
    );
    out.push(',');
    push_json_string_array_field(&mut out, "strata.determinism_sources_supported", &[]);
    out.push(',');
    push_json_field(
        &mut out,
        "message_observation.capture_model",
        MESSAGE_OBSERVATION_CAPTURE_MODEL,
    );
    out.push(',');
    push_json_bool_field(
        &mut out,
        "message_observation.payload_capture_supported",
        false,
    );
    out.push(',');
    push_json_bool_field(
        &mut out,
        "message_observation.order_capture_supported",
        false,
    );
    out.push(',');
    push_json_bool_field(&mut out, "message_observation.redaction_supported", false);
    out.push(',');
    push_json_field(&mut out, "allocation.model", ALLOCATION_MODEL);
    out.push(',');
    push_json_bool_field(&mut out, "allocation.failure_modes_declared", false);
    out.push(',');
    push_json_bool_field(&mut out, "allocation.safepoint_on_allocation", false);
    out.push(',');
    push_json_bool_field(
        &mut out,
        "component_composition.observability_supported",
        true,
    );
    out.push(',');
    push_json_bool_field(&mut out, "spawn_observability.kind_supported", true);
    out.push(',');
    push_json_bool_field(
        &mut out,
        "spawn_observability.authority_result_supported",
        true,
    );
    out.push(',');
    push_json_bool_field(
        &mut out,
        "spawn_observability.backend_unavailable_supported",
        true,
    );
    out.push(',');
    push_json_bool_field(&mut out, "spawn_observability.placement_supported", false);
    out.push(',');
    push_json_bool_field(&mut out, "distributed.itc_retirement.enabled", false);
    out.push(',');
    push_json_u32_field(
        &mut out,
        "distributed.itc_retirement.evidence_retention_ms",
        0,
    );
    out.push(',');
    push_json_bool_field(&mut out, "distributed.cross_cluster.supported", false);
    out.push(',');
    push_json_string_array_field(&mut out, "transport.features_supported", &[]);
    out.push(',');
    push_json_string_array_field(&mut out, "repository.features_supported", &[]);
    out.push(',');
    push_json_string_array_field(&mut out, "archive_validation.features_supported", &[]);
    out.push(',');
    push_json_string_array_field(&mut out, "causality.primitives_supported", &[]);
    out.push(',');
    push_json_field(
        &mut out,
        "port_authority.enforcement_model",
        "mantle.port_authority.v1",
    );
    out.push(',');
    push_json_bool_field(&mut out, "port_authority.denial_classes_declared", true);
    out.push(',');
    push_json_field(
        &mut out,
        "family_protocol.compatibility_model",
        "unsupported",
    );
    out.push(',');
    push_json_u64_field(
        &mut out,
        "validity_window.defaults.member_none_max_ms",
        VALIDITY_MEMBER_NONE_MAX_MS,
    );
    out.push(',');
    push_json_field(
        &mut out,
        "validity_window.defaults.member_heartbeat_formula",
        VALIDITY_MEMBER_HEARTBEAT_FORMULA,
    );
    out.push(',');
    push_json_u64_field(
        &mut out,
        "validity_window.defaults.member_active_renewal_ms",
        VALIDITY_MEMBER_ACTIVE_RENEWAL_MS,
    );
    out.push(',');
    push_json_u64_field(
        &mut out,
        "validity_window.defaults.issuance_default_ms",
        VALIDITY_ISSUANCE_DEFAULT_MS,
    );
    out.push(',');
    push_json_u64_field(
        &mut out,
        "validity_window.defaults.cross_cluster_default_ms",
        VALIDITY_CROSS_CLUSTER_DEFAULT_MS,
    );
    out.push(',');
    push_json_u64_field(
        &mut out,
        "validity_window.defaults.repository_authority_default_ms",
        VALIDITY_REPOSITORY_AUTHORITY_DEFAULT_MS,
    );
    out.push(',');
    push_json_bool_field(&mut out, "performance.claims_published", false);
    out.push(',');
    push_json_field(&mut out, "performance.envelope_id", "none");
    out.push(',');
    push_json_field(
        &mut out,
        "supported_conformance_corpus_version",
        CONFORMANCE_CORPUS_VERSION,
    );
    out.push(',');
    push_json_field(&mut out, "backend_identity", BACKEND_IDENTITY);
    out.push(',');
    push_json_runtime_feature_array_field(
        &mut out,
        "supported_runtime_features",
        target_profile.supported_features(),
    );
    out.push(',');
    push_json_runtime_feature_array_field(
        &mut out,
        "implementation_limits",
        target_profile.implementation_limits(),
    );
    out.push(',');
    push_json_implementation_limit_boundaries(&mut out, target_profile.unsupported_boundaries());
    out.push('}');
    out
}

fn push_json_field(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    push_json_string(out, value);
}

fn push_json_bool_field(out: &mut String, key: &str, value: bool) {
    push_json_string(out, key);
    out.push(':');
    out.push_str(if value { "true" } else { "false" });
}

fn push_json_u32_field(out: &mut String, key: &str, value: u32) {
    push_json_string(out, key);
    out.push(':');
    write!(out, "{value}").expect("writing to a String cannot fail");
}

fn push_json_u64_field(out: &mut String, key: &str, value: u64) {
    push_json_string(out, key);
    out.push(':');
    write!(out, "{value}").expect("writing to a String cannot fail");
}

fn push_json_string_array_field(out: &mut String, key: &str, values: &[&str]) {
    push_json_string(out, key);
    out.push_str(":[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(out, value);
    }
    out.push(']');
}

fn push_json_runtime_feature_array_field(out: &mut String, key: &str, values: &[RuntimeFeature]) {
    push_json_string(out, key);
    out.push_str(":[");
    for (index, feature) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(out, feature.as_str());
    }
    out.push(']');
}

fn push_json_implementation_limit_boundaries(
    out: &mut String,
    boundaries: &[UnsupportedRuntimeBoundary],
) {
    push_json_string(out, "implementation_limit_boundaries");
    out.push_str(":[");
    for (index, boundary) in boundaries.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        push_json_field(out, "feature", boundary.feature.as_str());
        out.push(',');
        push_json_field(out, "boundary", boundary.boundary);
        out.push(',');
        push_json_field(out, "required_authority", boundary.required_authority);
        out.push(',');
        push_json_field(out, "fail_stage", boundary.fail_stage);
        out.push('}');
    }
    out.push(']');
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => push_json_control_escape(out, c),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_json_control_escape(out: &mut String, ch: char) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let value = ch as usize;
    out.push_str("\\u00");
    out.push(char::from(HEX[(value >> 4) & 0x0f]));
    out.push(char::from(HEX[value & 0x0f]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_feature_declaration_lists_supported_runtime_features() {
        let declaration = render_runtime_feature_declaration(RuntimeFeatureDeclarationFormat::Text);

        assert!(declaration.contains("mantle runtime feature declaration"));
        assert!(declaration.contains(RUNTIME_FEATURE_DECLARATION_SCHEMA_VERSION));
        assert!(declaration.contains("strata_version: 0.16.0"));
        assert!(declaration.contains("non_progress_containment: none"));
        assert!(declaration.contains("mailbox.logical_capacity_model: message_count"));
        assert!(declaration.contains("mailbox.closed_outcome_supported: true"));
        assert!(declaration.contains("scheduler.reduction_window: 2000"));
        assert!(declaration.contains("wire_format.version: mantle.archive.v1"));
        assert!(declaration.contains("message_observation.capture_model"));
        assert!(declaration.contains("allocation.model: host_process_allocator"));
        assert!(declaration.contains("validity_window.defaults.member_none_max_ms"));
        assert!(declaration.contains("source_language_support: artifact_declared_metadata"));
        assert!(declaration.contains("target_profile: mantle.local_only.v1"));
        assert!(declaration.contains("mantle_profiles:\n  - mantle.local_only.v1"));
        assert!(declaration.contains("runtime_scope: single_host_local_runtime"));
        assert!(declaration.contains("network_authority: none"));
        assert!(declaration.contains("remote_operations.policy: non_admitted"));
        assert!(declaration.contains("local_execution"));
        assert!(declaration.contains("typed_boundary_tables"));
        assert!(declaration.contains("unsupported distributed_transport"));
        assert!(declaration.contains("unsupported remote_send"));
        assert!(declaration.contains("unsupported remote_spawn"));
    }

    #[test]
    fn json_feature_declaration_uses_machine_readable_schema() {
        let declaration = render_runtime_feature_declaration(RuntimeFeatureDeclarationFormat::Json);

        assert!(declaration.starts_with("{\"declaration_schema_version\""));
        assert!(declaration.contains("\"mantle.feature_declaration.v6\""));
        assert!(declaration.contains("\"artifact_format\":\"mantle-target-artifact\""));
        assert!(declaration.contains("\"strata_version\":\"0.16.0\""));
        assert!(declaration.contains("\"source_language_support\":\"artifact_declared_metadata\""));
        assert!(declaration.contains("\"target_profile\":\"mantle.local_only.v1\""));
        assert!(declaration.contains("\"mantle_profiles\":[\"mantle.local_only.v1\"]"));
        assert!(declaration.contains("\"runtime_scope\":\"single_host_local_runtime\""));
        assert!(declaration.contains("\"network_authority\":\"none\""));
        assert!(declaration.contains("\"remote_operations.policy\":\"non_admitted\""));
        assert!(declaration.contains("\"non_progress_containment\":\"none\""));
        assert!(declaration.contains("\"mailbox.logical_capacity_model\":\"message_count\""));
        assert!(declaration.contains("\"mailbox.closed_outcome_supported\":true"));
        assert!(declaration.contains("\"scheduler.reduction_window\":2000"));
        assert!(declaration.contains("\"wire_format.version\":\"mantle.archive.v1\""));
        assert!(declaration.contains("\"message_observation.capture_model\""));
        assert!(declaration.contains("\"component_composition.observability_supported\":true"));
        assert!(declaration.contains("\"spawn_observability.backend_unavailable_supported\":true"));
        assert!(declaration.contains("\"validity_window.defaults.member_none_max_ms\":900000"));
        assert!(declaration.contains("\"local_spawn\""));
        assert!(declaration.contains(
            "\"implementation_limits\":[\"distributed_transport\",\"remote_send\",\"remote_spawn\"]"
        ));
        assert!(declaration.contains("\"implementation_limit_boundaries\""));
        assert!(declaration.contains("\"boundary\":\"remote_process_spawn\""));
        assert!(declaration.ends_with("]}"));
    }

    #[test]
    fn json_feature_declaration_escapes_control_chars_precisely() {
        let mut out = String::new();

        push_json_string(&mut out, "feature\u{0001}\u{001f}\n");

        assert_eq!(out, "\"feature\\u0001\\u001f\\n\"");
    }

    #[test]
    fn validation_rejects_malformed_source_language_metadata() {
        let artifact = MantleArtifact {
            format: ARTIFACT_FORMAT.into(),
            schema_version: ARTIFACT_SCHEMA_VERSION.into(),
            source_language: "not-valid".into(),
            target_requirements: mantle_artifact::ArtifactTargetRequirements::new(
                "not-valid",
                vec![RuntimeFeature::BoundedMailbox],
            ),
            module: String::new(),
            entry_process: mantle_artifact::ProcessId::new(0),
            entry_message: mantle_artifact::MessageId::new(0),
            types: Vec::new(),
            outputs: Vec::new(),
            protocols: Vec::new(),
            ports: Vec::new(),
            components: Vec::new(),
            compositions: Vec::new(),
            processes: Vec::new(),
            source_hash_fnv1a64: String::new(),
        };
        let err = validate_artifact_runtime_requirements(&artifact)
            .expect_err("malformed source language metadata should fail closed");

        assert!(
            err.to_string()
                .contains("artifact field source_language must be an identifier")
        );
    }

    #[test]
    fn validation_rejects_unsupported_runtime_feature() {
        for feature in LOCAL_RUNTIME_TARGET_PROFILE.implementation_limits() {
            let err = validate_runtime_features_supported(&[*feature])
                .expect_err("implementation limit should fail closed before execution");

            assert!(err.to_string().contains(feature.as_str()));
        }
    }
}
