use super::ast::EnumVariant;
use super::checked::{
    CheckedAction, CheckedMessageId, CheckedNextState, CheckedOutputId, CheckedProcess,
    CheckedProcessId, CheckedProcessRefId, CheckedSendTarget, CheckedStateId, CheckedStepResult,
    CheckedTransition, CheckedTypeKind,
};
use super::lexer::{Lexer, TokenKind};
use super::*;
use mantle_artifact::{
    ArtifactAction, ArtifactEffect, ArtifactMessageVariant, ArtifactSendTarget, ArtifactTypeKind,
    ArtifactValueTemplate, MAX_ACTIONS_PER_PROCESS, MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES,
    MAX_MAILBOX_BOUND, MAX_MESSAGE_VARIANTS_PER_PROCESS, MAX_PROCESS_COUNT,
    MAX_STATE_VALUES_PER_PROCESS, MAX_TYPE_COUNT, MAX_VALUE_TEMPLATE_FIELDS, MantleArtifact,
    ProcessId, ProcessRefId, StepResult, TypeId,
};

const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

const ACTOR_PING: &str = r#"
module actor_ping;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

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
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#;

const ACTOR_SEQUENCE: &str = r#"
module actor_sequence;

record MainState;
enum MainMsg { Start }
enum WorkerState { Waiting, SawFirst, Done }
enum WorkerMsg { First, Second }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

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

    fn step(state: WorkerState, First) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled First";
        return Continue(SawFirst);
    }

    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
}
"#;

const ACTOR_INSTANCES: &str = r#"
module actor_instances;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let first: ProcessRef<Worker> = spawn Worker;
        let second: ProcessRef<Worker> = spawn Worker;
        send first Ping;
        send second Ping;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker instance handled Ping";
        return Stop(Handled);
    }
}
"#;

#[test]
fn parses_and_checks_hello() {
    let checked = check_source(HELLO).expect("hello should check");

    assert_eq!(checked.module().name.as_str(), "hello");
    assert_eq!(checked.entry_process(), checked_process_id(0));
    assert_eq!(checked.entry_message(), checked_message_id(0));
    assert_eq!(checked.outputs(), ["hello from Strata"]);
    assert_eq!(checked.processes().len(), 1);
    let transition = only_transition(&checked.processes()[0]);
    assert_eq!(transition.message(), checked_message_id(0));
    assert_eq!(transition.step_result(), CheckedStepResult::Stop);
    assert_eq!(transition.next_state(), CheckedNextState::Current);
    assert_eq!(transition.effects(), &[Effect::Emit]);
    assert_eq!(
        transition.actions(),
        [CheckedAction::Emit {
            output: checked_output_id(0)
        }]
    );

    let artifact = lower_to_artifact(&checked, HELLO).expect("hello should lower");
    assert_eq!(
        artifact.processes[0].transitions[0].effects,
        vec![ArtifactEffect::Emit]
    );
}

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
                binding: None,
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
            binding: Some(Param {
                name: Identifier::new("job").expect("job identifier"),
                ty: TypeRef::Named(Identifier::new("Job").expect("Job identifier")),
            }),
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
fn forwards_process_ref_payload_through_received_send_target() {
    let source = r#"
module process_ref_reply;

record MainState;
record WorkerState;
record SinkState;
enum MainMsg { Start }
enum WorkerMsg { Work(ProcessRef<Sink>) }
enum SinkMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Sink>)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        send reply_to Done;
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let checked = check_source(source).expect("process ref payload forwarding should check");
    let checked_sink_ref = checked
        .types()
        .iter()
        .find(|ty| {
            ty.kind()
                == (CheckedTypeKind::ProcessRef {
                    target: checked_process_id(2),
                })
        })
        .expect("checked type table should contain Sink process reference type");
    assert_eq!(checked_sink_ref.label(), "__strata_checked_process_ref_2");
    assert_eq!(
        checked_sink_ref.kind(),
        CheckedTypeKind::ProcessRef {
            target: checked_process_id(2)
        }
    );
    assert_eq!(
        checked.processes()[1].message_cases()[0]
            .payload_type()
            .map(|ty| ty.id()),
        Some(checked_sink_ref.id())
    );
    let artifact = lower_to_artifact(&checked, source).expect("process ref payload should lower");
    let sink_ref = artifact_process_ref_type_id(&artifact, ProcessId::new(2));

    assert_eq!(
        artifact.processes[1].message_variants,
        [ArtifactMessageVariant::payload("Work", sink_ref)]
    );
    assert_eq!(
        artifact.processes[0].transitions[0].actions[2],
        ArtifactAction::Send {
            target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: mantle_artifact::MessageId::new(0),
            payload: Some(ArtifactValueTemplate::ProcessRef {
                ty: sink_ref,
                target_process: ProcessId::new(2),
                process_ref: ProcessRefId::new(1),
            }),
        }
    );
    assert_eq!(
        artifact.processes[1].transitions[0].actions[0],
        ArtifactAction::Send {
            target: ArtifactSendTarget::ReceivedPayload {
                ty: sink_ref,
                target_process: ProcessId::new(2),
            },
            message: mantle_artifact::MessageId::new(0),
            payload: None,
        }
    );
}

#[test]
fn process_ref_type_label_is_bounded_for_max_length_target_process() {
    let target = format!("P{}", "a".repeat(MAX_IDENTIFIER_BYTES - 1));
    let source = format!(
        r#"
module process_ref_limit;

record MainState;
record WorkerState;
record SinkState;
enum MainMsg {{ Start }}
enum WorkerMsg {{ Work(ProcessRef<{target}>) }}
enum SinkMsg {{ Done }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return WorkerState;
    }}

    fn step(state: WorkerState, Work(reply_to: ProcessRef<{target}>)) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}

proc {target} mailbox bounded(1) {{
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {{
        return SinkState;
    }}

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    );

    let checked = check_source(&source).expect("max-length process-ref target should check");
    let checked_sink_ref = checked
        .types()
        .iter()
        .find(|ty| {
            ty.kind()
                == (CheckedTypeKind::ProcessRef {
                    target: checked_process_id(2),
                })
        })
        .expect("checked type table should contain bounded process reference type");

    assert_eq!(checked_sink_ref.label(), "__strata_checked_process_ref_2");
    assert!(checked_sink_ref.label().len() <= MAX_IDENTIFIER_BYTES);

    let artifact =
        lower_to_artifact(&checked, &source).expect("bounded process ref label should lower");
    let sink_ref = artifact_process_ref_type_id(&artifact, ProcessId::new(2));
    assert_eq!(
        artifact.types[sink_ref.index()].label,
        checked_sink_ref.label()
    );
}

#[test]
fn rejects_send_missing_required_message_payload() {
    let source = payload_source_with(
        "send worker Assign;",
        "fn step(state: WorkerState, Assign(job: Job))",
    );

    let err = check_source(&source).expect_err("missing message payload should fail");

    assert!(
        err.to_string()
            .contains("message Assign requires a payload")
    );
}

#[test]
fn rejects_payload_for_unit_message_variant() {
    let source = ACTOR_PING.replace("send worker Ping;", "send worker Ping(MainState);");

    let err = check_source(&source).expect_err("payload on unit message should fail");

    assert!(
        err.to_string()
            .contains("message Ping does not accept a payload")
    );
}

#[test]
fn accepts_payload_value_near_label_limit_without_wrapping_message_label() {
    let source = payload_message_label_overflow_source();

    let checked = check_source(&source).expect("payload value near label limit should check");
    let worker = &checked.processes()[1];

    assert_eq!(worker.message_cases()[0].label(), "Assign");
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
    lower_to_artifact(&checked, &source).expect("near-limit payload should lower to artifact");
}

#[test]
fn rejects_wildcard_payload_binding() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, _(job: Job))",
    );

    let err = parse_source(&source).expect_err("wildcard payload binding should fail");

    assert!(
        err.to_string()
            .contains("wildcard patterns cannot bind payloads")
    );
}

#[test]
fn rejects_forward_payload_binding_with_wrong_send_type() {
    let source = r#"
module forward_payload_wrong_type;

record MainState;
record Job { phase: JobPhase }
record OtherJob { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Assign(OtherJob) }

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

    fn step(state: SinkState, Assign(job: OtherJob)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("forwarded payload type mismatch should fail");

    assert!(
        err.to_string()
            .contains("value binding job has type Job, expected OtherJob")
    );
}

#[test]
fn rejects_process_ref_payload_with_wrong_target_type() {
    let source = r#"
module wrong_process_ref_payload;

record MainState;
record WorkerState;
record SinkState;
record OtherState;
enum MainMsg { Start }
enum WorkerMsg { Work(ProcessRef<Other>) }
enum SinkMsg { Done }
enum OtherMsg { Done }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sink: ProcessRef<Sink> = spawn Sink;
        send worker Work(sink);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(reply_to: ProcessRef<Other>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return SinkState;
    }

    fn step(state: SinkState, Done) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}

proc Other mailbox bounded(1) {
    type State = OtherState;
    type Msg = OtherMsg;

    fn init() -> OtherState ! [] ~ [] @det {
        return OtherState;
    }

    fn step(state: OtherState, Done) -> ProcResult<OtherState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("wrong process ref payload should fail");

    assert!(
        err.to_string()
            .contains("process reference payload sink targets process id 2, expected 3")
    );
}

#[test]
fn rejects_non_process_ref_payload_as_send_target() {
    let source = r#"
module non_ref_send_target;

record MainState;
record Job;
record WorkerState;
enum MainMsg { Start }
enum WorkerMsg { Work(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Work(Job);
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return WorkerState;
    }

    fn step(state: WorkerState, Work(job: Job)) -> ProcResult<WorkerState> ! [send] ~ [] @det {
        send job Work(Job);
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("non-ref send target should fail");

    assert!(
        err.to_string()
            .contains("process Worker send target job is not a process reference payload")
    );
}

#[test]
fn rejects_step_payload_binding_with_wrong_type() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, Assign(job: MainState))",
    );

    let err = check_source(&source).expect_err("wrong payload binding type should fail");

    assert!(
        err.to_string()
            .contains("step pattern payload job has type MainState, expected Job")
    );
}

#[test]
fn rejects_payload_binding_named_like_value_constructor() {
    let source = payload_source_with(
        "send worker Assign(Job { phase: Ready });",
        "fn step(state: WorkerState, Assign(Job: Job))",
    );

    let err = check_source(&source).expect_err("constructor-like payload binding should fail");

    assert!(
        err.to_string()
            .contains("payload binding Job conflicts with a declared type or value constructor")
    );
}

#[test]
fn rejects_process_ref_named_like_payload_binding() {
    let source = r#"
module payload_process_ref_conflict;

record MainState;
record Job { phase: JobPhase }
enum JobPhase { Ready, Done }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Ping }

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
        let job: ProcessRef<Sink> = spawn Sink;
        send job Ping;
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Ping) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("local binding shadowing should fail");

    assert!(
        err.to_string()
            .contains("process reference job conflicts with payload binding")
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
fn rejects_invalid_step_signature_before_payload_case_discovery() {
    let source = r#"
module invalid_step_discovery;

record MainState;
record Job { phase: Phase }
enum Phase { Ready }
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(Job) }
enum SinkState { Idle }
enum SinkMsg { Forward(Job) }

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

    fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let sink: ProcessRef<Sink> = spawn Sink;
        send sink Forward(Job { phase: Ready });
        return Stop(state);
    }
}

proc Sink mailbox bounded(1) {
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: SinkState, Forward(job: Job)) -> ProcResult<SinkState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("invalid step signature should fail first");

    assert!(err.to_string().contains(
        "step second parameter must be a message constructor pattern or wildcard pattern"
    ));
}

#[test]
fn rejects_generic_message_payload_type_with_precise_diagnostic() {
    let source = r#"
module generic_payload_type;

record MainState;
record Job;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Assign(ProcResult<Job>) }

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

    fn step(state: WorkerState, Assign(job: ProcResult<Job>)) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("generic payload type should fail");

    assert!(err.to_string().contains(
        "payload type ProcResult<Job> must be a named record, enum, or process reference type"
    ));
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

#[test]
fn rejects_payload_entry_message() {
    let source = r#"
module entry_payload;

record MainState;
record Job;
enum MainMsg { Start(Job) }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start(job: Job)) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("payload entry message should fail");

    assert!(
        err.to_string()
            .contains("entry message Start must not require a payload")
    );
}

#[test]
fn rejects_state_enum_payload_variant() {
    let source = r#"
module state_payload;

record Job;
enum MainState { Idle(Job) }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("state payload variant should fail");

    assert!(
        err.to_string()
            .contains("state enum MainState variant Idle must not declare a payload")
    );
}

#[test]
fn public_ast_constructors_validate_values() {
    let identifier = Identifier::new("MainState").expect("valid identifier should construct");
    assert_eq!(identifier.as_str(), "MainState");
    let identifier_from_try =
        Identifier::try_from("Worker").expect("TryFrom should construct identifiers");
    assert_eq!(identifier_from_try.as_str(), "Worker");
    assert!(Identifier::new("1Invalid").is_err());
    assert!(Identifier::new("invalid-name").is_err());
    assert!(Identifier::new("_").is_err());
    assert!(Identifier::new("as").is_err());
    assert!(Identifier::new("let").is_err());
    assert!(Identifier::new("mut").is_err());
    assert!(Identifier::new("var").is_err());

    let output = OutputLiteral::new("hello from Strata").expect("valid output should construct");
    assert_eq!(output.as_str(), "hello from Strata");
    let output_from_try =
        OutputLiteral::try_from("worker handled Ping").expect("TryFrom should construct output");
    assert_eq!(output_from_try.as_str(), "worker handled Ping");
    assert!(OutputLiteral::new("").is_err());
    assert!(OutputLiteral::new("bad\noutput").is_err());
}

#[test]
fn resolves_lowercase_state_values_without_casing_semantics() {
    let source = r#"
module lowercase_state;

record Marker;
enum MainState { ready }
enum MainMsg { start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return ready;
    }

    fn step(state: MainState, start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(ready);
    }
}
"#;

    let checked = check_source(source).expect("lowercase state values should check");

    assert_eq!(checked_state_labels(&checked.processes()[0]), ["ready"]);
    assert_eq!(checked.processes()[0].init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(&checked.processes()[0]).next_state(),
        CheckedNextState::Value(checked_state_id(0))
    );
}

#[test]
fn rejects_state_value_named_like_step_state_parameter() {
    let source = r#"
module reserved_state_value;

record Marker;
enum MainState { state }
enum MainMsg { start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return state;
    }

    fn step(state: MainState, start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("reserved state value should fail");

    assert!(
        err.to_string()
            .contains("state value state conflicts with reserved step state parameter name")
    );
}

#[test]
fn parses_and_checks_actor_ping() {
    let checked = check_source(ACTOR_PING).expect("actor ping should check");

    assert_eq!(checked.module().name.as_str(), "actor_ping");
    assert_eq!(checked.entry_process(), checked_process_id(0));
    assert_eq!(checked.entry_message(), checked_message_id(0));
    assert_eq!(checked.outputs(), ["worker handled Ping"]);
    assert_eq!(checked.processes().len(), 2);

    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");
    let main_transition = only_transition(main);
    assert_eq!(main_transition.message(), checked_message_id(0));
    assert_eq!(
        main_transition.actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(0)
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                message: checked_message_id(0),
                payload: None
            }
        ]
    );

    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(worker.init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(worker).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
}

#[test]
fn parses_and_lowers_panic_step_result() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Panic(Handled);");

    let checked = check_source(&source).expect("panic step result should check");
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        only_transition(worker).step_result(),
        CheckedStepResult::Panic
    );
    assert_eq!(
        only_transition(worker).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );

    let artifact = lower_to_artifact(&checked, &source).expect("panic should lower");
    assert_eq!(
        artifact.processes[1].transitions[0].step_result,
        StepResult::Panic
    );
}

#[test]
fn parses_and_checks_actor_sequence_step_patterns() {
    let checked = check_source(ACTOR_SEQUENCE).expect("actor sequence should check");

    assert_eq!(checked.module().name.as_str(), "actor_sequence");
    assert_eq!(
        checked.outputs(),
        ["worker handled First", "worker handled Second"]
    );
    let worker = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Worker")
        .expect("Worker should be checked");
    assert_eq!(
        checked_state_labels(worker),
        ["Waiting", "SawFirst", "Done"]
    );
    assert_eq!(worker.transitions().len(), 2);
    assert_eq!(worker.transitions()[0].message(), checked_message_id(0));
    assert_eq!(
        worker.transitions()[0].step_result(),
        CheckedStepResult::Continue
    );
    assert_eq!(
        worker.transitions()[0].next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
    assert_eq!(worker.transitions()[1].message(), checked_message_id(1));
    assert_eq!(
        worker.transitions()[1].step_result(),
        CheckedStepResult::Stop
    );
    assert_eq!(
        worker.transitions()[1].next_state(),
        CheckedNextState::Value(checked_state_id(2))
    );

    let artifact = lower_to_artifact(&checked, ACTOR_SEQUENCE)
        .expect("step patterns should lower to transition records");
    assert_eq!(
        artifact.processes[0].transitions[0].effects,
        vec![ArtifactEffect::Spawn, ArtifactEffect::Send]
    );
    let worker_artifact = &artifact.processes[1];
    assert_eq!(worker_artifact.transitions.len(), 2);
    assert_eq!(
        worker_artifact.transitions[0].effects,
        vec![ArtifactEffect::Emit]
    );
    assert_eq!(
        worker_artifact.transitions[1].effects,
        vec![ArtifactEffect::Emit]
    );
    assert_eq!(
        worker_artifact.transitions[0].message,
        mantle_artifact::MessageId::new(0)
    );
    assert_eq!(
        worker_artifact.transitions[1].message,
        mantle_artifact::MessageId::new(1)
    );
    let encoded = artifact.encode();
    assert!(encoded.contains("process.1.transition.0.message=0"));
    assert!(encoded.contains("process.1.transition.1.message=1"));
    assert!(!encoded.contains("transition.0.message=First"));
}

#[test]
fn parses_and_checks_actor_instances_with_distinct_process_refs() {
    let checked = check_source(ACTOR_INSTANCES).expect("actor instances should check");
    let main = checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == "Main")
        .expect("Main should be checked");

    assert_eq!(main.process_refs().len(), 2);
    assert_eq!(main.process_refs()[0].debug_name().as_str(), "first");
    assert_eq!(main.process_refs()[0].target(), checked_process_id(1));
    assert_eq!(main.process_refs()[1].debug_name().as_str(), "second");
    assert_eq!(main.process_refs()[1].target(), checked_process_id(1));
    assert_eq!(
        only_transition(main).actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(0)
            },
            CheckedAction::Spawn {
                target: checked_process_id(1),
                process_ref: checked_process_ref_id(1)
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                message: checked_message_id(0),
                payload: None
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(1)),
                message: checked_message_id(0),
                payload: None
            }
        ]
    );

    let artifact =
        lower_to_artifact(&checked, ACTOR_INSTANCES).expect("actor instances should lower");
    let encoded = artifact.encode();
    assert!(encoded.contains("process.0.process_ref_count=2"));
    assert!(encoded.contains("process.0.process_ref.0.target_process=1"));
    assert!(encoded.contains("process.0.process_ref.1.target_process=1"));
    assert!(encoded.contains("process.0.transition.0.action.2.target_process_ref=0"));
    assert!(encoded.contains("process.0.transition.0.action.3.target_process_ref=1"));
}

#[test]
fn rejects_unknown_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        "fn step(state: WorkerState, Second)",
        "fn step(state: WorkerState, Unknown)",
    );

    let err = check_source(&source).expect_err("unknown step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker step pattern message Unknown is not accepted")
    );
}

#[test]
fn rejects_missing_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
"#,
        "",
    );

    let err = check_source(&source).expect_err("missing step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker must declare step pattern for message Second")
    );
}

#[test]
fn rejects_duplicate_step_message_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        "fn step(state: WorkerState, Second)",
        "fn step(state: WorkerState, First)",
    );

    let err = check_source(&source).expect_err("duplicate step pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate step pattern for message First")
    );
}

#[test]
fn rejects_duplicate_wildcard_step_pattern() {
    let source = ACTOR_SEQUENCE
        .replace(
            "fn step(state: WorkerState, First)",
            "fn step(state: WorkerState, _)",
        )
        .replace(
            "fn step(state: WorkerState, Second)",
            "fn step(state: WorkerState, _)",
        );

    let err = check_source(&source).expect_err("duplicate wildcard pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate wildcard step pattern")
    );
}

#[test]
fn rejects_unreachable_wildcard_step_pattern() {
    let source = ACTOR_SEQUENCE.replace(
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }
"#,
        r#"
    fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }

    fn step(state: WorkerState, _) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Continue(state);
    }
"#,
    );

    let err = check_source(&source).expect_err("unreachable wildcard pattern should fail");

    assert!(
        err.to_string()
            .contains("process Worker wildcard step pattern is unreachable")
    );
}

#[test]
fn rejects_typed_msg_step_parameter() {
    let source = ACTOR_PING.replace(
        "fn step(state: WorkerState, Ping)",
        "fn step(state: WorkerState, msg: WorkerMsg)",
    );

    let err = check_source(&source).expect_err("typed message parameter should fail");

    assert!(err.to_string().contains(
        "step second parameter must be a message constructor pattern or wildcard pattern"
    ));
}

#[test]
fn rejects_match_with_wrong_target() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match state {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("wrong match scrutinee should fail");

    assert!(
        err.to_string().contains(
            "process Worker match scrutinee state must be the step message parameter msg"
        )
    );
}

#[test]
fn rejects_match_with_wrong_message_parameter_type() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: MainMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("wrong message parameter type should fail");

    assert!(
        err.to_string()
            .contains("process Worker message parameter msg has type MainMsg, expected WorkerMsg")
    );
}

#[test]
fn rejects_missing_match_arm() {
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
        }
    }"#,
    );

    let err = check_source(&source).expect_err("missing match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker must declare step pattern for message Second")
    );
}

#[test]
fn rejects_duplicate_match_arm() {
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
            First => {
                emit "worker handled First again";
                return Stop(Done);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("duplicate match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker declares duplicate step pattern for message First")
    );
}

#[test]
fn rejects_unknown_match_arm() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Unknown => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("unknown match arm should fail");

    assert!(
        err.to_string()
            .contains("process Worker step pattern message Unknown is not accepted")
    );
}

#[test]
fn rejects_mixed_parameter_pattern_and_match_dispatch() {
    let source = ACTOR_SEQUENCE.replace(
        r#"fn step(state: WorkerState, Second) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Second";
        return Stop(Done);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Second => {
                emit "worker handled Second";
                return Stop(Done);
            }
        }
    }"#,
    );

    let err = check_source(&source).expect_err("mixed step dispatch should fail");

    assert!(
        err.to_string()
            .contains("process Worker cannot mix match step bodies with step parameter patterns")
    );
}

#[test]
fn rejects_step_pattern_invalid_next_state() {
    let source = ACTOR_SEQUENCE.replace("Continue(SawFirst)", "Continue(UnknownState)");

    let err = check_source(&source).expect_err("invalid next state should fail");

    assert!(
        err.to_string()
            .contains("value UnknownState is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_match_arm_comma_separator() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            },
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("comma-separated match arms should fail");

    assert!(
        err.to_string()
            .contains("match arms are block-delimited and must not use comma separators")
    );
}

#[test]
fn rejects_match_arm_split_fat_arrow() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping = > {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
    }"#,
    );

    let err = parse_source(&source).expect_err("split match arm arrow should fail");

    assert!(err.to_string().contains("expected =>"));
}

#[test]
fn rejects_match_body_in_init_for_buildable_source() {
    let source = ACTOR_PING.replace(
        r#"fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }"#,
        r#"fn init() -> WorkerState ! [] ~ [] @det {
        match Idle {
            Idle => {
                return Idle;
            }
        }
    }"#,
    );

    let module = parse_source(&source).expect("init match body should parse");
    let err = check_module(module).expect_err("init match body should fail checking");

    assert!(
        err.to_string()
            .contains("init must have a body block for buildable source")
    );
}

#[test]
fn rejects_trailing_statement_after_match_body() {
    let source = ACTOR_PING.replace(
        r#"fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }"#,
        r#"fn step(state: WorkerState, msg: WorkerMsg) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
        return Stop(Handled);
    }"#,
    );

    let err = parse_source(&source).expect_err("trailing statement after match should fail");

    assert!(
        err.to_string()
            .contains("match body must be the whole function body in this source slice")
    );
}

#[test]
fn rejects_nested_match_body_syntax() {
    let source = ACTOR_PING.replace(
        r#"emit "worker handled Ping";
        return Stop(Handled);"#,
        r#"emit "before match";
        match msg {
            Ping => {
                emit "worker handled Ping";
                return Stop(Handled);
            }
        }
        return Stop(Handled);"#,
    );

    let err = parse_source(&source).expect_err("nested match body should fail");

    assert!(
        err.to_string()
            .contains("match body must be the whole function body in this source slice")
    );
}

#[test]
fn resolves_process_references_to_ids_before_artifact_encoding() {
    let source = r#"
module actor_ping;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle, Handled }
enum WorkerMsg { Ping }

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}

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
"#;

    let checked = check_source(source).expect("reordered actor ping should check");
    let main = checked
        .processes()
        .get(checked.entry_process().index())
        .expect("Main entry should be present");

    assert_eq!(checked.entry_process(), checked_process_id(1));
    assert_eq!(main.debug_name().as_str(), "Main");
    assert_eq!(
        only_transition(main).actions(),
        [
            CheckedAction::Spawn {
                target: checked_process_id(0),
                process_ref: checked_process_ref_id(0)
            },
            CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                message: checked_message_id(0),
                payload: None
            }
        ]
    );

    let artifact = lower_to_artifact(&checked, source).expect("checked program should lower");
    let encoded = artifact.encode();
    assert!(encoded.contains("entry_process=1"));
    assert!(encoded.contains("process.1.transition.0.action.0.target_process=0"));
    assert!(encoded.contains("process.1.transition.0.action.0.process_ref=0"));
    assert!(encoded.contains("process.1.transition.0.action.1.target_process_ref=0"));
    assert!(!encoded.contains("target_process=Worker"));
}

#[test]
fn rejects_declaration_only_entry_points() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det;
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det;
}
"#;

    let err = check_source(source).expect_err("declaration-only source should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("init must have a body"),
        "unexpected error: {message}"
    );
}

#[test]
fn rejects_missing_main_entry_process() {
    let source = HELLO.replace("proc Main", "proc Worker");

    let err = check_source(&source).expect_err("missing Main should be rejected");

    assert!(
        err.to_string()
            .contains("entry process Main is not declared")
    );
}

#[test]
fn rejects_process_count_above_artifact_limit_during_checking() {
    let mut source = r#"
module too_many_processes;
record MainState;
enum MainMsg { Start }
"#
    .to_string();
    for index in 0..=MAX_PROCESS_COUNT {
        let name = if index == 0 {
            "Main".to_string()
        } else {
            format!("Proc{index}")
        };
        source.push_str(&format!(
            r#"
proc {name} mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det {{ return MainState; }}
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
        ));
    }
    let module = parse_source(&source).expect("oversized process source should parse");

    let err = check_module(module).expect_err("process count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process_count must be no greater than {MAX_PROCESS_COUNT}"
    )));
}

#[test]
fn rejects_mailbox_bound_above_artifact_limit_during_checking() {
    let source = HELLO.replace(
        "mailbox bounded(1)",
        &format!("mailbox bounded({})", MAX_MAILBOX_BOUND + 1),
    );
    let module = parse_source(&source).expect("mailbox-bound source should parse");

    let err = check_module(module).expect_err("mailbox bound above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main mailbox_bound must be no greater than {MAX_MAILBOX_BOUND}"
    )));
}

#[test]
fn rejects_zero_mailbox_bound_with_shared_count_diagnostic() {
    let source = HELLO.replace("mailbox bounded(1)", "mailbox bounded(0)");
    let module = parse_source(&source).expect("zero-mailbox-bound source should parse");

    let err = check_module(module).expect_err("zero mailbox bound should fail");

    assert!(
        err.to_string()
            .contains("process Main mailbox_bound must be greater than zero")
    );
}

#[test]
fn rejects_state_value_count_above_artifact_limit_during_checking() {
    let state_values = (0..=MAX_STATE_VALUES_PER_PROCESS)
        .map(|index| format!("State{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = HELLO
        .replace(
            "record MainState;",
            &format!("enum MainState {{ {state_values} }}"),
        )
        .replace(
            "enum MainMsg { Start }",
            "record Marker;\nenum MainMsg { Start }",
        )
        .replace("return MainState;", "return State0;");
    let module = parse_source(&source).expect("state-value-count source should parse");

    let err = check_module(module).expect_err("state value count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main state_value_count must be no greater than {MAX_STATE_VALUES_PER_PROCESS}"
    )));
}

#[test]
fn rejects_empty_state_enum_with_enum_diagnostic() {
    let source = HELLO.replace("record MainState;", "record Marker;\nenum MainState {}");

    let err = check_source(&source).expect_err("empty state enum should fail");

    assert!(
        err.to_string()
            .contains("enum MainState must declare at least one variant")
    );
}

#[test]
fn preserves_undeclared_state_type_diagnostics() {
    for (source, expected) in [
        (
            HELLO.replace("type State = MainState;", "type State = MissingState;"),
            "type MissingState is not declared",
        ),
        (
            HELLO.replace("type State = MainState;", "type State = Box<MainState>;"),
            "type Box<MainState> is not declared",
        ),
    ] {
        let err = check_source(&source).expect_err("undeclared state type should fail");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_message_count_above_artifact_limit_during_checking() {
    let messages = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("Msg{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = HELLO.replace(
        "enum MainMsg { Start }",
        &format!("enum MainMsg {{ {messages} }}"),
    );
    let module = parse_source(&source).expect("message-count source should parse");

    let err = check_module(module).expect_err("message count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main message_count must be no greater than {MAX_MESSAGE_VARIANTS_PER_PROCESS}"
    )));
}

#[test]
fn rejects_checked_type_count_above_artifact_limit_during_checking() {
    let module = checked_type_count_overflow_module();

    let err =
        check_module(module).expect_err("checked type count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "checked type_count exceeds Mantle artifact limit of {MAX_TYPE_COUNT} types"
    )));
}

#[test]
fn accepts_payload_send_count_above_message_variant_limit_without_case_expansion() {
    let phases = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("P{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sends = (0..=MAX_MESSAGE_VARIANTS_PER_PROCESS)
        .map(|index| format!("        send worker Assign(Job {{ phase: P{index} }});\n"))
        .collect::<String>();
    let mailbox_bound = MAX_MESSAGE_VARIANTS_PER_PROCESS + 1;
    let source = format!(
        r#"
module concrete_payload_count;

record MainState;
record Job {{ phase: JobPhase }}
enum JobPhase {{ {phases} }}
enum MainMsg {{ Start }}
enum WorkerState {{ Idle }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
{sends}        return Stop(state);
    }}
}}

proc Worker mailbox bounded({mailbox_bound}) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Continue(state);
    }}
}}
"#
    );
    let module = parse_source(&source).expect("payload-send-count source should parse");

    let checked = check_module(module).expect("payload sends should not expand message variants");
    let worker = &checked.processes()[1];

    assert_eq!(worker.message_cases().len(), 1);
    assert_eq!(worker.message_cases()[0].label(), "Assign");
    assert_eq!(
        worker.message_cases()[0]
            .payload_type()
            .map(ToString::to_string),
        Some("Job".to_string())
    );
}

#[test]
fn rejects_action_count_above_artifact_limit_during_checking() {
    let mut statements = String::new();
    for _ in 0..=MAX_ACTIONS_PER_PROCESS {
        statements.push_str("        emit \"hello from Strata\";\n");
    }
    let source = HELLO.replace("        emit \"hello from Strata\";\n", &statements);
    let module = parse_source(&source).expect("action-count source should parse");

    let err = check_module(module).expect_err("action count above artifact limit should fail");

    assert!(err.to_string().contains(&format!(
        "process Main action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
}

#[test]
fn rejects_process_action_budget_across_message_transitions_during_checking() {
    let first_actions = repeated_emit_statements(MAX_ACTIONS_PER_PROCESS / 2, 16);
    let second_actions = repeated_emit_statements((MAX_ACTIONS_PER_PROCESS / 2) + 1, 16);
    let source = format!(
        r#"
module action_budget;

record MainState;
enum MainMsg {{ Start, Again }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [emit] ~ [] @det {{
{first_actions}        return Stop(state);
    }}

    fn step(state: MainState, Again) -> ProcResult<MainState> ! [emit] ~ [] @det {{
{second_actions}        return Stop(state);
    }}
}}
"#
    );
    let module = parse_source(&source).expect("aggregate action-budget source should parse");

    let err = check_module(module).expect_err("aggregate action budget should fail");

    assert!(err.to_string().contains(&format!(
        "process Main action_count must be no greater than {MAX_ACTIONS_PER_PROCESS}"
    )));
}

#[test]
fn rejects_duplicate_process_members() {
    for (source, expected) in [
            (
                HELLO.replace(
                    "type State = MainState;",
                    "type State = MainState;\n    type State = MainState;",
                ),
                "process Main declares duplicate type State",
            ),
            (
                HELLO.replace(
                    "type Msg = MainMsg;",
                    "type Msg = MainMsg;\n    type Msg = MainMsg;",
                ),
                "process Main declares duplicate type Msg",
            ),
            (
                HELLO.replace(
                    "fn init() -> MainState ! [] ~ [] @det {",
                    "fn init() -> MainState ! [] ~ [] @det { return MainState; }\n\n    fn init() -> MainState ! [] ~ [] @det {",
                ),
                "process Main declares duplicate init function",
            ),
        ] {
            let err = parse_source(&source).expect_err("duplicate process member should fail");

            assert!(
                err.to_string().contains(expected),
                "expected {expected:?}, got {err}"
            );
        }
}

#[test]
fn rejects_missing_list_separators() {
    for source in [
        HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start Other }"),
        HELLO.replace("! [emit] ~ []", "! [emit send] ~ []"),
        HELLO.replace("ProcResult<MainState>", "ProcResult<MainState MainMsg>"),
    ] {
        let err = parse_source(&source).expect_err("missing separator should fail");

        assert!(err.to_string().contains("expected symbol"));
    }
}

#[test]
fn rejects_oversized_source_before_tokenizing() {
    let source = " ".repeat(MAX_SOURCE_BYTES + 1);

    let err = parse_source(&source).expect_err("oversized source should fail");

    assert!(err.to_string().contains("source exceeds maximum size"));
}

#[test]
fn rejects_excessive_token_count() {
    let source = "{}".repeat((MAX_TOKEN_COUNT / 2) + 1);

    let err = parse_source(&source).expect_err("excessive token count should fail");

    assert!(err.to_string().contains("maximum token count"));
}

#[test]
fn lexer_accepts_exact_source_token_limit_plus_eof() {
    let source = "{}".repeat(MAX_TOKEN_COUNT / 2);

    let tokens = Lexer::new(&source)
        .tokenize()
        .expect("exact source token limit should tokenize");

    assert_eq!(tokens.len(), MAX_TOKEN_COUNT + 1);
    assert!(matches!(
        tokens.last().map(|token| &token.kind),
        Some(TokenKind::Eof)
    ));
}

#[test]
fn rejects_excessive_type_nesting() {
    let mut nested_type = "MainState".to_string();
    for _ in 0..=MAX_TYPE_NESTING {
        nested_type = format!("Box<{nested_type}>");
    }
    let source = HELLO.replace(
        "ProcResult<MainState>",
        &format!("ProcResult<{nested_type}>"),
    );

    let err = parse_source(&source).expect_err("excessive type nesting should fail");

    assert!(
        err.to_string()
            .contains("type nesting exceeds maximum depth")
    );
}

#[test]
fn rejects_excessive_value_nesting_while_parsing() {
    let value = nested_record_value_source(MAX_VALUE_NESTING + 1);
    let source = HELLO.replacen("return MainState;", &format!("return {value};"), 1);

    let err = parse_source(&source).expect_err("excessive value nesting should fail");

    let message = err.to_string();
    assert!(message.contains("value nesting exceeds maximum depth"));
    assert!(
        message.contains(" at byte "),
        "expected byte-offset context in diagnostic: {message}"
    );
}

#[test]
fn rejects_emit_without_declared_effect() {
    let source = r#"
module hello;
record MainState;
enum MainMsg { Start }
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

    let err = check_source(source).expect_err("undeclared emit should be rejected");
    assert!(
        err.to_string()
            .contains("step uses effect emit but does not declare it")
    );
}

#[test]
fn rejects_spawn_without_declared_effect() {
    let source = ACTOR_PING.replace("! [spawn, send]", "! [send]");

    let err = check_source(&source).expect_err("undeclared spawn should be rejected");

    assert!(
        err.to_string()
            .contains("step uses effect spawn but does not declare it")
    );
}

#[test]
fn rejects_send_without_declared_effect() {
    let source = ACTOR_PING.replace("! [spawn, send]", "! [spawn]");

    let err = check_source(&source).expect_err("undeclared send should be rejected");

    assert!(
        err.to_string()
            .contains("step uses effect send but does not declare it")
    );
}

#[test]
fn rejects_unused_declared_effect() {
    let source = HELLO.replace("! [emit]", "! [emit, send]");

    let err = check_source(&source).expect_err("unused declared effect should be rejected");

    assert!(
        err.to_string()
            .contains("step declares effect send but does not use it")
    );
}

#[test]
fn rejects_duplicate_declared_effect() {
    let source = HELLO.replace("! [emit]", "! [emit, emit]");

    let err = check_source(&source).expect_err("duplicate declared effect should be rejected");

    assert!(
        err.to_string()
            .contains("step declares duplicate effect emit")
    );
}

#[test]
fn rejects_unknown_effect_name() {
    let source = HELLO.replace("! [emit]", "! [write]");

    let err = parse_source(&source).expect_err("unknown effect should fail");

    assert!(err.to_string().contains("unsupported effect write"));
}

#[test]
fn parses_and_checks_immutable_record_state_constructors() {
    let source = r#"
module record_state;

enum Phase { Idle, Handled }
record MainState {
    phase: Phase,
}
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { phase: Idle };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(MainState { phase: Handled });
    }
}
"#;

    let checked = check_source(source).expect("immutable record state should check");

    assert_eq!(
        checked_state_labels(&checked.processes()[0]),
        ["MainState{phase:Idle}", "MainState{phase:Handled}"]
    );
    assert_eq!(checked.processes()[0].init_state(), checked_state_id(0));
    assert_eq!(
        only_transition(&checked.processes()[0]).next_state(),
        CheckedNextState::Value(checked_state_id(1))
    );
}

#[test]
fn rejects_semicolons_after_braced_type_declarations() {
    for (source, expected) in [
        (
            HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start };"),
            "braced enum declarations are terminated by '}', not ';'",
        ),
        (
            HELLO.replace(
                "record MainState;",
                "enum Phase { Idle }\nrecord MainState { phase: Phase };",
            ),
            "braced record declarations are terminated by '}', not ';'",
        ),
    ] {
        let err = parse_source(&source).expect_err("braced type semicolon should be rejected");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_empty_braced_record_declarations() {
    let source = HELLO.replace("record MainState;", "record MainState {}");

    let err = parse_source(&source).expect_err("empty braced records should be rejected");

    assert!(
        err.to_string().contains(
            "fieldless records use `record MainState;`; braced records must declare at least one field"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_mutable_record_field_declarations() {
    let source = HELLO.replace(
        "record MainState;",
        "enum Phase { Idle }\nrecord MainState { mut phase: Phase }",
    );

    let err = parse_source(&source).expect_err("mutable record fields should be rejected");

    assert!(
        err.to_string()
            .contains("record fields are immutable; mutable field declarations are not supported")
    );
}

#[test]
fn rejects_security_declarations_instead_of_erasing_source() {
    let source = HELLO.replace(
        "record MainState;",
        "security mut policy;\nrecord MainState;",
    );

    let err = parse_source(&source).expect_err("security declarations should not be skipped");

    assert!(
        err.to_string()
            .contains("security declarations are not supported")
    );
}

#[test]
fn rejects_mutability_keywords_as_state_values() {
    for keyword in ["as", "mut", "var"] {
        let source = r#"
module reserved_mutability_keyword;

record Marker;
enum MainState { REPLACE_KEYWORD }
enum MainMsg { Start }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return REPLACE_KEYWORD;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [] ~ [] @det {
        return Stop(REPLACE_KEYWORD);
    }
}
"#
        .replace("REPLACE_KEYWORD", keyword);

        let err = parse_source(&source).expect_err("mutability keyword should be reserved");

        assert!(
            err.to_string()
                .contains(&format!("identifier {keyword:?} is reserved")),
            "unexpected error for {keyword}: {err}"
        );
    }
}

#[test]
fn rejects_assignment_syntax_in_record_values() {
    let source = HELLO
        .replace(
            "record MainState;",
            "enum Phase { Idle }\nrecord MainState { phase: Phase }",
        )
        .replace("return MainState;", "return MainState { phase = Idle };");

    let err = parse_source(&source).expect_err("record value assignment should be rejected");

    assert!(
        err.to_string()
            .contains("record value fields use ':'; assignment syntax is not supported")
    );
}

#[test]
fn rejects_empty_braced_record_values() {
    let source = HELLO.replace("return MainState;", "return MainState {};");

    let err = parse_source(&source).expect_err("empty braced record values should be rejected");

    assert!(
        err.to_string().contains(
            "fieldless record values use `MainState`; braced record values must declare at least one field"
        ),
        "unexpected error: {err}"
    );
}

#[test]
fn rejects_incomplete_or_invalid_record_values() {
    for (source, expected) in [
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nenum Mode { Cold }\nrecord MainState { phase: Phase, mode: Mode }",
                )
                .replace("return MainState;", "return MainState { phase: Idle };"),
            "record value MainState is missing field mode",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Idle, extra: Idle };",
                ),
            "record value MainState declares unknown field extra",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Idle, phase: Idle };",
                ),
            "record value MainState duplicates field phase",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nenum Other { Wrong }\nrecord MainState { phase: Phase }",
                )
                .replace("return MainState;", "return MainState { phase: Wrong };"),
            "value Wrong is not a variant of enum Phase",
        ),
        (
            HELLO
                .replace(
                    "record MainState;",
                    "enum Phase { Idle }\nrecord MainState { phase: Phase }",
                )
                .replace(
                    "return MainState;",
                    "return MainState { phase: Other { value: Idle } };",
                ),
            "expected enum variant identifier for enum Phase",
        ),
    ] {
        let err = check_source(&source).expect_err("invalid record value should be rejected");

        assert!(
            err.to_string().contains(expected),
            "expected {expected:?}, got {err}"
        );
    }
}

#[test]
fn rejects_duplicate_process_ref_on_same_path() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Worker> = spawn Worker;\n        let worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("duplicate process reference should be rejected");

    assert!(
        err.to_string()
            .contains("duplicates process reference id 0")
    );
}

#[test]
fn allows_multiple_process_refs_for_same_process_definition() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;\n        send worker Ping;",
        "let first: ProcessRef<Worker> = spawn Worker;\n        let second: ProcessRef<Worker> = spawn Worker;\n        send first Ping;\n        send second Ping;",
    );

    check_source(&source).expect("distinct process refs may target the same process definition");
}

#[test]
fn rejects_spawn_without_process_ref() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "spawn Worker;",
    );

    let err = parse_source(&source).expect_err("standalone spawn should be rejected");

    assert!(
        err.to_string()
            .contains("expected emit, let, send, or return statement")
    );
}

#[test]
fn rejects_send_to_process_definition_name() {
    let source = ACTOR_PING.replace("send worker Ping;", "send Worker Ping;");

    let err = check_source(&source).expect_err("send to process definition should be rejected");

    assert!(
        err.to_string()
            .contains("process Main sends to undeclared process reference Worker")
    );
}

#[test]
fn rejects_process_ref_named_like_step_parameter() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let state: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source)
        .expect_err("step parameter process reference name should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference state conflicts with a step parameter name")
    );
}

#[test]
fn rejects_process_ref_named_like_process_declaration() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let Worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source)
        .expect_err("process declaration process reference name should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference Worker conflicts with a process declaration")
    );
}

#[test]
fn allows_same_spawn_target_in_distinct_terminal_step_patterns() {
    let source = r#"
module spawn_by_message;

record MainState;
enum MainMsg { Start, Restart }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

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

    fn step(state: MainState, Restart) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
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
        return Stop(Idle);
    }
}
"#;

    check_source(source).expect("mutually exclusive step patterns may spawn the same process");
}

#[test]
fn rejects_static_self_spawn() {
    let source = ACTOR_PING
        .replace("! [emit] ~ [] @det", "! [spawn] ~ [] @det")
        .replace(
            r#"emit "worker handled Ping";"#,
            "let child: ProcessRef<Worker> = spawn Worker;",
        );

    let err = check_source(&source).expect_err("self-spawn should be rejected");

    assert!(err.to_string().contains("process Worker spawns itself"));
}

#[test]
fn rejects_send_before_static_spawn() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;\n        send worker Ping;",
        "send worker Ping;\n        let worker: ProcessRef<Worker> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("send before spawn should be rejected");

    assert!(
        err.to_string()
            .contains("sends through unbound process reference id 0 within message transition 0")
    );
}

#[test]
fn rejects_process_ref_type_that_does_not_match_spawn_target() {
    let source = ACTOR_PING
        .replace(
            "enum WorkerMsg { Ping }",
            "enum WorkerMsg { Ping }\nenum HelperState { Idle }\nenum HelperMsg { Ping }",
        )
        .replace(
            "let worker: ProcessRef<Worker> = spawn Worker;",
            "let worker: ProcessRef<Helper> = spawn Worker;",
        )
        .replace(
            r#"
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}
"#,
            r#"
proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [emit] ~ [] @det {
        emit "worker handled Ping";
        return Stop(Handled);
    }
}

proc Helper mailbox bounded(1) {
    type State = HelperState;
    type Msg = HelperMsg;

    fn init() -> HelperState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: HelperState, Ping) -> ProcResult<HelperState> ! [] ~ [] @det {
        return Stop(Idle);
    }
}
"#,
        );

    let err = check_source(&source).expect_err("mismatched process ref type should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker has type ProcessRef<Helper> but spawns Worker"
    ));
}

#[test]
fn rejects_process_ref_binding_with_non_process_ref_type() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: WorkerState = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("non-ProcessRef spawn binding type should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_wrong_type_constructor() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: WorkerRef<Worker> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("wrong process reference constructor should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_wrong_type_arity() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Worker, Worker> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("wrong process reference type arity should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker must be typed as ProcessRef<ProcessName>"
    ));
}

#[test]
fn rejects_process_ref_binding_with_nested_target_type() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<ProcessRef<Worker>> = spawn Worker;",
    );

    let err =
        check_source(&source).expect_err("nested process reference target should be rejected");

    assert!(err.to_string().contains(
        "process Main process reference worker has nested process reference target type ProcessRef<Worker>"
    ));
}

#[test]
fn rejects_process_ref_type_with_undeclared_process_target() {
    let source = ACTOR_PING.replace(
        "let worker: ProcessRef<Worker> = spawn Worker;",
        "let worker: ProcessRef<Unknown> = spawn Worker;",
    );

    let err = check_source(&source).expect_err("undeclared process ref target should be rejected");

    assert!(
        err.to_string()
            .contains("process Main process reference worker targets undeclared process Unknown")
    );
}

#[test]
fn rejects_send_without_static_spawn() {
    let source = ACTOR_PING
        .replace("! [spawn, send] ~ [] @det", "! [send] ~ [] @det")
        .replace(
            "        let worker: ProcessRef<Worker> = spawn Worker;\n",
            "",
        );

    let err = check_source(&source).expect_err("send without spawn should be rejected");

    assert!(
        err.to_string()
            .contains("sends to undeclared process reference worker")
    );
}

#[test]
fn rejects_mailbox_overflow_through_process_ref() {
    let source = ACTOR_PING.replace(
        "send worker Ping;",
        "send worker Ping;\n        send worker Ping;",
    );

    let err = check_source(&source).expect_err("mailbox overflow should be rejected");

    assert!(
        err.to_string()
            .contains("sends to Worker, but its mailbox would exceed bound 1")
    );
}

#[test]
fn rejects_unhandled_message_after_process_ref_target_stops() {
    let source = ACTOR_SEQUENCE.replace("return Continue(SawFirst);", "return Stop(SawFirst);");

    let err = check_source(&source).expect_err("message left after stop should be rejected");

    assert!(
        err.to_string()
            .contains("process Worker would retain 1 unhandled message(s)")
    );
}

#[test]
fn rejects_send_to_unknown_message() {
    let source = ACTOR_PING.replace("send worker Ping;", "send worker Unknown;");

    let err = check_source(&source).expect_err("unknown message should be rejected");

    assert!(
        err.to_string()
            .contains("sends message Unknown not accepted by Worker")
    );
}

#[test]
fn rejects_unbounded_cross_spawn_loop() {
    let source = r#"
module spawn_loop;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }
enum HelperState { Idle }
enum HelperMsg { Ping }

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
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [spawn, send] ~ [] @det {
        let helper: ProcessRef<Helper> = spawn Helper;
        send helper Ping;
        return Continue(Idle);
    }
}

proc Helper mailbox bounded(1) {
    type State = HelperState;
    type Msg = HelperMsg;

    fn init() -> HelperState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: HelperState, Ping) -> ProcResult<HelperState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Ping;
        return Continue(Idle);
    }
}
"#;

    let err = check_source(source).expect_err("spawn loop should be rejected");

    assert!(
        err.to_string()
            .contains("static runtime process instance limit exceeded")
    );
}

#[test]
fn rejects_emit_output_too_large_for_artifacts() {
    let output = "a".repeat(MAX_FIELD_VALUE_BYTES + 1);
    let source = HELLO.replace("hello from Strata", &output);

    let err = check_source(&source).expect_err("oversized emit output should fail");

    assert!(
        err.to_string()
            .contains("output literal exceeds maximum length")
    );
}

#[test]
fn rejects_bare_concrete_state_return_with_accurate_message() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Handled;");

    let err = check_source(&source).expect_err("bare state return should be rejected");

    let message = err.to_string();
    assert!(message.contains(
        "step body must return Stop(<state value>), Continue(<state value>), or Panic(<state value>)"
    ));
    assert!(!message.contains("or a concrete state value"));
}

#[test]
fn rejects_panic_step_result_with_wrong_state_value() {
    let source = ACTOR_PING.replace("return Stop(Handled);", "return Panic(MainState);");

    let err = check_source(&source).expect_err("panic must carry a WorkerState value");

    assert!(
        err.to_string()
            .contains("value MainState is not a variant of enum WorkerState")
    );
}

#[test]
fn rejects_step_proc_result_with_wrong_state_argument() {
    let source = HELLO.replace("ProcResult<MainState>", "ProcResult<MainMsg>");

    let err = check_source(&source).expect_err("wrong ProcResult argument should fail");

    assert!(
        err.to_string()
            .contains("step returns ProcResult<MainMsg>, expected ProcResult<MainState>")
    );
}

#[test]
fn rejects_reserved_proc_result_type_declarations() {
    for source in [
        HELLO.replace("record MainState;", "record ProcResult;"),
        HELLO.replace("enum MainMsg { Start }", "enum ProcResult { Start }"),
    ] {
        let err = check_source(&source).expect_err("reserved type name should fail");

        assert!(err.to_string().contains("type name ProcResult is reserved"));
    }
}

#[test]
fn rejects_internal_checked_type_label_prefix_declarations() {
    for source in [
        HELLO.replace(
            "record MainState;",
            "record __strata_checked_process_ref_Main;",
        ),
        HELLO.replace(
            "enum MainMsg { Start }",
            "enum __strata_checked_process_ref_Main { Start }",
        ),
    ] {
        let err = check_source(&source).expect_err("reserved type label prefix should fail");

        assert!(
            err.to_string()
                .contains("uses reserved prefix __strata_checked_")
        );
    }
}

#[test]
fn rejects_duplicate_enum_variants() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainMsg { Start, Start }");

    let err = check_source(&source).expect_err("duplicate variant should be rejected");

    assert!(
        err.to_string()
            .contains("duplicate variant in enum MainMsg declaration Start")
    );
}

#[test]
fn rejects_record_enum_type_name_collision() {
    let source = HELLO.replace("enum MainMsg { Start }", "enum MainState { Start }");

    let err = check_source(&source).expect_err("type name collision should be rejected");

    assert!(
        err.to_string()
            .contains("duplicate type declaration MainState used by record and enum")
    );
}

#[test]
fn rejects_invalid_annotation_identifier_start() {
    let source = HELLO.replacen("@det", "@1", 1);

    let err = parse_source(&source).expect_err("invalid annotation should fail lexing");

    assert!(err.to_string().contains("expected identifier after '@'"));
}

fn nested_record_value_source(depth: usize) -> String {
    let mut value = "Leaf".to_string();
    for index in (0..depth).rev() {
        value = format!("State{index} {{ next: {value} }}");
    }
    value
}

fn payload_source_with(send_statement: &str, step_header: &str) -> String {
    format!(
        r#"
module actor_payloads;

record MainState;
record Job {{ phase: JobPhase }}
record WorkerState {{ job: Job }}
enum MainMsg {{ Start }}
enum JobPhase {{ Ready, Done }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        {send_statement}
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return WorkerState {{ job: Job {{ phase: Done }} }};
    }}

    {step_header} -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(WorkerState {{ job: job }});
    }}
}}
"#
    )
}

fn checked_type_count_overflow_module() -> Module {
    let payload_variants_per_process = MAX_MESSAGE_VARIANTS_PER_PROCESS - 1;
    let mut process_count = 1usize;
    while process_count * (payload_variants_per_process + 2) <= MAX_TYPE_COUNT {
        process_count += 1;
    }
    assert!(process_count <= MAX_PROCESS_COUNT);

    let mut records = Vec::new();
    let mut enums = Vec::new();
    let mut processes = Vec::new();

    for process_index in 0..process_count {
        let state_name = format!("State{process_index}");
        let msg_name = format!("Msg{process_index}");
        records.push(Record {
            name: ident(state_name.as_str()),
            fields: Vec::new(),
        });

        let mut variants = vec![EnumVariant {
            name: ident("Start"),
            payload_type: None,
        }];
        for payload_index in 0..payload_variants_per_process {
            let payload_name = format!("Payload{process_index}_{payload_index}");
            records.push(Record {
                name: ident(&payload_name),
                fields: Vec::new(),
            });
            variants.push(EnumVariant {
                name: ident(format!("M{process_index}_{payload_index}")),
                payload_type: Some(TypeRef::Named(ident(payload_name))),
            });
        }
        enums.push(Enum {
            name: ident(msg_name.as_str()),
            variants,
        });

        let process_name = if process_index == 0 {
            "Main".to_string()
        } else {
            format!("P{process_index}")
        };
        let state_type = TypeRef::Named(ident(state_name.as_str()));
        processes.push(Process {
            name: ident(process_name),
            mailbox_bound: 1,
            state_type: state_type.clone(),
            msg_type: TypeRef::Named(ident(msg_name)),
            init: Function {
                name: ident("init"),
                params: Vec::new(),
                return_type: state_type.clone(),
                effects: Vec::new(),
                may: Vec::new(),
                determinism: Determinism::Det,
                body: Some(FunctionBody::Block(FunctionBlock {
                    statements: Vec::new(),
                    returns: ReturnExpr::Value(ValueExpr::Identifier(ident(state_name.as_str()))),
                })),
            },
            steps: vec![Function {
                name: ident("step"),
                params: vec![
                    FunctionParam::Binding(Param {
                        name: ident("state"),
                        ty: state_type.clone(),
                    }),
                    FunctionParam::Pattern(Pattern::Wildcard),
                ],
                return_type: TypeRef::Applied {
                    constructor: ident("ProcResult"),
                    args: vec![state_type],
                },
                effects: Vec::new(),
                may: Vec::new(),
                determinism: Determinism::Det,
                body: Some(FunctionBody::Block(FunctionBlock {
                    statements: Vec::new(),
                    returns: ReturnExpr::Call {
                        name: ident("Stop"),
                        arg: ValueExpr::Identifier(ident("state")),
                    },
                })),
            }],
        });
    }

    Module {
        name: ident("type_count_overflow"),
        records,
        enums,
        processes,
    }
}

fn ident(value: impl Into<String>) -> Identifier {
    Identifier::new(value).expect("test identifier should be valid")
}

fn payload_message_label_overflow_source() -> String {
    let field_names = payload_overflow_field_names();
    let record_fields = field_names
        .iter()
        .map(|name| format!("    {name}: Phase,\n"))
        .collect::<String>();
    let payload_fields = field_names
        .iter()
        .map(|name| format!("            {name}: Ready,\n"))
        .collect::<String>();

    format!(
        r#"
module payload_label_limit;

record MainState;
record WorkerState;
enum Phase {{ Ready }}
record Job {{
{record_fields}}}
enum MainMsg {{ Start }}
enum WorkerMsg {{ Assign(Job) }}

proc Main mailbox bounded(16) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Assign(Job {{
{payload_fields}        }});
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(16) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return WorkerState;
    }}

    fn step(state: WorkerState, Assign(job: Job)) -> ProcResult<WorkerState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#
    )
}

fn payload_overflow_field_names() -> Vec<String> {
    let mut field_names = (0..MAX_VALUE_TEMPLATE_FIELDS)
        .map(|index| format!("f{index}"))
        .collect::<Vec<_>>();
    let target_payload_len = MAX_FIELD_VALUE_BYTES - "Assign()".len() + 1;
    let mut payload_len = payload_record_label(&field_names).len();

    for field_name in &mut field_names {
        while payload_len < target_payload_len && field_name.len() < MAX_IDENTIFIER_BYTES {
            field_name.push('x');
            payload_len += 1;
        }
        if payload_len == target_payload_len {
            break;
        }
    }

    let payload_label = payload_record_label(&field_names);
    let message_label = format!("Assign({payload_label})");
    assert!(payload_label.len() <= MAX_FIELD_VALUE_BYTES);
    assert!(message_label.len() > MAX_FIELD_VALUE_BYTES);

    field_names
}

fn payload_record_label(field_names: &[String]) -> String {
    let fields = field_names
        .iter()
        .map(|name| format!("{name}:Ready"))
        .collect::<Vec<_>>()
        .join(",");
    format!("Job{{{fields}}}")
}

fn checked_process_id(index: usize) -> CheckedProcessId {
    CheckedProcessId::from_index(index).expect("valid checked process id")
}

fn checked_process_ref_id(index: usize) -> CheckedProcessRefId {
    CheckedProcessRefId::from_index(index).expect("valid checked process reference id")
}

fn checked_state_id(index: usize) -> CheckedStateId {
    CheckedStateId::from_index(index).expect("valid checked state id")
}

fn checked_message_id(index: usize) -> CheckedMessageId {
    CheckedMessageId::from_index(index).expect("valid checked message id")
}

fn checked_output_id(index: usize) -> CheckedOutputId {
    CheckedOutputId::from_index(index).expect("valid checked output id")
}

fn checked_state_labels(process: &CheckedProcess) -> Vec<&str> {
    process
        .state_values()
        .iter()
        .map(|state| state.label())
        .collect()
}

fn artifact_state_labels(process: &mantle_artifact::ArtifactProcess) -> Vec<&str> {
    process
        .state_values
        .iter()
        .map(|state| state.label.as_str())
        .collect()
}

fn artifact_type_id(artifact: &MantleArtifact, label: &str) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.label == label)
        .unwrap_or_else(|| panic!("artifact type {label} should exist"));
    TypeId::from_index(index).expect("artifact type index should fit")
}

fn artifact_process_ref_type_id(artifact: &MantleArtifact, target: ProcessId) -> TypeId {
    let index = artifact
        .types
        .iter()
        .position(|ty| ty.kind == ArtifactTypeKind::ProcessRef { target })
        .unwrap_or_else(|| {
            panic!(
                "artifact process reference type targeting process {} should exist",
                target.as_u32()
            )
        });
    TypeId::from_index(index).expect("artifact type index should fit")
}

fn repeated_emit_statements(count: usize, indent: usize) -> String {
    let padding = " ".repeat(indent);
    let mut statements = String::new();
    for _ in 0..count {
        statements.push_str(&padding);
        statements.push_str("emit \"hello from Strata\";\n");
    }
    statements
}

fn only_transition(process: &CheckedProcess) -> &CheckedTransition {
    assert_eq!(
        process.transitions().len(),
        1,
        "expected exactly one checked transition for {}",
        process.debug_name()
    );
    &process.transitions()[0]
}
