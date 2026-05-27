use super::support::*;

#[test]
fn parses_step_return_type_as_structured_type_ref() {
    let module = parse_source(HELLO).expect("hello should parse");
    let steps = &module.processes[0].steps;
    assert_eq!(steps.len(), 1);
    let step = &steps[0];

    assert_eq!(
        &step.return_type,
        &TypeRef::Applied {
            constructor: Identifier::new(PROC_RESULT_TYPE).expect("ProcResult identifier"),
            args: vec![TypeRef::Named(
                Identifier::new("MainState").expect("MainState identifier")
            )],
            const_args: Vec::new(),
        }
    );
    assert_eq!(
        step.params,
        [
            FunctionParam::Binding(Param {
                name: Identifier::new("state").expect("state identifier"),
                ty: TypeRef::Named(Identifier::new("MainState").expect("MainState identifier")),
            }),
            FunctionParam::Pattern(Pattern::Constructor {
                name: Identifier::new("Start").expect("Start identifier"),
                payload: None,
            }),
        ]
    );
}

#[test]
fn parses_and_checks_wildcard_step_pattern() {
    let source = r#"
module actor_catchall;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, SawFirst }
enum WorkerMsg { First, Second, Third }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Second;
        return Stop(state);
    }
}

proc Worker mailbox bounded(3) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;
    let module = parse_source(source).expect("wildcard step pattern should parse");
    let worker = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Worker")
        .expect("Worker should parse");
    assert_eq!(
        worker.steps[1].params[1],
        FunctionParam::Pattern(Pattern::Wildcard)
    );

    let checked = check_module(module).expect("wildcard step pattern should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(worker.transitions().len(), 3);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(
        worker.transitions()[0].next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].next_state(),
        CheckedNextState::Current
    );
    assert_eq!(worker.transitions()[2].message(), checked_message_id(2));
    assert_eq!(
        worker.transitions()[2].next_state(),
        CheckedNextState::Current
    );

    let artifact =
        lower_to_artifact(&checked, source).expect("wildcard should lower to typed transitions");
    let worker_artifact = &artifact.processes[1];
    assert_eq!(worker_artifact.transitions.len(), 3);
    assert_eq!(
        worker_artifact.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        worker_artifact.transitions[1].message,
        mantle_artifact::MessageId::new(1)
    );
    assert_eq!(
        worker_artifact.transitions[2].message,
        mantle_artifact::MessageId::new(2)
    );
}

#[test]
fn checks_subset_map_payload_patterns_in_step_signature_and_match_body() {
    let source = r#"
module subset_map_payload_patterns;

enum Phase {
    Ready,
    Done,
}
record MainState;
record WorkerState {
    phase: Phase,
}
enum MainMsg {
    Start,
}
enum WorkerMsg {
    Lookup(Map<Phase,Phase,2>),
}

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority spawn_body_worker: Cap<Spawn<BodyWorker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let body_worker: ProcessRef<BodyWorker> = spawn BodyWorker;
        send worker Lookup(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
        send body_worker Lookup(Map<Phase,Phase,2>[Ready => Done, Done => Ready]);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState { phase: Ready };
    }

    fn step(state: WorkerState, Lookup(Map[Ready => phase, ..])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(WorkerState { phase: phase });
    }
}

proc BodyWorker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState { phase: Ready };
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match msg {
            Lookup(Map[Ready => phase, ..]) => {
                return Stop(WorkerState { phase: phase });
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("subset map step patterns should check");
    let artifact = lower_to_artifact(&checked, source).expect("subset map steps should lower");

    for process_name in ["Worker", "BodyWorker"] {
        let process = checked
            .processes()
            .iter()
            .find(|process| process.debug_name().as_str() == process_name)
            .unwrap_or_else(|| panic!("{process_name} should be checked"));
        assert_eq!(
            checked_state_labels(process),
            ["WorkerState{phase:Ready}", "WorkerState{phase:Done}"]
        );
    }

    for process_name in ["Worker", "BodyWorker"] {
        let process = artifact
            .processes
            .iter()
            .find(|process| process.debug_name == process_name)
            .unwrap_or_else(|| panic!("{process_name} should lower"));
        let mantle_artifact::NextState::Template(ArtifactValueTemplate::Record { fields, .. }) =
            &process.transitions[0].next_state
        else {
            panic!("{process_name} should lower next state to a record template");
        };
        let phase = fields
            .iter()
            .find(|field| field.name == "phase")
            .unwrap_or_else(|| panic!("{process_name} should template phase"));
        let ArtifactValueTemplate::MapValue {
            key,
            keys,
            projection,
            ..
        } = &phase.value
        else {
            panic!("{process_name} should project phase from a map payload");
        };
        assert_eq!(key, &artifact_value("Ready"));
        assert_eq!(keys.as_slice(), [artifact_value("Ready")]);
        assert_eq!(*projection, mantle_artifact::MapProjectionMode::Subset);
    }
}

#[test]
fn checks_wildcard_only_step_pattern() {
    let source = HELLO.replace(
        "fn step(state: MainState, Start)",
        "fn step(state: MainState, _)",
    );

    let checked = check_source(&source).expect("wildcard-only step pattern should check");
    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");

    assert_eq!(main.transitions().len(), 1);
    assert_eq!(main.transitions()[0].message(), checked_message_id(0));
}

#[test]
fn parses_checks_and_lowers_match_step_body() {
    let source = r#"
module actor_match;

record MainState;
enum MainMsg { Start }
enum WorkerState { Waiting, SawFirst, Done }
enum WorkerMsg { First, Second }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker First;
        send worker Second;
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Waiting;
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            First => {
                emit "worker matched First";
                return Continue(SawFirst);
            }
            Second => {
                emit "worker matched Second";
                return Stop(Done);
            }
        }
    }
}
"#;

    let module = parse_source(source).expect("match source should parse");
    let worker = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Worker")
        .expect("Worker should parse");
    let Some(FunctionBody::Match(match_body)) = &worker.steps[0].body else {
        panic!("Worker step should parse as a match body");
    };
    assert_eq!(match_body.scrutinee.as_str(), "msg");
    assert_eq!(match_body.arms.len(), 2);

    let checked = check_module(module).expect("match source should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(
        worker.transitions()[0].next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].next_state(),
        CheckedNextState::Value(checked_state_id(2))
    );
    assert_eq!(
        checked.outputs(),
        ["worker matched First", "worker matched Second"]
    );

    let artifact = lower_to_artifact(&checked, source).expect("match should lower");
    let worker_artifact = &artifact.processes[1];
    assert_eq!(worker_artifact.transitions.len(), 2);
    assert_eq!(
        worker_artifact.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        worker_artifact.transitions[1].message,
        mantle_artifact::MessageId::new(1)
    );
}

#[test]
fn match_step_body_accepts_wildcard_arm() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled First";
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            First => {
                emit "worker handled First";
                return Continue(SawFirst);
            }
            _ => {
                emit "worker handled Second";
                return Stop(Done);
            }
        }
    }"#,
    );

    let checked = check_source(&source).expect("match wildcard should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");

    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(
        worker.transitions()[0].next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].next_state(),
        CheckedNextState::Value(checked_state_id(2))
    );

    let artifact = lower_to_artifact(&checked, &source).expect("wildcard match should lower");
    let worker_artifact = &artifact.processes[1];
    assert_eq!(worker_artifact.transitions.len(), 2);
    assert_eq!(
        worker_artifact.transitions[0].effects,
        [ArtifactEffect::Emit]
    );
    assert_eq!(
        worker_artifact.transitions[1].effects,
        [ArtifactEffect::Emit]
    );
}

#[test]
fn match_step_body_binds_payload_immutably() {
    let source = r#"
module actor_match_payloads;

record MainState;
record Job { phase: JobPhase }
record WorkerState { job: Job }
enum MainMsg { Start }
enum JobPhase { Ready, Done }
enum WorkerMsg { Assign(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState { job: Job { phase: Done } };
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match msg {
            Assign(job: Job) => {
                return Stop(WorkerState { job: job });
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("payload match should check");
    let worker = &checked.processes()[1];

    assert_eq!(
        checked_state_labels(worker),
        [
            "WorkerState{job:Job{phase:Done}}",
            "WorkerState{job:Job{phase:Ready}}"
        ]
    );
    assert!(matches!(
        only_transition(worker).next_state(),
        CheckedNextState::Template(_)
    ));
}

#[test]
fn step_signature_destructures_record_list_and_map_payloads() {
    let source = r#"
module step_signature_payload_destructuring;

record MainState;
record WorkerState { seen: Phase }
record Job { phase: Phase }
enum Phase { Ready, Done }
enum MainMsg { Start }
enum WorkerMsg {
    Assign(Job),
    Items(List<Phase,2>),
    Lookup(Map<Phase,Phase,1>),
    Finish,
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
        send worker Assign(Job { phase: Ready });
        send worker Items(List<Phase,2>[Done, Ready]);
        send worker Lookup(Map<Phase,Phase,1>[Ready => Done]);
        send worker Finish;
        return Stop(state);
    }
}

proc Worker mailbox bounded(4) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState { seen: Ready };
    }

    fn step(state: WorkerState, Assign(Job { phase })) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(WorkerState { seen: phase });
    }

    fn step(state: WorkerState, Items(List[phase, _])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(WorkerState { seen: phase });
    }

    fn step(state: WorkerState, Lookup(Map[Ready => phase])) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(WorkerState { seen: phase });
    }

    fn step(state: WorkerState, Finish) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("step signatures should destructure payloads");
    let worker = &checked.processes()[1];

    assert_eq!(
        checked_state_labels(worker),
        ["WorkerState{seen:Ready}", "WorkerState{seen:Done}"]
    );
}

#[test]
fn match_step_body_destructures_record_list_and_map_payloads() {
    let source = r#"
module match_step_payload_destructuring;

record MainState;
record WorkerState { seen: Phase }
record Job { phase: Phase }
enum Phase { Ready, Done }
enum MainMsg { Start }
enum WorkerMsg {
    Assign(Job),
    Items(List<Phase,2>),
    Lookup(Map<Phase,Phase,1>),
    Finish,
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
        send worker Assign(Job { phase: Ready });
        send worker Items(List<Phase,2>[Done, Ready]);
        send worker Lookup(Map<Phase,Phase,1>[Ready => Done]);
        send worker Finish;
        return Stop(state);
    }
}

proc Worker mailbox bounded(4) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState { seen: Ready };
    }

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match msg {
            Assign(Job { phase }) => {
                return Continue(WorkerState { seen: phase });
            }
            Items(List[phase, _]) => {
                return Continue(WorkerState { seen: phase });
            }
            Lookup(Map[Ready => phase]) => {
                return Continue(WorkerState { seen: phase });
            }
            Finish => {
                return Stop(state);
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("match step should destructure payloads");
    let worker = &checked.processes()[1];

    assert_eq!(
        checked_state_labels(worker),
        ["WorkerState{seen:Ready}", "WorkerState{seen:Done}"]
    );
}

#[test]
fn state_match_destructures_record_list_and_map_payloads() {
    let source = r#"
module state_match_payload_destructuring;

record MainState;
record Job { phase: Phase }
enum Phase { Ready, Done }
enum WorkerState {
    Holding(Job),
    Listed(List<Phase,1>),
    Mapped(Map<Phase,Phase,1>),
}
enum MainMsg { Start }
enum WorkerMsg { Advance }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Advance;
        send worker Advance;
        send worker Advance;
        return Stop(state);
    }
}

proc Worker mailbox bounded(3) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Holding(Job { phase: Ready });
    }

    fn step(state: WorkerState, Advance) -> ProcResult<WorkerState> ! [] ~ [] @det {
        match state {
            Holding(Job { phase }) => {
                return Continue(Listed(List<Phase,1>[phase]));
            }
            Listed(List[phase]) => {
                return Continue(Mapped(Map<Phase,Phase,1>[Ready => phase]));
            }
            Mapped(Map[Ready => phase]) => {
                return Stop(Holding(Job { phase: phase }));
            }
        }
    }
}
"#;

    let checked = check_source(source).expect("state match should destructure payloads");
    let worker = &checked.processes()[1];

    assert_eq!(
        checked_state_labels(worker),
        [
            "Holding(Job{phase:Ready})",
            "Listed(List[Ready])",
            "Mapped(Map[Ready=>Ready])",
        ]
    );
}
