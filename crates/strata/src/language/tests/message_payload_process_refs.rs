use super::support::*;

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

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority spawn_sink: Cap<Spawn<Sink>>;

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
            *ty.kind()
                == (CheckedTypeKind::ProcessRef {
                    target: checked_process_id(2),
                })
        })
        .expect("checked type table should contain Sink process reference type");
    assert_eq!(checked_sink_ref.label(), "__strata_checked_process_ref_2");
    assert_eq!(
        checked_sink_ref.kind(),
        &CheckedTypeKind::ProcessRef {
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
            port: None,
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
            port: None,
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
            *ty.kind()
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
