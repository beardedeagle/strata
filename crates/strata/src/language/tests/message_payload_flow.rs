use super::support::*;

#[test]
fn parses_checks_and_lowers_message_payload_step_binding() {
    let source = r#"
module actor_payloads;

record MainState;
record Job { phase: JobPhase }
record WorkerState { job: Job }
enum MainMsg { Start }
enum JobPhase { Ready, Done }
enum WorkerMsg { Assign(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

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

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(WorkerState { job: job });
    }
}
"#;

    let module = parse_source(source).expect("payload source should parse");
    let worker = module
        .processes
        .iter()
        .find(|process| process.name.as_str() == "Worker")
        .expect("Worker should parse");
    assert_eq!(
        worker.steps[0].params[1],
        FunctionParam::Pattern(Pattern::Constructor {
            name: Identifier::new("Assign").expect("Assign identifier"),
            payload: Some(ConstructorPayloadPattern::Binding(Param {
                name: Identifier::new("job").expect("job identifier"),
                ty: TypeRef::Named(Identifier::new("Job").expect("Job identifier")),
            })),
        })
    );

    let checked = check_module(module).expect("payload source should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        worker
            .message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign"]
    );
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

    let artifact = lower_to_artifact(&checked, source).expect("payload source should lower");
    let job = artifact_type_id(&artifact, "Job");
    assert_eq!(
        artifact.processes[1].message_variants,
        [ArtifactMessageVariant::payload("Assign", job)]
    );
    assert_eq!(
        artifact_state_labels(&artifact.processes[1]),
        [
            "WorkerState{job:Job{phase:Done}}",
            "WorkerState{job:Job{phase:Ready}}"
        ]
    );
}

#[test]
fn uses_one_payload_message_case_for_multiple_payload_values() {
    let source = r#"
module actor_payload_cases;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum WorkerState { Idle, ReadySeen, DoneSeen }
enum MainMsg { Start }
enum WorkerMsg { Assign(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        send worker Assign(Job { phase: Done });
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;

    let checked = check_source(source).expect("multiple payload sends should check");
    let worker = &checked.processes()[1];

    assert_eq!(
        worker
            .message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign"]
    );
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
    assert_eq!(worker.transitions().len(), 1);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
}

#[test]
fn wildcard_step_pattern_handles_payload_messages_without_binding() {
    let source = r#"
module actor_payload_wildcard;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum WorkerState { Idle }
enum MainMsg { Start }
enum WorkerMsg { Assign(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job { phase: Ready });
        send worker Assign(Job { phase: Done });
        return Stop(state);
    }
}

proc Worker mailbox bounded(2) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
}
"#;

    let checked = check_source(source).expect("wildcard payload handler should check");
    let worker = &checked.processes()[1];

    assert_eq!(
        worker
            .message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign"]
    );
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
    assert_eq!(worker.transitions().len(), 1);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
}

#[test]
fn forwards_payload_binding_through_send() {
    let source = r#"
module forward_payload;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Assign(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

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
        return Idle;
    }

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Assign(job);
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Assign(job: Job)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("payload binding forwarding should check");
    let sink = &checked.processes()[2];

    assert_eq!(
        sink.message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign"]
    );

    let artifact = lower_to_artifact(&checked, source).expect("forwarded payload should lower");
    let job = artifact_type_id(&artifact, "Job");
    assert_eq!(
        artifact.processes[2].message_variants,
        [ArtifactMessageVariant::payload("Assign", job)]
    );
}

#[test]
fn accepts_payload_message_without_concrete_send_case() {
    let source = r#"
module unsent_payload_case;

record MainState;
record Job { phase: JobPhase }
record WorkerState { job: Job }
enum MainMsg { Start }
enum JobPhase { Ready, Done }
enum WorkerMsg { Assign(Job), Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

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
        return WorkerState { job: Job { phase: Done } };
    }

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("unsent payload message should check");
    let worker = &checked.processes()[1];

    assert_eq!(
        worker
            .message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign", "Ping"]
    );
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
    assert_eq!(worker.message_cases()[1].payload_type(), None);
}

#[test]
fn accepts_payload_enum_type_declared_after_message_enum() {
    let source = r#"
module payload_enum_order;

record MainState;
enum MainMsg { Start }
enum WorkerMsg { Assign(JobKind) }
enum JobKind { Ready }
enum WorkerState { Idle }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Ready);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Assign(kind: JobKind)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("later enum payload type should resolve");
    let worker = &checked.processes()[1];

    assert_eq!(
        worker
            .message_cases()
            .iter()
            .map(|case| case.label())
            .collect::<Vec<_>>(),
        ["Assign"]
    );
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("JobKind".to_string())
    );
}
