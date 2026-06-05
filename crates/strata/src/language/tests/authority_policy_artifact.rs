use super::super::{
    AuthorityPolicyBuildOptions, SourceProvenanceHash, admit_authority_policy_artifact,
    check_source, render_authority_effect_artifact, render_authority_policy_artifact,
};

const SOURCE: &str = r#"
module authority_policy_binding;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;

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

const BOUNDED_TABLE_SOURCE: &str = r#"
module authority_policy_bounded_table;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }
enum SinkState { Idle }
enum SinkMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    authority spawn_sink: Cap<Spawn<Sink>>;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Done;
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

#[test]
fn policy_admission_rejects_noncanonical_decision_id() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect).replacen(
        "\"decision_id\":0,\"process_id\":0",
        "\"decision_id\":1,\"process_id\":0",
        1,
    );

    assert_policy_rejects(
        policy,
        &authority_effect,
        "decision_id 1 at array index 0 is not canonical",
    );
}

#[test]
fn policy_admission_rejects_duplicate_authority_ordering() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect).replace(
        ",{\"decision_id\":1,\"process_id\":0,\"authority_id\":1,\"descriptor\":{\"kind\":\"port_connect\",\"port_id\":0},\"decision\":\"admit\"}",
        ",{\"decision_id\":1,\"process_id\":0,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1},\"decision\":\"admit\"}",
    );

    assert_policy_rejects(
        policy,
        &authority_effect,
        "decision table is not closed over checked authorities at decision_id 1",
    );
}

#[test]
fn policy_admission_rejects_unknown_process_id() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect).replacen(
        "\"decision_id\":0,\"process_id\":0",
        "\"decision_id\":0,\"process_id\":99",
        1,
    );

    assert_policy_rejects(
        policy,
        &authority_effect,
        "references unknown process id 99",
    );
}

#[test]
fn policy_admission_rejects_unknown_authority_id() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect).replacen(
        "\"process_id\":0,\"authority_id\":0",
        "\"process_id\":0,\"authority_id\":99",
        1,
    );

    assert_policy_rejects(
        policy,
        &authority_effect,
        "references unknown authority id 99",
    );
}

#[test]
fn policy_admission_rejects_unsupported_decision() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect).replacen(
        "\"decision\":\"admit\"",
        "\"decision\":\"grant\"",
        1,
    );

    assert_policy_rejects(
        policy,
        &authority_effect,
        "unsupported authority policy decision \"grant\"",
    );
}

#[test]
fn policy_admission_rejects_schema_identity_mismatch() {
    let authority_effect = example_artifact();
    let policy = example_policy(&authority_effect);
    let cases = [
        (
            policy.replacen(
                "\"schema_id\":\"strata.authority_policy_decisions\"",
                "\"schema_id\":\"foreign.authority_policy_decisions\"",
                1,
            ),
            "field \"schema_id\" must be",
        ),
        (
            policy.replacen(
                "\"artifact_kind\":\"authority_policy_decisions\"",
                "\"artifact_kind\":\"checked_authority_effects\"",
                1,
            ),
            "field \"artifact_kind\" must be",
        ),
        (
            policy.replacen(
                "\"schema_version_major\":1",
                "\"schema_version_major\":99",
                1,
            ),
            "field \"schema_version_major\" must be 1, got 99",
        ),
        (
            policy.replacen(
                "\"authority_effect_schema_id\":\"strata.checked_authority_effects\"",
                "\"authority_effect_schema_id\":\"foreign.checked_authority_effects\"",
                1,
            ),
            "field \"authority_effect_schema_id\" must be",
        ),
    ];

    for (forged, expected) in cases {
        assert_policy_rejects(forged, &authority_effect, expected);
    }
}

#[test]
fn bounded_policy_decision_table_requires_canonical_process_authority_order() {
    let authority_effect = bounded_table_artifact();
    let policy = example_policy(&authority_effect);
    assert!(
        policy.contains(
            "\"decision_id\":0,\"process_id\":0,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1}"
        ),
        "policy should include process 0 authority 0: {policy}"
    );
    assert!(
        policy.contains(
            "\"decision_id\":1,\"process_id\":1,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":2}"
        ),
        "policy should include process 1 authority 0: {policy}"
    );

    let forged_cases = [
        (
            policy.replace(
                ",{\"decision_id\":1,\"process_id\":1,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":2},\"decision\":\"admit\"}",
                "",
            ),
            "decision count 1 does not match checked authority count 2",
        ),
        (
            policy.replace(
                "{\"decision_id\":1,\"process_id\":1,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":2},\"decision\":\"admit\"}",
                "{\"decision_id\":1,\"process_id\":0,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1},\"decision\":\"admit\"}",
            ),
            "decision table is not closed over checked authorities at decision_id 1",
        ),
        (
            policy.replacen(
                "\"decision_id\":1,\"process_id\":1,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":2}",
                "\"decision_id\":1,\"process_id\":1,\"authority_id\":0,\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":0}",
                1,
            ),
            "descriptor does not match checked authority/effect facts",
        ),
    ];

    for (forged, expected) in forged_cases {
        assert_policy_rejects(forged, &authority_effect, expected);
    }
}

fn assert_policy_rejects(policy: String, authority_effect: &str, expected: &str) {
    let err = admit_authority_policy_artifact(&policy, authority_effect)
        .expect_err("forged authority policy artifact should fail closed");
    assert!(
        err.to_string().contains(expected),
        "expected {expected:?}, got {err}"
    );
}

fn example_policy(authority_effect: &str) -> String {
    render_authority_policy_artifact(authority_effect, AuthorityPolicyBuildOptions::default())
        .expect("authority policy artifact should render")
}

fn example_artifact() -> String {
    let checked = check_source(SOURCE).expect("authority policy source should check");
    let source_hash = SourceProvenanceHash::from_source(SOURCE);
    render_authority_effect_artifact(&checked, "authority_policy_binding.str", &source_hash)
        .expect("authority/effect artifact should render")
}

fn bounded_table_artifact() -> String {
    let checked = check_source(BOUNDED_TABLE_SOURCE)
        .expect("bounded policy decision table source should check");
    let source_hash = SourceProvenanceHash::from_source(BOUNDED_TABLE_SOURCE);
    render_authority_effect_artifact(&checked, "authority_policy_bounded_table.str", &source_hash)
        .expect("bounded authority/effect artifact should render")
}
