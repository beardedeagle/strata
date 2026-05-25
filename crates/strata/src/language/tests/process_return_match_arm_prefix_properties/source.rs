fn arm_prefix_property_source(
    module_name: &str,
    ready: ArmPrefixKind,
    done: ArmPrefixKind,
    effects: &[Effect],
) -> String {
    format!(
        r#"
module {module_name};

record MainState;
record SinkState;
enum MainMsg {{ Start }}
enum Phase {{ Ready, Done }}
enum Route {{ Assign(Phase) }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Route) }}
enum SinkMsg {{ Ack }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! {effects_source} ~ [] @det {{
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {{
            Ready => {{
{ready_statements}                return Continue(SawReady);
            }}
            Done => {{
{done_statements}                return Stop(Done);
            }}
        }};
    }}
}}

proc Sink mailbox bounded(2) {{
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {{
        return SinkState;
    }}

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#,
        effects_source = effects_source(effects),
        ready_statements = ready.statements("ready"),
        done_statements = done.statements("done"),
    )
}

fn arm_action_block_property_source(module_name: &str, kind: ArmActionBlockKind) -> String {
    let effects = effects_source(kind.effects());
    let statements = kind.statements();
    format!(
        r#"
module {module_name};

record MainState;
record SinkState;
record Job {{
    phase: Phase,
}}
record Assignment {{
    phase: Phase,
    enabled: Bool,
    jobs: List<Job,1>,
}}
enum Bool {{ False, True }}
enum MainMsg {{ Start }}
enum Phase {{ Ready, Done }}
enum Route {{ Assign(Assignment) }}
enum WorkerState {{ Idle, SawReady, SawDone }}
enum WorkerMsg {{ Envelope(Route) }}
enum SinkMsg {{ Ack }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Assignment {{
            phase: Ready,
            enabled: True,
            jobs: List<Job,1>[Job {{ phase: Ready }}],
        }}));
        send worker Envelope(Assign(Assignment {{
            phase: Done,
            enabled: False,
            jobs: List<Job,1>[Job {{ phase: Done }}],
        }}));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Envelope(Assign(Assignment {{ phase: phase, enabled: enabled, jobs: jobs }}))) -> ProcResult<WorkerState> ! {effects} ~ [] @det {{
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {{
            Ready => {{
{ready_statements}                return Continue(SawReady);
            }}
            Done => {{
{done_statements}                return Stop(SawDone);
            }}
        }};
    }}
}}

proc Sink mailbox bounded(2) {{
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {{
        return SinkState;
    }}

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {{
        return Continue(state);
    }}
}}
"#,
        ready_statements = statements,
        done_statements = statements,
    )
}

fn visit_bounded_arm_statement_sequences(
    remaining: usize,
    sequence: &mut Vec<BoundedArmStatementKind>,
    visit: &mut impl FnMut(&[BoundedArmStatementKind]),
) {
    if remaining == 0 {
        visit(sequence);
        return;
    }
    for kind in BOUNDED_ARM_STATEMENT_KINDS {
        sequence.push(kind);
        visit_bounded_arm_statement_sequences(remaining - 1, sequence, visit);
        sequence.pop();
    }
}

fn assert_bounded_arm_statement_sequence(sequence: &[BoundedArmStatementKind]) {
    let module_name = bounded_sequence_module_name(sequence);
    let source = bounded_arm_action_block_source(&module_name, sequence);
    let checked = check_source(&source).unwrap_or_else(|err| {
        panic!("bounded selected-arm sequence {sequence:?} should check: {err}")
    });
    let worker = checked_process(&checked, "Worker");
    let expected_actions = bounded_top_level_actions(sequence);

    assert_eq!(
        worker.transitions().len(),
        2,
        "bounded selected-arm sequence {sequence:?} should source-select one transition per payload"
    );
    for transition in worker.transitions() {
        assert_eq!(
            checked_action_kinds(transition.actions()),
            expected_actions,
            "bounded selected-arm sequence {sequence:?} should preserve top-level action order"
        );
    }

    let artifact = lower_to_artifact(&checked, &source).unwrap_or_else(|err| {
        panic!("bounded selected-arm sequence {sequence:?} should lower: {err}")
    });
    let worker_artifact = artifact
        .processes
        .iter()
        .find(|process| process.debug_name == "Worker")
        .expect("Worker artifact process should exist");
    for transition in &worker_artifact.transitions {
        assert_eq!(
            artifact_action_kinds(&transition.actions),
            expected_actions,
            "bounded selected-arm sequence {sequence:?} should lower to the same typed artifact actions"
        );
        assert_artifact_send_actions_use_ids(&transition.actions);
        assert_nested_artifact_send_actions_use_ids(&transition.actions);
    }

    let encoded = artifact.encode();
    for source_only in [
        "selected_phase",
        "selected_enabled",
        "selected_jobs",
        "selected_item_phase",
        "selected_item_urgent",
    ] {
        assert!(
            !encoded.lines().any(|line| line.contains(source_only)),
            "bounded selected-arm sequence {sequence:?} must not lower source binding name {source_only}"
        );
    }
}

fn bounded_arm_action_block_source(
    module_name: &str,
    sequence: &[BoundedArmStatementKind],
) -> String {
    let statements = bounded_arm_statements(sequence);
    let effects = if sequence.is_empty() {
        "[spawn]"
    } else {
        "[emit, spawn, send]"
    };
    format!(
        r#"
module {module_name};

record MainState;
record SinkState;
record Job {{
    phase: Phase,
    urgent: Bool,
}}
record Assignment {{
    phase: Phase,
    enabled: Bool,
    jobs: List<Job,1>,
}}
enum Bool {{ False, True }}
enum MainMsg {{ Start }}
enum Phase {{ Ready, Done }}
enum Route {{ Assign(Assignment) }}
enum WorkerState {{ Idle, SawReady, SawDone }}
enum WorkerMsg {{ Envelope(Route) }}
enum SinkMsg {{ Ack }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Assignment {{
            phase: Ready,
            enabled: True,
            jobs: List<Job,1>[Job {{ phase: Ready, urgent: True }}],
        }}));
        send worker Envelope(Assign(Assignment {{
            phase: Done,
            enabled: False,
            jobs: List<Job,1>[Job {{ phase: Done, urgent: False }}],
        }}));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Envelope(Assign(Assignment {{ phase: selected_phase, enabled: selected_enabled, jobs: selected_jobs }}))) -> ProcResult<WorkerState> ! {effects} ~ [] @det {{
        let sink: ProcessRef<Sink> = spawn Sink;
        return match selected_phase {{
            Ready => {{
{statements}                return Continue(SawReady);
            }}
            Done => {{
{statements}                return Stop(SawDone);
            }}
        }};
    }}
}}

proc Sink mailbox bounded(2) {{
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {{
        return SinkState;
    }}

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {{
        return Continue(state);
    }}
}}
"#
    )
}

fn bounded_arm_statements(sequence: &[BoundedArmStatementKind]) -> String {
    sequence
        .iter()
        .enumerate()
        .map(|(index, kind)| kind.statement(index))
        .collect::<String>()
}

fn bounded_sequence_module_name(sequence: &[BoundedArmStatementKind]) -> String {
    if sequence.is_empty() {
        return "process_return_match_arm_bounded_empty".to_string();
    }
    let suffix = sequence
        .iter()
        .map(|kind| kind.name())
        .collect::<Vec<_>>()
        .join("_");
    format!("process_return_match_arm_bounded_{suffix}")
}

fn bounded_top_level_actions(sequence: &[BoundedArmStatementKind]) -> Vec<ActionKind> {
    let mut actions = Vec::with_capacity(sequence.len() + 1);
    actions.push(ActionKind::Spawn);
    actions.extend(sequence.iter().map(|kind| kind.top_level_action()));
    actions
}

