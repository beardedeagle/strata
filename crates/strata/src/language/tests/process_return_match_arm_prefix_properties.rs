use super::support::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmPrefixKind {
    None,
    Emit,
    Send,
    EmitThenSend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    Spawn,
    Emit,
    Send,
    IfElse,
    ForEach,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmActionBlockKind {
    MultipleIf,
    MultipleFor,
    IfWithFor,
    IfWithForNestedLoopIf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundedArmStatementKind {
    IfElse,
    ForEach,
    IfWithFor,
    ForWithIf,
}

const ARM_PREFIX_KINDS: [ArmPrefixKind; 4] = [
    ArmPrefixKind::None,
    ArmPrefixKind::Emit,
    ArmPrefixKind::Send,
    ArmPrefixKind::EmitThenSend,
];

const ARM_ACTION_BLOCK_KINDS: [ArmActionBlockKind; 4] = [
    ArmActionBlockKind::MultipleIf,
    ArmActionBlockKind::MultipleFor,
    ArmActionBlockKind::IfWithFor,
    ArmActionBlockKind::IfWithForNestedLoopIf,
];

const BOUNDED_ARM_STATEMENT_KINDS: [BoundedArmStatementKind; 4] = [
    BoundedArmStatementKind::IfElse,
    BoundedArmStatementKind::ForEach,
    BoundedArmStatementKind::IfWithFor,
    BoundedArmStatementKind::ForWithIf,
];

#[test]
fn property_generated_uniform_arm_prefix_shapes_lower_as_typed_actions() {
    for kind in ARM_PREFIX_KINDS {
        let source = arm_prefix_property_source(
            format!("process_return_match_arm_prefix_property_{}", kind.name()).as_str(),
            kind,
            kind,
            effects_for_kind(kind),
        );
        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("generated {kind:?} source should check: {err}"));
        let worker = checked_process(&checked, "Worker");
        let expected_effects = effects_for_kind(kind);
        let expected_actions = actions_for_kind(kind);

        assert_eq!(
            worker.transitions().len(),
            2,
            "generated {kind:?} source should create one transition per concrete payload"
        );
        for transition in worker.transitions() {
            assert_eq!(
                transition.effects(),
                expected_effects,
                "generated {kind:?} transition should retain exact declared effects"
            );
            assert_eq!(
                checked_action_kinds(transition.actions()),
                expected_actions,
                "generated {kind:?} transition should lower only uniform spawn plus selected arm actions"
            );
        }

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated {kind:?} source should lower: {err}"));
        let worker_artifact = artifact
            .processes
            .iter()
            .find(|process| process.debug_name == "Worker")
            .expect("Worker artifact process should exist");
        for transition in &worker_artifact.transitions {
            assert_eq!(
                transition.effects.to_vec(),
                expected_effects
                    .iter()
                    .copied()
                    .map(artifact_effect_for)
                    .collect::<Vec<_>>(),
                "generated {kind:?} artifact transition should retain exact typed effects"
            );
            assert_eq!(
                artifact_action_kinds(&transition.actions),
                expected_actions,
                "generated {kind:?} artifact transition should contain typed action variants"
            );
            assert_artifact_send_actions_use_ids(&transition.actions);
        }
    }
}

#[test]
fn property_divergent_arm_prefix_effect_sets_fail_closed() {
    for ready in ARM_PREFIX_KINDS {
        for done in ARM_PREFIX_KINDS {
            if ready == done {
                continue;
            }
            let declared_effects = union_effects(ready, done);
            let source = arm_prefix_property_source(
                format!(
                    "process_return_match_arm_prefix_reject_{}_{}",
                    ready.name(),
                    done.name()
                )
                .as_str(),
                ready,
                done,
                &declared_effects,
            );

            let err = match check_source(&source) {
                Ok(_) => {
                    panic!("divergent generated arms {ready:?}/{done:?} should fail closed")
                }
                Err(err) => err,
            };
            let err = err.to_string();
            assert!(
                err.contains("declares effect") && err.contains("does not use it"),
                "divergent generated arms {ready:?}/{done:?} failed for unexpected reason: {err}"
            );
        }
    }
}

#[test]
fn property_generated_selected_arm_action_block_shapes_lower_as_typed_actions() {
    for kind in ARM_ACTION_BLOCK_KINDS {
        let source = arm_action_block_property_source(
            format!(
                "process_return_match_arm_action_block_property_{}",
                kind.name()
            )
            .as_str(),
            kind,
        );
        let checked = check_source(&source).unwrap_or_else(|err| {
            panic!("generated selected-arm {kind:?} source should check: {err}")
        });
        let worker = checked_process(&checked, "Worker");
        let expected_actions = kind.top_level_actions();

        assert_eq!(
            worker.transitions().len(),
            2,
            "generated {kind:?} source should create one transition per concrete payload"
        );
        for transition in worker.transitions() {
            assert_eq!(
                transition.effects(),
                kind.effects(),
                "generated {kind:?} transition should retain exact declared effects"
            );
            assert_eq!(
                checked_action_kinds(transition.actions()),
                expected_actions,
                "generated {kind:?} transition should lower selected arm as typed action-block actions"
            );
        }

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated selected-arm {kind:?} should lower: {err}"));
        let worker_artifact = artifact
            .processes
            .iter()
            .find(|process| process.debug_name == "Worker")
            .expect("Worker artifact process should exist");
        for transition in &worker_artifact.transitions {
            assert_eq!(
                artifact_action_kinds(&transition.actions),
                expected_actions,
                "generated {kind:?} artifact transition should preserve typed action variants"
            );
            assert_artifact_send_actions_use_ids(&transition.actions);
            assert_nested_artifact_send_actions_use_ids(&transition.actions);
        }
        let encoded = artifact.encode();
        assert!(
            !encoded.lines().any(|line| line.contains("job_phase")),
            "generated {kind:?} artifact must not dispatch through source loop binding job_phase"
        );
    }
}

#[test]
fn exhaustive_bounded_selected_arm_action_blocks_lower_as_typed_artifact_actions() {
    let mut sequence = Vec::new();
    for len in 0..=2 {
        visit_bounded_arm_statement_sequences(len, &mut sequence, &mut |sequence| {
            assert_bounded_arm_statement_sequence(sequence);
        });
    }
}

#[test]
fn exhaustive_bounded_invalid_selected_arm_action_blocks_fail_closed() {
    for case in BOUNDED_INVALID_ARM_CASES {
        let source = invalid_bounded_arm_action_block_source(case);
        let err = match check_source(&source) {
            Ok(_) => panic!("invalid bounded arm case {case:?} should fail"),
            Err(err) => err,
        };
        let message = err.to_string();

        assert!(
            message.contains(case.expected_diagnostic()),
            "invalid bounded arm case {case:?} failed for unexpected reason: {message}"
        );
    }
}

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

fn checked_process<'a>(checked: &'a CheckedProgram, name: &str) -> &'a CheckedProcess {
    checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == name)
        .unwrap_or_else(|| panic!("checked process {name} should exist"))
}

fn checked_action_kinds(actions: &[CheckedAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            CheckedAction::Spawn { .. } => ActionKind::Spawn,
            CheckedAction::Emit { .. } => ActionKind::Emit,
            CheckedAction::Send { .. } => ActionKind::Send,
            CheckedAction::IfElse { .. } => ActionKind::IfElse,
            CheckedAction::ForEach { .. } => ActionKind::ForEach,
        })
        .collect()
}

fn artifact_action_kinds(actions: &[ArtifactAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            ArtifactAction::Spawn { .. } => ActionKind::Spawn,
            ArtifactAction::Emit { .. } => ActionKind::Emit,
            ArtifactAction::Send { .. } => ActionKind::Send,
            ArtifactAction::IfElse { .. } => ActionKind::IfElse,
            ArtifactAction::ForEach { .. } => ActionKind::ForEach,
        })
        .collect()
}

fn assert_artifact_send_actions_use_ids(actions: &[ArtifactAction]) {
    for action in actions {
        let ArtifactAction::Send {
            target, message, ..
        } = action
        else {
            continue;
        };
        assert_eq!(
            *target,
            ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            "arm-local send should lower through a process-ref id, not a source target name"
        );
        assert_eq!(
            *message,
            MessageId::new(0),
            "arm-local send should lower through a message id, not a source message name"
        );
    }
}

fn assert_nested_artifact_send_actions_use_ids(actions: &[ArtifactAction]) {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                assert_artifact_send_actions_use_ids(then_actions);
                assert_artifact_send_actions_use_ids(else_actions);
                assert_nested_artifact_send_actions_use_ids(then_actions);
                assert_nested_artifact_send_actions_use_ids(else_actions);
            }
            ArtifactAction::ForEach { body, .. } => {
                assert_artifact_send_actions_use_ids(body);
                assert_nested_artifact_send_actions_use_ids(body);
            }
            ArtifactAction::Spawn { .. }
            | ArtifactAction::Emit { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
}

fn effects_for_kind(kind: ArmPrefixKind) -> &'static [Effect] {
    match kind {
        ArmPrefixKind::None => &[Effect::Spawn],
        ArmPrefixKind::Emit => &[Effect::Emit, Effect::Spawn],
        ArmPrefixKind::Send => &[Effect::Spawn, Effect::Send],
        ArmPrefixKind::EmitThenSend => &[Effect::Emit, Effect::Spawn, Effect::Send],
    }
}

fn actions_for_kind(kind: ArmPrefixKind) -> Vec<ActionKind> {
    let mut actions = vec![ActionKind::Spawn];
    match kind {
        ArmPrefixKind::None => {}
        ArmPrefixKind::Emit => actions.push(ActionKind::Emit),
        ArmPrefixKind::Send => actions.push(ActionKind::Send),
        ArmPrefixKind::EmitThenSend => {
            actions.push(ActionKind::Emit);
            actions.push(ActionKind::Send);
        }
    }
    actions
}

fn union_effects(left: ArmPrefixKind, right: ArmPrefixKind) -> Vec<Effect> {
    let mut effects = vec![Effect::Spawn];
    if [left, right].iter().any(|kind| kind.uses_emit()) {
        effects.insert(0, Effect::Emit);
    }
    if [left, right].iter().any(|kind| kind.uses_send()) {
        effects.push(Effect::Send);
    }
    effects
}

fn effects_source(effects: &[Effect]) -> String {
    let effects = effects
        .iter()
        .map(|effect| match effect {
            Effect::Emit => "emit",
            Effect::Spawn => "spawn",
            Effect::Send => "send",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{effects}]")
}

fn artifact_effect_for(effect: Effect) -> ArtifactEffect {
    match effect {
        Effect::Emit => ArtifactEffect::Emit,
        Effect::Spawn => ArtifactEffect::Spawn,
        Effect::Send => ArtifactEffect::Send,
    }
}

impl ArmPrefixKind {
    fn name(self) -> &'static str {
        match self {
            ArmPrefixKind::None => "none",
            ArmPrefixKind::Emit => "emit",
            ArmPrefixKind::Send => "send",
            ArmPrefixKind::EmitThenSend => "emit_send",
        }
    }

    fn statements(self, label: &str) -> String {
        match self {
            ArmPrefixKind::None => String::new(),
            ArmPrefixKind::Emit => format!("                emit \"{label} arm prefix\";\n"),
            ArmPrefixKind::Send => "                send sink Ack;\n".to_string(),
            ArmPrefixKind::EmitThenSend => {
                format!(
                    "                emit \"{label} arm prefix\";\n                send sink Ack;\n"
                )
            }
        }
    }

    fn uses_emit(self) -> bool {
        matches!(self, ArmPrefixKind::Emit | ArmPrefixKind::EmitThenSend)
    }

    fn uses_send(self) -> bool {
        matches!(self, ArmPrefixKind::Send | ArmPrefixKind::EmitThenSend)
    }
}

impl BoundedArmStatementKind {
    fn name(self) -> &'static str {
        match self {
            Self::IfElse => "if",
            Self::ForEach => "for",
            Self::IfWithFor => "if_for",
            Self::ForWithIf => "for_if",
        }
    }

    fn top_level_action(self) -> ActionKind {
        match self {
            Self::IfElse | Self::IfWithFor => ActionKind::IfElse,
            Self::ForEach | Self::ForWithIf => ActionKind::ForEach,
        }
    }

    fn statement(self, index: usize) -> String {
        match self {
            Self::IfElse => format!(
                "                if (selected_enabled == True) {{\n                    emit \"bounded if {index} then\";\n                    send sink Ack;\n                }} else {{\n                    emit \"bounded if {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForEach => format!(
                "                for Job {{ phase: selected_item_phase, urgent: selected_item_urgent }} in selected_jobs {{\n                    emit \"bounded for {index} item\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::IfWithFor => format!(
                "                if (selected_enabled == True) {{\n                    emit \"bounded if-for {index} branch\";\n                    for Job {{ phase: selected_item_phase, urgent: selected_item_urgent }} in selected_jobs {{\n                        emit \"bounded if-for {index} item\";\n                        send sink Ack;\n                    }}\n                }} else {{\n                    emit \"bounded if-for {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForWithIf => format!(
                "                for Job {{ phase: selected_item_phase, urgent: selected_item_urgent }} in selected_jobs {{\n                    if (selected_item_urgent == True) {{\n                        emit \"bounded for-if {index} then\";\n                        send sink Ack;\n                    }} else {{\n                        emit \"bounded for-if {index} else\";\n                        send sink Ack;\n                    }}\n                }}\n"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BoundedInvalidArmCase {
    name: &'static str,
    statements: &'static str,
    effects: &'static str,
    expected: &'static str,
}

const BOUNDED_INVALID_ARM_CASES: [BoundedInvalidArmCase; 3] = [
    BoundedInvalidArmCase {
        name: "nested_for",
        statements: "                for Job { phase: selected_item_phase, urgent: selected_item_urgent } in selected_jobs {\n                    for Job { phase: selected_nested_phase, urgent: selected_nested_urgent } in selected_jobs {\n                        emit \"invalid nested loop\";\n                    }\n                }\n",
        effects: "[emit, spawn]",
        expected: "nested for loops are not supported in this source slice",
    },
    BoundedInvalidArmCase {
        name: "too_deep_if",
        statements: "                if (selected_enabled == True) {\n                    if (selected_enabled == True) {\n                        if (selected_enabled == True) {\n                            emit \"invalid deep branch\";\n                        } else {\n                            emit \"invalid deep fallback\";\n                        }\n                    } else {\n                        emit \"middle fallback\";\n                    }\n                } else {\n                    emit \"outer fallback\";\n                }\n",
        effects: "[emit, spawn]",
        expected: "statement-level if action nesting exceeds maximum depth",
    },
    BoundedInvalidArmCase {
        name: "branch_process_ref",
        statements: "                if (selected_enabled == True) {\n                    let branch_sink: ProcessRef<Sink> = spawn Sink;\n                    emit \"invalid branch process ref\";\n                } else {\n                    emit \"branch fallback\";\n                }\n",
        effects: "[emit, spawn]",
        expected: "statement-level if branches cannot bind process references",
    },
];

impl BoundedInvalidArmCase {
    const fn expected_diagnostic(self) -> &'static str {
        self.expected
    }
}

fn invalid_bounded_arm_action_block_source(case: BoundedInvalidArmCase) -> String {
    format!(
        r#"
module process_return_match_arm_invalid_{name};

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
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(1) {{
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
                emit "done";
                return Stop(SawDone);
            }}
        }};
    }}
}}

proc Sink mailbox bounded(1) {{
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
        name = case.name,
        effects = case.effects,
        statements = case.statements,
    )
}

impl ArmActionBlockKind {
    fn name(self) -> &'static str {
        match self {
            Self::MultipleIf => "multiple_if",
            Self::MultipleFor => "multiple_for",
            Self::IfWithFor => "if_with_for",
            Self::IfWithForNestedLoopIf => "if_with_for_nested_loop_if",
        }
    }

    fn effects(self) -> &'static [Effect] {
        match self {
            Self::MultipleIf => &[Effect::Emit, Effect::Spawn],
            Self::MultipleFor | Self::IfWithFor | Self::IfWithForNestedLoopIf => {
                &[Effect::Emit, Effect::Spawn, Effect::Send]
            }
        }
    }

    fn top_level_actions(self) -> Vec<ActionKind> {
        match self {
            Self::MultipleIf => vec![ActionKind::Spawn, ActionKind::IfElse, ActionKind::IfElse],
            Self::MultipleFor => vec![ActionKind::Spawn, ActionKind::ForEach, ActionKind::ForEach],
            Self::IfWithFor | Self::IfWithForNestedLoopIf => {
                vec![ActionKind::Spawn, ActionKind::IfElse]
            }
        }
    }

    fn statements(self) -> &'static str {
        match self {
            Self::MultipleIf => {
                "                if (enabled == True) {\n                    emit \"first selected branch\";\n                } else {\n                    emit \"first selected fallback\";\n                }\n                if (phase == Ready) {\n                    emit \"second selected branch\";\n                } else {\n                    emit \"second selected fallback\";\n                }\n"
            }
            Self::MultipleFor => {
                "                for Job { phase: job_phase } in jobs {\n                    emit \"first selected loop\";\n                    send sink Ack;\n                }\n                for Job { phase: job_phase } in jobs {\n                    emit \"second selected loop\";\n                    send sink Ack;\n                }\n"
            }
            Self::IfWithFor => {
                "                if (enabled == True) {\n                    emit \"selected branch loop\";\n                    for Job { phase: job_phase } in jobs {\n                        emit \"selected branch loop item\";\n                        send sink Ack;\n                    }\n                } else {\n                    emit \"selected fallback\";\n                }\n"
            }
            Self::IfWithForNestedLoopIf => {
                "                if (enabled == True) {\n                    emit \"selected branch loop\";\n                    for Job { phase: job_phase } in jobs {\n                        if (job_phase == Ready) {\n                            if (enabled == True) {\n                                emit \"selected nested branch loop item\";\n                                send sink Ack;\n                            } else {\n                                emit \"selected nested branch loop fallback\";\n                                send sink Ack;\n                            }\n                        } else {\n                            emit \"selected outer branch loop fallback\";\n                            send sink Ack;\n                        }\n                    }\n                } else {\n                    emit \"selected fallback\";\n                }\n"
            }
        }
    }
}
