use super::super::{
    AUTHORITY_EFFECT_HASH_ALG, AUTHORITY_EFFECT_SCHEMA_ID, AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR,
    AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR, AuthorityEffectAdmissionResult,
    AuthorityEffectArtifactAdmitFormat, RuntimeSpawnAuthorityPolicy, SourceProvenanceHash,
    admit_authority_effect_artifact, check_source, lower_to_artifact,
    render_authority_effect_admission_summary, render_authority_effect_artifact,
    render_runtime_authority_effect_binding,
};

const SOURCE: &str = r#"
module authority_effect_binding;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerPort>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerPort Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

const LOCAL_SUPERVISION_SOURCE: &str = r#"
module authority_effect_local_supervision;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Crash }

proc Main mailbox bounded(2) {
    type State = MainState;
    type Msg = MainMsg;

    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child worker: Worker = spawn Worker as permanent;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [send] ~ [] @det {
        send worker Crash;
        return Continue(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Crash) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Panic(state);
    }
}
"#;

#[test]
fn artifact_emits_required_identity_fields_and_admits() {
    let artifact = example_artifact();
    let summary = admit_authority_effect_artifact(&artifact)
        .expect("rendered authority/effect artifact should admit");

    assert!(artifact.contains(&format!("\"schema_id\":\"{AUTHORITY_EFFECT_SCHEMA_ID}\"")));
    assert!(artifact.contains(&format!("\"hash_alg\":\"{AUTHORITY_EFFECT_HASH_ALG}\"")));
    assert!(artifact.contains("\"artifact_kind\":\"checked_authority_effects\""));
    assert!(artifact.contains("\"processes\":[{"));
    assert!(artifact.contains("\"protocol_count\":1"));
    assert!(artifact.contains("\"port_count\":1"));
    assert!(artifact.contains("\"component_count\":1"));
    assert!(artifact.contains("\"state_count\":1"));
    assert!(artifact.contains("\"message_count\":1"));
    assert!(artifact.contains("\"authority_id\":0"));
    assert!(artifact.contains("\"spawn_site_id\":0"));
    assert!(artifact.contains("\"transition_effects\":[{"));
    assert!(artifact.contains("\"effect\":\"spawn\""));
    assert!(artifact.contains("\"component_authority_surfaces\":[{"));
    assert!(artifact.contains("\"import_port_count\":0"));
    assert_eq!(summary.schema_id, AUTHORITY_EFFECT_SCHEMA_ID);
    assert_eq!(
        summary.schema_version_major,
        AUTHORITY_EFFECT_SCHEMA_VERSION_MAJOR
    );
    assert_eq!(
        summary.schema_version_minor,
        AUTHORITY_EFFECT_SCHEMA_VERSION_MINOR
    );
    assert_eq!(summary.protocol_count, 1);
    assert_eq!(summary.port_count, 1);
    assert_eq!(summary.process_count, 2);
    assert_eq!(summary.component_count, 1);
    assert_eq!(summary.authority_count, 2);
    assert_eq!(summary.spawn_site_count, 1);
    assert_eq!(summary.transition_effect_count, 2);
    assert_eq!(summary.component_authority_surface_count, 1);
    assert_eq!(
        summary.admission_result,
        AuthorityEffectAdmissionResult::Admitted
    );
}

#[test]
fn artifact_emits_supervisor_spawn_facts_and_admits() {
    let artifact = local_supervision_artifact();
    let summary = admit_authority_effect_artifact(&artifact)
        .expect("local supervision authority/effect artifact should admit");

    assert!(artifact.contains("\"kind\":\"lexical_supervisor_child\""));
    assert!(artifact.contains("\"supervisor_spawn_facts\":[{"));
    assert!(artifact.contains("\"supervisor_id\":0"));
    assert!(artifact.contains("\"child_id\":0"));
    assert!(artifact.contains("\"target_process_id\":1"));
    assert!(artifact.contains("\"spawn_site_id\":0"));
    assert_eq!(summary.process_count, 2);
    assert_eq!(summary.spawn_site_count, 1);
}

#[test]
fn admission_summary_renders_text_and_json() {
    let artifact = example_artifact();
    let summary = admit_authority_effect_artifact(&artifact)
        .expect("rendered authority/effect artifact should admit");
    let text = render_authority_effect_admission_summary(
        &summary,
        "target/strata/authority_effect_binding.authority-effect.json",
        AuthorityEffectArtifactAdmitFormat::Text,
    );
    let json = render_authority_effect_admission_summary(
        &summary,
        "target/strata/authority_effect_binding.authority-effect.json",
        AuthorityEffectArtifactAdmitFormat::Json,
    );

    assert!(text.contains("admission_result: admitted"));
    assert!(text.contains("ports: 1"));
    assert!(text.contains("transition_effects: 2"));
    assert!(json.contains(&format!("\"schema_id\":\"{AUTHORITY_EFFECT_SCHEMA_ID}\"")));
    assert!(json.contains("\"port_count\":1"));
    assert!(json.contains("\"component_authority_surface_count\":1"));
}

#[test]
fn runtime_binding_matches_runtime_artifact_and_removes_labels() {
    let (authority_effect, artifact) = authority_effect_and_runtime_artifact();
    let binding = render_runtime_authority_effect_binding(
        &authority_effect,
        &artifact,
        RuntimeSpawnAuthorityPolicy::DenyDeclared,
    )
    .expect("authority/effect artifact should bind to matching runtime artifact");

    assert!(binding.contains("\"schema_id\":\"mantle.runtime_authority_effect_binding\""));
    assert!(binding.contains("\"deployment_id\":0"));
    assert!(
        binding.contains("\"authority_effect_schema_id\":\"strata.checked_authority_effects\"")
    );
    assert!(binding.contains("\"spawn_authority_policy\":\"deny_declared\""));
    assert!(binding.contains("\"transition_effects\":[{"));
    assert!(
        !binding.contains("\"process\":"),
        "runtime binding must not carry process labels as executable bindings: {binding}"
    );
    assert!(
        !binding.contains("\"target_process\":"),
        "runtime binding must not carry target labels as executable bindings: {binding}"
    );
    assert!(
        !binding.contains("\"port\":"),
        "runtime binding must not carry port labels as executable bindings: {binding}"
    );
}

#[test]
fn runtime_binding_rejects_mismatched_artifact_identity() {
    let (authority_effect, mut artifact) = authority_effect_and_runtime_artifact();
    artifact.source_hash_fnv1a64 = "1111111111111111".to_string();
    let err = render_runtime_authority_effect_binding(
        &authority_effect,
        &artifact,
        RuntimeSpawnAuthorityPolicy::AdmitDeclared,
    )
    .expect_err("mismatched .mta identity must fail closed");

    assert!(
        err.to_string()
            .contains("source fingerprint does not match"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_forged_widened_authority() {
    let (authority_effect, artifact) = authority_effect_and_runtime_artifact();
    let forged = authority_effect
        .replace(
            "{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}",
            "{\"kind\":\"spawn\",\"target_process_id\":0,\"target_process\":\"Main\"}",
        )
        .replace(
            "\"kind\":\"dynamic_local\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":0",
            "\"kind\":\"dynamic_local\",\"target_process_id\":0,\"target_process\":\"Main\",\"authority_id\":0",
        );
    let err = render_runtime_authority_effect_binding(
        &forged,
        &artifact,
        RuntimeSpawnAuthorityPolicy::AdmitDeclared,
    )
    .expect_err("forged widened authority must fail closed");

    assert!(
        err.to_string().contains("descriptor does not match"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn runtime_binding_rejects_forged_transition_effect_order() {
    let (authority_effect, artifact) = authority_effect_and_runtime_artifact();
    let forged = authority_effect.replace(
        "\"effects\":[{\"effect_id\":0,\"effect\":\"spawn\"},{\"effect_id\":1,\"effect\":\"send\"}]",
        "\"effects\":[{\"effect_id\":0,\"effect\":\"send\"},{\"effect_id\":1,\"effect\":\"spawn\"}]",
    );
    admit_authority_effect_artifact(&forged)
        .expect("structural admission does not re-check source-derived effect facts");
    let err = render_runtime_authority_effect_binding(
        &forged,
        &artifact,
        RuntimeSpawnAuthorityPolicy::AdmitDeclared,
    )
    .expect_err("forged transition effect facts must fail closed at runtime binding");

    assert!(
        err.to_string()
            .contains("transition_id 0 effects do not match runtime artifact"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn admission_rejects_unsupported_non_empty_policy_inputs() {
    assert_rejects(
        &example_artifact().replace("\"policy_inputs\":[]", "\"policy_inputs\":[{}]"),
        "field \"policy_inputs\" is not implemented",
    );
}

#[test]
fn admission_rejects_noncanonical_transition_effect_order() {
    assert_rejects(
        &example_artifact().replacen("\"effect_id\":0", "\"effect_id\":1", 1),
        "effect_id 1 at array index 0 is not canonical",
    );
}

#[test]
fn admission_rejects_unknown_transition_message_id() {
    assert_rejects(
        &example_artifact().replacen("\"message_id\":0", "\"message_id\":99", 1),
        "references unknown message id 99",
    );
}

#[test]
fn admission_rejects_unknown_current_state_id() {
    assert_rejects(
        &example_artifact().replacen("\"current_state_id\":null", "\"current_state_id\":99", 1),
        "references unknown state id 99",
    );
}

#[test]
fn admission_rejects_dynamic_spawn_site_without_authority() {
    assert_rejects(
        &example_artifact().replace(
            "\"target_process\":\"Worker\",\"authority_id\":0,\"supervisor_id\":null",
            "\"target_process\":\"Worker\",\"authority_id\":null,\"supervisor_id\":null",
        ),
        "dynamic_local spawn_site id 0 must carry authority_id",
    );
}

#[test]
fn admission_rejects_dynamic_spawn_site_with_supervisor_ids() {
    assert_rejects(
        &example_artifact().replace(
            "\"authority_id\":0,\"supervisor_id\":null,\"supervisor_child_id\":null",
            "\"authority_id\":0,\"supervisor_id\":0,\"supervisor_child_id\":0",
        ),
        "dynamic_local spawn_site id 0 must not carry supervisor ids",
    );
}

#[test]
fn admission_rejects_lexical_supervisor_child_with_dynamic_authority() {
    assert_rejects(
        &example_artifact().replacen(
            "\"kind\":\"dynamic_local\"",
            "\"kind\":\"lexical_supervisor_child\"",
            1,
        ),
        "lexical_supervisor_child spawn_site id 0 must not carry authority_id",
    );
}

#[test]
fn admission_rejects_lexical_supervisor_child_without_supervisor_ids() {
    assert_rejects(
        &example_artifact().replace(
            "\"kind\":\"dynamic_local\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":0,\"supervisor_id\":null,\"supervisor_child_id\":null",
            "\"kind\":\"lexical_supervisor_child\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":null,\"supervisor_id\":null,\"supervisor_child_id\":null",
        ),
        "lexical_supervisor_child spawn_site id 0 must carry supervisor_id",
    );
}

#[test]
fn admission_rejects_lexical_supervisor_child_without_matching_supervisor_fact() {
    assert_rejects(
        &example_artifact().replace(
            "\"kind\":\"dynamic_local\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":0,\"supervisor_id\":null,\"supervisor_child_id\":null",
            "\"kind\":\"lexical_supervisor_child\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":null,\"supervisor_id\":0,\"supervisor_child_id\":0",
        ),
        "references unknown supervisor id 0",
    );
}

#[test]
fn admission_rejects_lexical_supervisor_child_supervisor_outside_bounds() {
    assert_rejects(
        &example_artifact().replace(
            "\"kind\":\"dynamic_local\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":0,\"supervisor_id\":null,\"supervisor_child_id\":null",
            "\"kind\":\"lexical_supervisor_child\",\"target_process_id\":1,\"target_process\":\"Worker\",\"authority_id\":null,\"supervisor_id\":99,\"supervisor_child_id\":0",
        ),
        "references unknown supervisor id 99",
    );
}

#[test]
fn admission_rejects_supervisor_child_spawn_site_backlink_mismatch() {
    assert_rejects(
        &local_supervision_artifact().replace(
            "\"child_id\":0,\"child\":\"worker\",\"target_process_id\":1,\"target_process\":\"Worker\",\"spawn_site_id\":0",
            "\"child_id\":0,\"child\":\"worker\",\"target_process_id\":1,\"target_process\":\"Worker\",\"spawn_site_id\":1",
        ),
        "declared spawn_site_id 1 for supervisor child",
    );
}

#[test]
fn admission_rejects_spawn_site_authority_retargeting() {
    assert_rejects(
        &example_artifact().replace(
            "{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}",
            "{\"kind\":\"spawn\",\"target_process_id\":0,\"target_process\":\"Main\"}",
        ),
        "dynamic_local spawn_site id 0 targets process id 1, but authority_id 0 targets 0",
    );
}

#[test]
fn admission_rejects_unknown_protocol_descriptor_id() {
    assert_rejects(
        &example_artifact().replacen(
            "{\"kind\":\"port_connect\",\"port_id\":0,\"port\":\"WorkerPort\"}",
            "{\"kind\":\"protocol_boundary\",\"protocol_id\":99,\"protocol\":\"WorkerProtocol\"}",
            1,
        ),
        "references unknown protocol id 99",
    );
}

#[test]
fn admission_rejects_unknown_port_descriptor_id() {
    assert_rejects(
        &example_artifact().replacen(
            "\"port_id\":0,\"port\":\"WorkerPort\"",
            "\"port_id\":99,\"port\":\"WorkerPort\"",
            1,
        ),
        "references unknown port id 99",
    );
}

#[test]
fn admission_rejects_component_authority_retargeting() {
    assert_rejects(
        &example_artifact().replace(
            "{\"kind\":\"component_export\",\"component_id\":0,\"component\":\"WorkerComponent\"}",
            "{\"kind\":\"component_export\",\"component_id\":1,\"component\":\"WorkerComponent\"}",
        ),
        "references unknown component id 1",
    );
}

#[test]
fn admission_rejects_unknown_component_descriptor_id() {
    assert_rejects(
        &example_artifact().replace(
            "{\"kind\":\"component_export\",\"component_id\":0,\"component\":\"WorkerComponent\"}",
            "{\"kind\":\"component_export\",\"component_id\":99,\"component\":\"WorkerComponent\"}",
        ),
        "references unknown component id 99",
    );
}

#[test]
fn admission_rejects_duplicate_import_port_authority() {
    assert_rejects(
        &component_import_artifact().replace(
            "\"import_port_count\":1,\"component_authority\"",
            "\"import_port_count\":2,\"component_authority\"",
        )
        .replace(
            "\"import_port_authorities\":[{\"port_id\":1,\"port\":\"WorkerPort\",\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":1,\"port\":\"WorkerPort\"}}]",
            "\"import_port_authorities\":[{\"port_id\":1,\"port\":\"WorkerPort\",\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":1,\"port\":\"WorkerPort\"}},{\"port_id\":1,\"port\":\"WorkerPort\",\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":1,\"port\":\"WorkerPort\"}}]",
        ),
        "component surface imports port id 1 more than once",
    );
}

#[test]
fn admission_rejects_missing_declared_component_import_authority() {
    let artifact = component_import_artifact();
    assert!(
        artifact.contains("\"import_port_count\":1"),
        "component import source should declare one imported port authority: {artifact}"
    );

    assert_rejects(
        &artifact.replace(
            "\"import_port_authorities\":[{\"port_id\":1,\"port\":\"WorkerPort\",\"port_authority\":{\"kind\":\"port_connect\",\"port_id\":1,\"port\":\"WorkerPort\"}}]",
            "\"import_port_authorities\":[]",
        ),
        "component_import_port_count 0 does not match declared count 1",
    );
}

fn assert_rejects(artifact: &str, expected: &str) {
    let err = admit_authority_effect_artifact(artifact)
        .expect_err("forged authority/effect artifact should fail closed");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn example_artifact() -> String {
    let checked = check_source(SOURCE).expect("authority/effect source should check");
    let source_hash = SourceProvenanceHash::from_source(SOURCE);
    render_authority_effect_artifact(&checked, "authority_effect_binding.str", &source_hash)
        .expect("authority/effect artifact should render")
}

fn component_import_artifact() -> String {
    let source = r#"
module authority_effect_component_import;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Work }

protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component MainComponent exports MainPort imports WorkerPort requires Cap<ComponentExport<MainComponent>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;
    let checked = check_source(source).expect("component import source should check");
    let source_hash = SourceProvenanceHash::from_source(source);
    render_authority_effect_artifact(
        &checked,
        "authority_effect_component_import.str",
        &source_hash,
    )
    .expect("component import authority/effect artifact should render")
}

fn local_supervision_artifact() -> String {
    let checked =
        check_source(LOCAL_SUPERVISION_SOURCE).expect("local supervision source should check");
    let source_hash = SourceProvenanceHash::from_source(LOCAL_SUPERVISION_SOURCE);
    render_authority_effect_artifact(
        &checked,
        "authority_effect_local_supervision.str",
        &source_hash,
    )
    .expect("local supervision authority/effect artifact should render")
}

fn authority_effect_and_runtime_artifact() -> (String, mantle_artifact::MantleArtifact) {
    let checked = check_source(SOURCE).expect("authority/effect source should check");
    let source_hash = SourceProvenanceHash::from_source(SOURCE);
    let authority_effect =
        render_authority_effect_artifact(&checked, "authority_effect_binding.str", &source_hash)
            .expect("authority/effect artifact should render");
    let artifact = lower_to_artifact(&checked, SOURCE).expect("source should lower");
    (authority_effect, artifact)
}
