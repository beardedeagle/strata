use std::path::Path;

use mantle_artifact::{AuthorityId, Error, MantleArtifact, ProcessId, Result, read_text_artifact};

mod json;
mod validation;

use json::JsonObject;
use validation::{validate_component_surfaces, validate_policy_decisions, validate_processes};

pub(crate) const MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES: usize = 1024 * 1024;

const BINDING_SCHEMA_ID: &str = "mantle.runtime_authority_effect_binding";
const BINDING_SCHEMA_VERSION_MAJOR: u32 = 1;
const BINDING_SCHEMA_VERSION_MINOR: u32 = 0;
const BINDING_KIND: &str = "runtime_authority_effect_binding";
const BINDING_ADMISSION_RESULT: &str = "admitted";
const DEPLOYMENT_ID: u32 = 0;
const SOURCE_FINGERPRINT_ALGORITHM: &str = "fnv1a64-diagnostic";
// Checked authority/effect fact schemas are frontend-owned. Mantle validates the
// binding against the loaded artifact's source language plus this suffix instead
// of hardcoding any frontend-specific schema ownership.
const AUTHORITY_EFFECT_SCHEMA_SUFFIX: &str = ".checked_authority_effects";
const AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR: u32 = 1;
const AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR: u32 = 0;
const AUTHORITY_POLICY_SCHEMA_SUFFIX: &str = ".authority_policy_decisions";
const AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR: u32 = 1;
const AUTHORITY_POLICY_SCHEMA_VERSION_MINOR: u32 = 0;

const TOP_LEVEL_FIELDS: &[&str] = &[
    "schema_id",
    "schema_version_major",
    "schema_version_minor",
    "artifact_kind",
    "deployment_id",
    "source_language",
    "source_module",
    "source_fingerprint",
    "source_fingerprint_algorithm",
    "mantle_artifact_format",
    "mantle_artifact_schema_version",
    "mantle_artifact_module",
    "mantle_artifact_source_hash_fnv1a64",
    "authority_effect_schema_id",
    "authority_effect_schema_version_major",
    "authority_effect_schema_version_minor",
    "authority_policy_schema_id",
    "authority_policy_schema_version_major",
    "authority_policy_schema_version_minor",
    "processes",
    "component_authority_surfaces",
    "policy_decisions",
    "admission_result",
    "extensions",
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeAuthorityEffectBinding {
    policy: RuntimeAuthorityPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeAuthorityPolicy {
    AdmitAll,
    Decisions(Vec<RuntimeProcessAuthorityPolicy>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeProcessAuthorityPolicy {
    decisions: Vec<RuntimeAuthorityDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeAuthorityDecision {
    pub(crate) decision_id: u32,
    pub(crate) decision: RuntimeAuthorityPolicyDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeAuthorityPolicyDecision {
    Admit,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeAuthorityDecisionResult {
    pub(crate) decision_id: Option<u32>,
    pub(crate) decision: RuntimeAuthorityPolicyDecision,
}

impl RuntimeAuthorityEffectBinding {
    pub(crate) fn read_path(path: &Path, artifact: &MantleArtifact) -> Result<Self> {
        let text = read_text_artifact(path, MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES)?;
        Self::decode_and_validate(&text, artifact)
    }

    pub(crate) fn decode_text(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        Self::decode_and_validate(text, artifact)
    }

    #[cfg(test)]
    pub(crate) fn decode_for_test(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        Self::decode_and_validate(text, artifact)
    }

    pub(crate) fn into_policy(self) -> RuntimeAuthorityPolicy {
        self.policy
    }

    fn decode_and_validate(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        if text.len() > MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES {
            return Err(Error::new(format!(
                "runtime authority/effect binding exceeds maximum size of {MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES} bytes"
            )));
        }
        let object = JsonObject::new(text, "runtime authority/effect binding")?;
        object.required_string_eq("schema_id", BINDING_SCHEMA_ID)?;
        object.required_u32_eq("schema_version_major", BINDING_SCHEMA_VERSION_MAJOR)?;
        object.required_u32_eq("schema_version_minor", BINDING_SCHEMA_VERSION_MINOR)?;
        object.required_string_eq("artifact_kind", BINDING_KIND)?;
        object.require_exact_fields(TOP_LEVEL_FIELDS)?;
        object.required_u32_eq("deployment_id", DEPLOYMENT_ID)?;
        object.required_string_eq("admission_result", BINDING_ADMISSION_RESULT)?;
        object.required_empty_object("extensions")?;

        object.required_string_eq("source_language", artifact.source_language.as_ref())?;
        object.required_string_eq("source_module", &artifact.module)?;
        object.required_string_eq("source_fingerprint", &artifact.source_hash_fnv1a64)?;
        object.required_string_eq("source_fingerprint_algorithm", SOURCE_FINGERPRINT_ALGORITHM)?;
        object.required_string_eq("mantle_artifact_format", artifact.format.as_ref())?;
        object.required_string_eq(
            "mantle_artifact_schema_version",
            artifact.schema_version.as_ref(),
        )?;
        object.required_string_eq("mantle_artifact_module", &artifact.module)?;
        object.required_string_eq(
            "mantle_artifact_source_hash_fnv1a64",
            &artifact.source_hash_fnv1a64,
        )?;
        object.required_schema_id_with_suffix(
            "authority_effect_schema_id",
            artifact.source_language.as_ref(),
            AUTHORITY_EFFECT_SCHEMA_SUFFIX,
            "authority/effect",
        )?;
        object.required_u32_eq(
            "authority_effect_schema_version_major",
            AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
        )?;
        object.required_u32_eq(
            "authority_effect_schema_version_minor",
            AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
        )?;
        object.required_schema_id_with_suffix(
            "authority_policy_schema_id",
            artifact.source_language.as_ref(),
            AUTHORITY_POLICY_SCHEMA_SUFFIX,
            "authority policy",
        )?;
        object.required_u32_eq(
            "authority_policy_schema_version_major",
            AUTHORITY_POLICY_SCHEMA_VERSION_MAJOR,
        )?;
        object.required_u32_eq(
            "authority_policy_schema_version_minor",
            AUTHORITY_POLICY_SCHEMA_VERSION_MINOR,
        )?;

        validate_processes(&object.required_array("processes")?, artifact)?;
        validate_component_surfaces(
            &object.required_array("component_authority_surfaces")?,
            artifact,
        )?;
        let policy =
            validate_policy_decisions(&object.required_array("policy_decisions")?, artifact)?;
        Ok(Self { policy })
    }
}

impl RuntimeAuthorityPolicy {
    pub(crate) fn admit_all() -> Self {
        Self::AdmitAll
    }

    pub(crate) fn decision_for_authority(
        &self,
        process_id: ProcessId,
        authority_id: AuthorityId,
    ) -> Result<RuntimeAuthorityDecisionResult> {
        let Self::Decisions(processes) = self else {
            return Ok(RuntimeAuthorityDecisionResult {
                decision_id: None,
                decision: RuntimeAuthorityPolicyDecision::Admit,
            });
        };
        let process = processes.get(process_id.index()).ok_or_else(|| {
            Error::new(format!(
                "authority policy has no decision row for process_id {}",
                process_id.as_u32()
            ))
        })?;
        let decision = process.decisions.get(authority_id.index()).ok_or_else(|| {
            Error::new(format!(
                "authority policy has no decision for process_id {} authority_id {}",
                process_id.as_u32(),
                authority_id.as_u32()
            ))
        })?;
        Ok(RuntimeAuthorityDecisionResult {
            decision_id: Some(decision.decision_id),
            decision: decision.decision,
        })
    }
}

impl RuntimeAuthorityPolicyDecision {
    pub(crate) const fn admits(self) -> bool {
        matches!(self, Self::Admit)
    }
}

pub fn validate_runtime_authority_effect_binding_text(
    text: &str,
    artifact: &MantleArtifact,
) -> Result<()> {
    RuntimeAuthorityEffectBinding::decode_and_validate(text, artifact).map(|_| ())
}
