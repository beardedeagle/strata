mod ast;
mod authority_effect_artifact;
mod authority_summary;
mod checked;
mod checked_render;
mod checker;
mod component_authority_edges;
mod composition_artifact;
mod composition_report;
mod diagnostic;
mod import_access;
mod import_scope;
mod import_symbols;
mod lexer;
mod lowering;
mod parser;
mod source_program;
mod target_requirements;

#[cfg(test)]
mod tests;

pub use ast::{
    AuthorityDeclaration, CollectionPatternBinding, Component, ComponentInstance, Composition,
    ConstructorPayloadPattern, Determinism, Effect, Enum, ForEachItem, Function, FunctionBlock,
    FunctionBody, FunctionParam, Identifier, Import, ListPattern, ListValue, MapPattern,
    MapPatternCompleteness, MapPatternEntry, MapValue, MapValueEntry, Match, MatchArm, Module,
    OutputLiteral, Param, Pattern, Port, PortBinding, Process, Protocol, Record, RecordField,
    RecordValue, RecordValueField, ReturnExpr, Statement, TypeRef, ValueBooleanOperator,
    ValueEqualityOperator, ValueExpr, ValueScalarArithmeticOperator, ValueScalarOrderingOperator,
};
pub use authority_effect_artifact::{
    AUTHORITY_EFFECT_ARTIFACT_EXTENSION, AUTHORITY_EFFECT_HASH_ALG, AUTHORITY_EFFECT_SCHEMA_ID,
    AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR, AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR,
    AuthorityEffectAdmissionResult, AuthorityEffectAdmissionSummary,
    AuthorityEffectArtifactAdmitFormat, MAX_AUTHORITY_EFFECT_ARTIFACT_BYTES,
    RUNTIME_AUTHORITY_EFFECT_BINDING_ARTIFACT_EXTENSION,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_ID,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MAJOR,
    RUNTIME_AUTHORITY_EFFECT_BINDING_SCHEMA_VERSION_MINOR, RuntimeSpawnAuthorityPolicy,
    admit_authority_effect_artifact, render_authority_effect_admission_summary,
    render_authority_effect_artifact, render_runtime_authority_effect_binding,
};
pub use authority_summary::{AuthoritySummaryFormat, render_authority_summary};
pub use checked::CheckedProgram;
pub use checker::check_module;
pub use composition_artifact::{
    COMPONENT_COMPOSITION_ARTIFACT_EXTENSION, COMPONENT_COMPOSITION_HASH_ALG,
    COMPONENT_COMPOSITION_SCHEMA_ID, COMPONENT_COMPOSITION_SCHEMA_VERSION_MAJOR,
    COMPONENT_COMPOSITION_SCHEMA_VERSION_MINOR, ComponentCompositionAdmissionResult,
    ComponentCompositionAdmissionSummary, ComponentCompositionArtifactAdmitFormat,
    MAX_COMPONENT_COMPOSITION_ARTIFACT_BYTES, RUNTIME_COMPOSITION_BINDING_ARTIFACT_EXTENSION,
    RUNTIME_COMPOSITION_BINDING_SCHEMA_ID, RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MAJOR,
    RUNTIME_COMPOSITION_BINDING_SCHEMA_VERSION_MINOR, admit_component_composition_artifact,
    render_component_composition_admission_summary, render_component_composition_artifact,
    render_runtime_composition_binding,
};
pub use composition_report::{
    CompositionAdmissionReport, CompositionAdmissionReportFormat,
    render_composition_admission_report,
};
pub use diagnostic::{Error, Result};
pub use lowering::{lower_to_artifact, lower_to_artifact_with_source_hash};
pub use parser::parse_source;
pub use source_program::{
    ImportDependency, SourceProgram, SourceProvenanceHash, SourceUnit, SourceUnitId,
    check_source_program,
};

const STATIC_RUNTIME_DISPATCH_LIMIT: usize = 10_000;
const STATIC_RUNTIME_PROCESS_LIMIT: usize = 10_000;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_SOURCE_PROGRAM_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SOURCE_UNIT_COUNT: usize = 64;
const MAX_TOKEN_COUNT: usize = 128_000;
const MAX_TYPE_NESTING: usize = 32;
const MAX_VALUE_NESTING: usize = 32;
const PROC_RESULT_TYPE: &str = "ProcResult";
const PROCESS_REF_TYPE: &str = "ProcessRef";
const CAP_TYPE: &str = "Cap";
const SPAWN_TYPE: &str = "Spawn";
const PROTOCOL_BOUNDARY_TYPE: &str = "ProtocolBoundary";
const PORT_CONNECT_TYPE: &str = "PortConnect";
const COMPONENT_EXPORT_TYPE: &str = "ComponentExport";
const LIST_TYPE: &str = "List";
const MAP_TYPE: &str = "Map";
const UNIT_TYPE: &str = "Unit";
const OPTION_TYPE: &str = "Option";
const RESULT_TYPE: &str = "Result";
const SEND_ERROR_TYPE: &str = "SendError";
const SPAWN_ERROR_TYPE: &str = "SpawnError";
const BOOL_TYPE: &str = "Bool";
const BOOL_FALSE: &str = "False";
const BOOL_TRUE: &str = "True";

pub fn check_source(source: &str) -> Result<CheckedProgram> {
    let module = parse_source(source)?;
    if !module.imports.is_empty() {
        return Err(Error::new(
            "imports require checking from a root source path",
        ));
    }
    check_module(module)
}
