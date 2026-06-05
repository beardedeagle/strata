use std::fmt::Write as _;

use super::super::checked_render::push_json_field;
use super::super::composition_artifact::codec::{JsonArray, JsonObject};
use super::super::diagnostic::{Error, Result};
use super::source_facts::{
    CheckedAuthorityEffectFacts, DescriptorFact, admitted_authority_effect_facts,
};
use super::{
    AUTHORITY_EFFECT_SCHEMA_ID, AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
    AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR, SOURCE_FINGERPRINT_ALGORITHM,
};

pub const AUTHORITY_POLICY_SCHEMA_ID: &str = "strata.authority_policy_decisions";
pub const AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR: u32 = 1;
pub const AUTHORITY_POLICY_SCHEMA_VERSION_MINOR: u32 = 0;
pub const AUTHORITY_POLICY_ARTIFACT_EXTENSION: &str = "authority-policy.json";
pub const MAX_AUTHORITY_POLICY_ARTIFACT_BYTES: usize = 1024 * 1024;

const ARTIFACT_KIND: &str = "authority_policy_decisions";
const ADMISSION_RESULT_ADMITTED: &str = "admitted";
const TOP_LEVEL_FIELDS: &[&str] = &[
    "schema_id",
    "schema_version_major",
    "schema_version_minor",
    "artifact_kind",
    "source_language",
    "source_module",
    "source_fingerprint",
    "source_fingerprint_algorithm",
    "authority_effect_schema_id",
    "authority_effect_schema_version_major",
    "authority_effect_schema_version_minor",
    "decisions",
    "admission_result",
    "extensions",
];
const DECISION_FIELDS: &[&str] = &[
    "decision_id",
    "process_id",
    "authority_id",
    "descriptor",
    "decision",
];
const DESCRIPTOR_SPAWN_FIELDS: &[&str] = &["kind", "target_process_id"];
const DESCRIPTOR_PORT_FIELDS: &[&str] = &["kind", "port_id"];
const DESCRIPTOR_PROTOCOL_FIELDS: &[&str] = &["kind", "protocol_id"];
const DESCRIPTOR_COMPONENT_FIELDS: &[&str] = &["kind", "component_id"];
const INITIAL_POLICY_CAPACITY: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityPolicyDecision {
    Admit,
    Deny,
}

impl AuthorityPolicyDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admit => "admit",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityPolicyBuildOptions {
    pub spawn_authority_decision: AuthorityPolicyDecision,
    pub port_authority_decision: AuthorityPolicyDecision,
}

impl Default for AuthorityPolicyBuildOptions {
    fn default() -> Self {
        Self {
            spawn_authority_decision: AuthorityPolicyDecision::Admit,
            port_authority_decision: AuthorityPolicyDecision::Admit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityPolicyAdmissionResult {
    Admitted,
}

impl AuthorityPolicyAdmissionResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => ADMISSION_RESULT_ADMITTED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityPolicyAdmissionSummary {
    pub schema_id: &'static str,
    pub schema_version_major: u32,
    pub schema_version_minor: u32,
    pub authority_decision_count: usize,
    pub denied_authority_decision_count: usize,
    pub admission_result: AuthorityPolicyAdmissionResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuthorityPolicyDecisions {
    pub(super) decisions: Vec<AuthorityDecisionFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AuthorityDecisionFact {
    pub(super) process_id: u32,
    pub(super) authority_id: u32,
    pub(super) descriptor: DescriptorFact,
    pub(super) decision: AuthorityPolicyDecision,
}

pub fn render_authority_policy_artifact(
    authority_effect_text: &str,
    options: AuthorityPolicyBuildOptions,
) -> Result<String> {
    let facts = admitted_authority_effect_facts(authority_effect_text)?;
    let mut out = String::with_capacity(INITIAL_POLICY_CAPACITY);
    out.push('{');
    push_json_field(&mut out, "schema_id", AUTHORITY_POLICY_SCHEMA_ID);
    out.push_str(",\"schema_version_major\":");
    let _ = write!(out, "{AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR}");
    out.push_str(",\"schema_version_minor\":");
    let _ = write!(out, "{AUTHORITY_POLICY_SCHEMA_VERSION_MINOR}");
    out.push(',');
    push_json_field(&mut out, "artifact_kind", ARTIFACT_KIND);
    push_identity_json(&mut out, &facts);
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
    push_decisions_json(&mut out, &facts, options)?;
    out.push(',');
    push_json_field(&mut out, "admission_result", ADMISSION_RESULT_ADMITTED);
    out.push_str(",\"extensions\":{}");
    out.push('}');
    if out.len() > MAX_AUTHORITY_POLICY_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "authority policy artifact exceeds maximum size of {MAX_AUTHORITY_POLICY_ARTIFACT_BYTES} bytes"
        )));
    }
    Ok(out)
}

pub fn admit_authority_policy_artifact(
    policy_text: &str,
    authority_effect_text: &str,
) -> Result<AuthorityPolicyAdmissionSummary> {
    let facts = admitted_authority_effect_facts(authority_effect_text)?;
    let decisions = admitted_authority_policy_decisions(policy_text, &facts)?;
    let denied = decisions
        .decisions
        .iter()
        .filter(|decision| decision.decision == AuthorityPolicyDecision::Deny)
        .count();
    Ok(AuthorityPolicyAdmissionSummary {
        schema_id: AUTHORITY_POLICY_SCHEMA_ID,
        schema_version_major: AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR,
        schema_version_minor: AUTHORITY_POLICY_SCHEMA_VERSION_MINOR,
        authority_decision_count: decisions.decisions.len(),
        denied_authority_decision_count: denied,
        admission_result: AuthorityPolicyAdmissionResult::Admitted,
    })
}

pub(super) fn admitted_authority_policy_decisions(
    policy_text: &str,
    facts: &CheckedAuthorityEffectFacts<'_>,
) -> Result<AuthorityPolicyDecisions> {
    if policy_text.len() > MAX_AUTHORITY_POLICY_ARTIFACT_BYTES {
        return Err(Error::new(format!(
            "authority policy artifact exceeds maximum size of {MAX_AUTHORITY_POLICY_ARTIFACT_BYTES} bytes"
        )));
    }
    let policy = JsonObject::new(policy_text, "authority policy artifact")?;
    policy.require_exact_fields(TOP_LEVEL_FIELDS)?;
    policy.required_string_eq("schema_id", AUTHORITY_POLICY_SCHEMA_ID)?;
    policy
        .required_u32("schema_version_major")
        .and_then(|value| {
            require_u32_eq(
                value,
                AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR,
                "schema_version_major",
            )
        })?;
    policy
        .required_u32("schema_version_minor")
        .and_then(|value| {
            require_u32_eq(
                value,
                AUTHORITY_POLICY_SCHEMA_VERSION_MINOR,
                "schema_version_minor",
            )
        })?;
    policy.required_string_eq("artifact_kind", ARTIFACT_KIND)?;
    policy.required_string_eq("source_language", facts.source_language.as_ref())?;
    policy.required_string_eq("source_module", facts.source_module.as_ref())?;
    policy.required_string_eq("source_fingerprint", facts.source_fingerprint.as_ref())?;
    policy.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
    policy.required_string_eq("authority_effect_schema_id", AUTHORITY_EFFECT_SCHEMA_ID)?;
    require_u32_eq(
        policy.required_u32("authority_effect_schema_version_major")?,
        AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
        "authority_effect_schema_version_major",
    )?;
    require_u32_eq(
        policy.required_u32("authority_effect_schema_version_minor")?,
        AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
        "authority_effect_schema_version_minor",
    )?;
    policy.required_string_eq("admission_result", ADMISSION_RESULT_ADMITTED)?;
    policy.required_empty_object("extensions")?;
    let decisions = authority_decision_facts(&policy.required_array("decisions")?, facts)?;
    Ok(AuthorityPolicyDecisions { decisions })
}

pub fn render_authority_policy_admission_summary(
    summary: &AuthorityPolicyAdmissionSummary,
    artifact_path: &str,
    format: super::AuthorityEffectArtifactAdmitFormat,
) -> String {
    match format {
        super::AuthorityEffectArtifactAdmitFormat::Text => format!(
            "authority_policy_artifact: {artifact_path}\nschema_id: {}\nschema_version: {}.{}\nauthority_decisions: {}\ndenied_authority_decisions: {}\nadmission_result: {}\n",
            summary.schema_id,
            summary.schema_version_major,
            summary.schema_version_minor,
            summary.authority_decision_count,
            summary.denied_authority_decision_count,
            summary.admission_result.as_str()
        ),
        super::AuthorityEffectArtifactAdmitFormat::Json => {
            let mut out = String::new();
            out.push('{');
            push_json_field(&mut out, "schema_id", summary.schema_id);
            out.push_str(",\"schema_version_major\":");
            let _ = write!(out, "{}", summary.schema_version_major);
            out.push_str(",\"schema_version_minor\":");
            let _ = write!(out, "{}", summary.schema_version_minor);
            out.push(',');
            push_json_field(&mut out, "artifact", artifact_path);
            out.push_str(",\"authority_decision_count\":");
            let _ = write!(out, "{}", summary.authority_decision_count);
            out.push_str(",\"denied_authority_decision_count\":");
            let _ = write!(out, "{}", summary.denied_authority_decision_count);
            out.push(',');
            push_json_field(
                &mut out,
                "admission_result",
                summary.admission_result.as_str(),
            );
            out.push('}');
            out
        }
    }
}

fn push_identity_json(out: &mut String, facts: &CheckedAuthorityEffectFacts<'_>) {
    out.push(',');
    push_json_field(out, "source_language", facts.source_language.as_ref());
    out.push(',');
    push_json_field(out, "source_module", facts.source_module.as_ref());
    out.push(',');
    push_json_field(out, "source_fingerprint", facts.source_fingerprint.as_ref());
    out.push(',');
    push_json_field(
        out,
        "source_fingerprint_algorithm",
        SOURCE_FINGERPRINT_ALGORITHM,
    );
}

fn push_decisions_json(
    out: &mut String,
    facts: &CheckedAuthorityEffectFacts<'_>,
    options: AuthorityPolicyBuildOptions,
) -> Result<()> {
    out.push_str(",\"decisions\":[");
    let mut decision_id = 0u32;
    for (process_id, process) in facts.processes.iter().enumerate() {
        for (authority_id, descriptor) in process.authorities.iter().copied().enumerate() {
            if decision_id > 0 {
                out.push(',');
            }
            out.push_str("{\"decision_id\":");
            let _ = write!(out, "{decision_id}");
            out.push_str(",\"process_id\":");
            let _ = write!(out, "{process_id}");
            out.push_str(",\"authority_id\":");
            let _ = write!(out, "{authority_id}");
            out.push_str(",\"descriptor\":");
            push_descriptor_json(out, descriptor);
            out.push(',');
            push_json_field(
                out,
                "decision",
                decision_for_descriptor(descriptor, options).as_str(),
            );
            out.push('}');
            decision_id = decision_id
                .checked_add(1)
                .ok_or_else(|| Error::new("authority policy decision id overflowed"))?;
        }
    }
    out.push(']');
    Ok(())
}

fn authority_decision_facts(
    decisions: &JsonArray<'_>,
    facts: &CheckedAuthorityEffectFacts<'_>,
) -> Result<Vec<AuthorityDecisionFact>> {
    let expected_count = authority_count(facts)?;
    let mut parsed = Vec::with_capacity(expected_count);
    decisions.for_each_object(|index, decision| {
        decision.require_exact_fields(DECISION_FIELDS)?;
        let decision_id = decision.required_u32("decision_id")?;
        if usize::try_from(decision_id).ok() != Some(index) {
            return Err(Error::new(format!(
                "authority policy decision_id {decision_id} at array index {index} is not canonical"
            )));
        }
        let process_id = decision.required_u32("process_id")?;
        let authority_id = decision.required_u32("authority_id")?;
        let checked = facts
            .processes
            .get(usize::try_from(process_id).map_err(|_| {
                Error::new(format!(
                    "authority policy decision references unknown process id {process_id}"
                ))
            })?)
            .ok_or_else(|| {
                Error::new(format!(
                    "authority policy decision references unknown process id {process_id}"
                ))
            })?;
        let descriptor = checked
            .authorities
            .get(usize::try_from(authority_id).map_err(|_| {
                Error::new(format!(
                    "authority policy decision references unknown authority id {authority_id}"
                ))
            })?)
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "authority policy decision references unknown authority id {authority_id}"
                ))
            })?;
        let policy_descriptor = descriptor_fact(&decision.required_object("descriptor")?)?;
        if policy_descriptor != descriptor {
            return Err(Error::new(format!(
                "authority policy decision_id {decision_id} descriptor does not match checked authority/effect facts"
            )));
        }
        parsed.push(AuthorityDecisionFact {
            process_id,
            authority_id,
            descriptor,
            decision: policy_decision(decision.required_string("decision")?.as_ref())?,
        });
        Ok(())
    })?;
    if parsed.len() != expected_count {
        return Err(Error::new(format!(
            "authority policy decision count {} does not match checked authority count {expected_count}",
            parsed.len()
        )));
    }
    let mut cursor = 0usize;
    for (process_id, process) in facts.processes.iter().enumerate() {
        let process_id =
            u32::try_from(process_id).map_err(|_| Error::new("process id overflowed"))?;
        for authority_id in 0..process.authorities.len() {
            let authority_id =
                u32::try_from(authority_id).map_err(|_| Error::new("authority id overflowed"))?;
            let actual = parsed
                .get(cursor)
                .ok_or_else(|| Error::new("authority policy decision table is truncated"))?;
            if (process_id, authority_id) != (actual.process_id, actual.authority_id) {
                return Err(Error::new(format!(
                    "authority policy decision table is not closed over checked authorities at decision_id {}",
                    cursor
                )));
            }
            cursor = cursor
                .checked_add(1)
                .ok_or_else(|| Error::new("authority policy decision cursor overflowed"))?;
        }
    }
    Ok(parsed)
}

fn authority_count(facts: &CheckedAuthorityEffectFacts<'_>) -> Result<usize> {
    facts.processes.iter().try_fold(0usize, |count, process| {
        count
            .checked_add(process.authorities.len())
            .ok_or_else(|| Error::new("authority policy authority count overflowed"))
    })
}

fn decision_for_descriptor(
    descriptor: DescriptorFact,
    options: AuthorityPolicyBuildOptions,
) -> AuthorityPolicyDecision {
    match descriptor {
        DescriptorFact::Spawn { .. } => options.spawn_authority_decision,
        DescriptorFact::PortConnect { .. } => options.port_authority_decision,
        DescriptorFact::ProtocolBoundary { .. } | DescriptorFact::ComponentExport { .. } => {
            AuthorityPolicyDecision::Admit
        }
    }
}

pub(super) fn push_descriptor_json(out: &mut String, descriptor: DescriptorFact) {
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
            "unsupported authority policy descriptor kind {other:?}"
        ))),
    }
}

fn policy_decision(value: &str) -> Result<AuthorityPolicyDecision> {
    match value {
        "admit" => Ok(AuthorityPolicyDecision::Admit),
        "deny" => Ok(AuthorityPolicyDecision::Deny),
        other => Err(Error::new(format!(
            "unsupported authority policy decision {other:?}"
        ))),
    }
}

fn require_u32_eq(actual: u32, expected: u32, field: &str) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::new(format!(
            "authority policy artifact field {field:?} must be {expected}, got {actual}"
        )))
    }
}
