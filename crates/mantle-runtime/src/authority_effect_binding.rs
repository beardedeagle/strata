use std::path::Path;

use mantle_artifact::{Error, MantleArtifact, Result, read_text_artifact};

use crate::limits::SpawnAuthorityPolicy;

mod json;
mod validation;

use json::JsonObject;
use validation::{validate_component_surfaces, validate_policy, validate_processes};

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
    "processes",
    "component_authority_surfaces",
    "policy",
    "admission_result",
    "extensions",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeAuthorityEffectBinding {
    spawn_authority_policy: SpawnAuthorityPolicy,
}

impl RuntimeAuthorityEffectBinding {
    pub(crate) fn read_path(path: &Path, artifact: &MantleArtifact) -> Result<Self> {
        let text = read_text_artifact(path, MAX_RUNTIME_AUTHORITY_EFFECT_BINDING_BYTES)?;
        Self::decode_and_validate(&text, artifact)
    }

    #[cfg(test)]
    pub(crate) fn decode_for_test(text: &str, artifact: &MantleArtifact) -> Result<Self> {
        Self::decode_and_validate(text, artifact)
    }

    pub(crate) fn spawn_authority_policy(self) -> SpawnAuthorityPolicy {
        self.spawn_authority_policy
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
        )?;
        object.required_u32_eq(
            "authority_effect_schema_version_major",
            AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
        )?;
        object.required_u32_eq(
            "authority_effect_schema_version_minor",
            AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
        )?;

        validate_processes(&object.required_array("processes")?, artifact)?;
        validate_component_surfaces(
            &object.required_array("component_authority_surfaces")?,
            artifact,
        )?;
        let spawn_authority_policy = validate_policy(&object.required_object("policy")?)?;
        Ok(Self {
            spawn_authority_policy,
        })
    }
}

pub fn validate_runtime_authority_effect_binding_text(
    text: &str,
    artifact: &MantleArtifact,
) -> Result<()> {
    RuntimeAuthorityEffectBinding::decode_and_validate(text, artifact).map(|_| ())
}
