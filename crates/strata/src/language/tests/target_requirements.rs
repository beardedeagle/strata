use super::support::*;
use mantle_artifact::RuntimeFeature;

#[test]
fn lowering_declares_typed_target_requirements_for_basic_runtime_effects() {
    let source = r#"
module target_requirements_basic;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "target requirements basic";
        return Stop(state);
    }
}
"#;
    let checked = check_source(source).expect("basic source should check");
    let artifact = lower_to_artifact(&checked, source).expect("basic source should lower");

    assert_eq!(
        artifact.target_requirements.source_language.as_ref(),
        "strata"
    );
    assert_target_features(
        &artifact,
        &[
            RuntimeFeature::BoundedMailbox,
            RuntimeFeature::EmitEffect,
            RuntimeFeature::JsonlTrace,
            RuntimeFeature::LocalExecution,
        ],
    );
    assert!(
        !artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::RemoteSend)
    );
    assert!(
        !artifact
            .target_requirements
            .features
            .contains(&RuntimeFeature::TypedValueTemplates),
        "basic emit-only source should not require value templates: {:?}",
        artifact.target_requirements.features
    );
}

#[test]
fn lowering_declares_typed_requirements_for_boundary_composition() {
    let source = r#"
module target_requirements_composition;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Work }
protocol MainProtocol message MainMsg requires Cap<ProtocolBoundary<MainProtocol>>;
protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port MainPort protocol MainProtocol target Main requires Cap<PortConnect<MainPort>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;
component MainComponent exports MainPort imports WorkerPort requires Cap<ComponentExport<MainComponent>>;
composition AppComposition {
    instance main component MainComponent;
    instance worker component WorkerComponent;
    bind main imports WorkerPort -> worker exports WorkerPort;
}

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
        send worker via WorkerPort Work;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "target requirements composed";
        return Stop(state);
    }
}
"#;
    let checked = check_source(source).expect("composition source should check");
    let artifact = lower_to_artifact(&checked, source).expect("composition source should lower");

    assert_target_features(
        &artifact,
        &[
            RuntimeFeature::ComponentCompositionMetadata,
            RuntimeFeature::EmitEffect,
            RuntimeFeature::LocalSend,
            RuntimeFeature::LocalSpawn,
            RuntimeFeature::TypedBoundaryTables,
        ],
    );
}

#[test]
fn lowering_target_requirements_are_deterministic_across_source_declaration_order() {
    let main_first = r#"
module target_requirements_order_main_first;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Ping }

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

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "ordered worker";
        return Stop(state);
    }
}
"#;
    let worker_first = r#"
module target_requirements_order_worker_first;

record MainState;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Ping }

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "ordered worker";
        return Stop(state);
    }
}

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
"#;
    let main_first_features = lower_target_features(main_first);
    let worker_first_features = lower_target_features(worker_first);

    assert_eq!(main_first_features, worker_first_features);
}

fn assert_target_features(artifact: &MantleArtifact, expected: &[RuntimeFeature]) {
    for feature in expected {
        assert!(
            artifact.target_requirements.features.contains(feature),
            "target requirements should include {feature:?}: {:?}",
            artifact.target_requirements.features
        );
    }
}

fn lower_target_features(source: &str) -> Vec<RuntimeFeature> {
    let checked = check_source(source).expect("source should check");
    let artifact = lower_to_artifact(&checked, source).expect("source should lower");
    artifact.target_requirements.features
}
