use super::support::*;
use mantle_artifact::{ArtifactPayload, ArtifactProcess, ArtifactTransition, StateId};

const MAX_MODEL_SEQUENCE_LEN: usize = 3;

const MODEL_STATEMENTS: [ModelStatement; 7] = [
    ModelStatement::Emit,
    ModelStatement::Send,
    ModelStatement::IfElse,
    ModelStatement::ForEach,
    ModelStatement::IfWithFor,
    ModelStatement::ForWithIf,
    ModelStatement::IfWithForNestedIf,
];

const SOURCE_ONLY_BINDINGS: [&str; 7] = [
    "selected_phase",
    "selected_enabled",
    "selected_jobs",
    "selected_item_phase",
    "selected_item_urgent",
    "assurance_item_phase",
    "assurance_item_urgent",
];

const ACTION_BLOCK_TERMINAL_PROFILE: TerminalProfile = TerminalProfile {
    ready: ModelTerminal::ContinueSawReady,
    done: ModelTerminal::StopSawDone,
};

const TERMINAL_CASES: [ModelTerminal; 3] = [
    ModelTerminal::ContinueSawReady,
    ModelTerminal::StopSawReady,
    ModelTerminal::PanicSawReady,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelStatement {
    Emit,
    Send,
    IfElse,
    ForEach,
    IfWithFor,
    ForWithIf,
    IfWithForNestedIf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ActionShape {
    Emit,
    Spawn,
    Send,
    IfElse {
        then_actions: Vec<ActionShape>,
        else_actions: Vec<ActionShape>,
    },
    ForEach {
        body: Vec<ActionShape>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectedArm {
    Ready,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalProfile {
    ready: ModelTerminal,
    done: ModelTerminal,
}

impl TerminalProfile {
    const fn for_arm(self, arm: SelectedArm) -> ModelTerminal {
        match arm {
            SelectedArm::Ready => self.ready,
            SelectedArm::Done => self.done,
        }
    }

    const fn send_done_message(self) -> bool {
        matches!(self.ready, ModelTerminal::ContinueSawReady)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelTerminal {
    ContinueSawReady,
    StopSawReady,
    PanicSawReady,
    StopSawDone,
}

impl ModelTerminal {
    const fn source(self) -> &'static str {
        match self {
            Self::ContinueSawReady => "Continue(SawReady)",
            Self::StopSawReady => "Stop(SawReady)",
            Self::PanicSawReady => "Panic(SawReady)",
            Self::StopSawDone => "Stop(SawDone)",
        }
    }

    const fn checked_step_result(self) -> CheckedStepResult {
        match self {
            Self::ContinueSawReady => CheckedStepResult::Continue,
            Self::StopSawReady | Self::StopSawDone => CheckedStepResult::Stop,
            Self::PanicSawReady => CheckedStepResult::Panic,
        }
    }

    const fn artifact_step_result(self) -> StepResult {
        match self {
            Self::ContinueSawReady => StepResult::Continue,
            Self::StopSawReady | Self::StopSawDone => StepResult::Stop,
            Self::PanicSawReady => StepResult::Panic,
        }
    }

    const fn state_label(self) -> &'static str {
        match self {
            Self::ContinueSawReady | Self::StopSawReady | Self::PanicSawReady => "SawReady",
            Self::StopSawDone => "SawDone",
        }
    }
}

#[derive(Clone, Copy)]
struct InvalidSourceCase {
    name: &'static str,
    effects: &'static str,
    ready_statements: &'static str,
    done_statements: &'static str,
    ready_return: &'static str,
    done_return: &'static str,
    send_done_message: bool,
    expected: &'static str,
}

const INVALID_SOURCE_CASES: [InvalidSourceCase; 7] = [
    InvalidSourceCase {
        name: "selected_missing_send_effect",
        effects: "[spawn]",
        ready_statements: "                send sink Ack;\n",
        done_statements: "",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "uses effect send but does not declare it",
    },
    InvalidSourceCase {
        name: "unselected_invalid_send_payload",
        effects: "[spawn, send]",
        ready_statements: "",
        done_statements: "                send sink Ack(Ready);\n",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "sends payload to message Ack, which does not accept one",
    },
    InvalidSourceCase {
        name: "nested_for",
        effects: "[emit, spawn]",
        ready_statements: "                for Job { phase: selected_item_phase, urgent: selected_item_urgent } in selected_jobs {\n                    for Job { phase: nested_item_phase, urgent: nested_item_urgent } in selected_jobs {\n                        emit \"invalid nested loop\";\n                    }\n                }\n",
        done_statements: "",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "nested for loops are not supported in this source slice",
    },
    InvalidSourceCase {
        name: "too_deep_runtime_if",
        effects: "[emit, spawn]",
        ready_statements: "                if (selected_enabled == True) {\n                    if (selected_enabled == True) {\n                        if (selected_enabled == True) {\n                            emit \"invalid deep branch\";\n                        } else {\n                            emit \"invalid deep fallback\";\n                        }\n                    } else {\n                        emit \"middle fallback\";\n                    }\n                } else {\n                    emit \"outer fallback\";\n                }\n",
        done_statements: "",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "statement-level if action nesting exceeds maximum depth",
    },
    InvalidSourceCase {
        name: "branch_process_ref",
        effects: "[emit, spawn]",
        ready_statements: "                if (selected_enabled == True) {\n                    let branch_sink: ProcessRef<Sink> = spawn Sink;\n                    emit \"invalid branch process ref\";\n                } else {\n                    emit \"branch fallback\";\n                }\n",
        done_statements: "",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "statement-level if branches cannot bind process references",
    },
    InvalidSourceCase {
        name: "empty_runtime_if",
        effects: "[spawn]",
        ready_statements: "                if (selected_enabled == True) {\n                } else {\n                }\n",
        done_statements: "",
        ready_return: "Continue(SawReady)",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "statement-level if branches cannot both be empty",
    },
    InvalidSourceCase {
        name: "final_runtime_if",
        effects: "[spawn]",
        ready_statements: "",
        done_statements: "",
        ready_return: "if (selected_enabled == True) { return Continue(SawReady); } else { return Stop(SawDone); }",
        done_return: "Stop(SawDone)",
        send_done_message: false,
        expected: "if branches are pure value expressions and must not perform statements",
    },
];

#[test]
fn bounded_assurance_selected_arm_action_blocks_match_checked_ir_and_artifacts() {
    let mut sequence = Vec::with_capacity(MAX_MODEL_SEQUENCE_LEN);
    let mut case_index = 0usize;
    for len in 0..=MAX_MODEL_SEQUENCE_LEN {
        visit_model_sequences(len, &mut sequence, &mut |sequence| {
            assert_valid_model_sequence(case_index, sequence, ACTION_BLOCK_TERMINAL_PROFILE);
            case_index = case_index
                .checked_add(1)
                .expect("bounded model case count should not overflow");
        });
    }

    assert_eq!(case_index, expected_model_case_count());
}

#[test]
fn bounded_assurance_terminal_variants_lower_checked_ir_and_artifacts() {
    for (index, terminal) in TERMINAL_CASES.into_iter().enumerate() {
        let module_name = format!("process_return_match_arm_assurance_terminal_{index}");
        let profile = TerminalProfile {
            ready: terminal,
            done: ModelTerminal::StopSawDone,
        };
        let source = source_with_arm_bodies(
            &module_name,
            "[spawn]",
            "",
            "",
            terminal.source(),
            ModelTerminal::StopSawDone.source(),
            false,
        );
        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should check: {err}"));
        let worker = checked_process(&checked, "Worker");
        assert_eq!(
            worker.transitions().len(),
            1,
            "terminal case {terminal:?} should select the reachable Ready arm"
        );
        assert_checked_terminal(worker, &worker.transitions()[0], profile);

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should lower: {err}"));
        artifact
            .validate()
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should validate: {err}"));
        let worker_artifact = artifact_process(&artifact, "Worker");
        assert_eq!(
            worker_artifact.transitions.len(),
            1,
            "terminal case {terminal:?} should emit one typed artifact transition"
        );
        assert_artifact_terminal(worker_artifact, &worker_artifact.transitions[0], profile);
    }
}

#[test]
fn bounded_assurance_invalid_source_models_fail_closed() {
    for case in INVALID_SOURCE_CASES {
        let source = invalid_source_case(case);
        let err = match check_source(&source) {
            Ok(_) => panic!("invalid bounded source case {} should fail", case.name),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(
            message.contains(case.expected),
            "invalid bounded source case {} failed for unexpected reason: {message}",
            case.name
        );
    }
}

#[test]
fn bounded_assurance_artifact_bypass_mutations_fail_admission() {
    let source = valid_source_for_sequence(
        "process_return_match_arm_assurance_artifact_bypass",
        &[
            ModelStatement::IfWithForNestedIf,
            ModelStatement::ForEach,
            ModelStatement::ForWithIf,
        ],
        ACTION_BLOCK_TERMINAL_PROFILE,
    );
    let checked = check_source(&source).expect("artifact-bypass seed source should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("artifact-bypass seed source should lower");
    artifact
        .validate()
        .expect("artifact-bypass seed artifact should validate before mutation");

    assert_artifact_mutation_rejected(
        artifact.clone(),
        insert_nested_for_each_into_first_worker_loop,
        "nested for loops are not supported",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        deepen_first_worker_runtime_if,
        "runtime if action nesting exceeds maximum depth",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        insert_spawn_inside_first_worker_runtime_if,
        "runtime if branch cannot bind process references",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        empty_first_worker_runtime_if_branches,
        "runtime if action branches cannot both be empty",
    );
    assert_artifact_mutation_rejected(
        artifact,
        remove_send_effect_from_worker_transition,
        "uses effect send but does not declare it",
    );
}

fn visit_model_sequences(
    remaining: usize,
    sequence: &mut Vec<ModelStatement>,
    visit: &mut impl FnMut(&[ModelStatement]),
) {
    if remaining == 0 {
        visit(sequence);
        return;
    }
    for statement in MODEL_STATEMENTS {
        sequence.push(statement);
        visit_model_sequences(remaining - 1, sequence, visit);
        sequence.pop();
    }
}

fn expected_model_case_count() -> usize {
    let mut total = 0usize;
    let mut cases_for_len = 1usize;
    for _ in 0..=MAX_MODEL_SEQUENCE_LEN {
        total = total
            .checked_add(cases_for_len)
            .expect("bounded model total should not overflow");
        cases_for_len = cases_for_len
            .checked_mul(MODEL_STATEMENTS.len())
            .expect("bounded model multiplier should not overflow");
    }
    total
}

fn assert_valid_model_sequence(
    case_index: usize,
    sequence: &[ModelStatement],
    terminal_profile: TerminalProfile,
) {
    let module_name = format!("process_return_match_arm_assurance_{case_index}");
    let source = valid_source_for_sequence(&module_name, sequence, terminal_profile);
    let expected = expected_action_shapes(sequence);

    let checked = check_source(&source)
        .unwrap_or_else(|err| panic!("bounded model sequence {sequence:?} should check: {err}"));
    let worker = checked_process(&checked, "Worker");
    assert_eq!(
        worker.transitions().len(),
        2,
        "bounded model sequence {sequence:?} should source-select one transition per concrete payload"
    );
    for transition in worker.transitions() {
        assert_eq!(
            checked_action_shapes(transition.actions()),
            expected,
            "bounded model sequence {sequence:?} should lower to exact checked action shape"
        );
        assert_checked_terminal(worker, transition, terminal_profile);
    }

    let artifact = lower_to_artifact(&checked, &source)
        .unwrap_or_else(|err| panic!("bounded model sequence {sequence:?} should lower: {err}"));
    artifact.validate().unwrap_or_else(|err| {
        panic!("bounded model sequence {sequence:?} artifact should validate: {err}")
    });
    let worker_artifact = artifact_process(&artifact, "Worker");
    assert_eq!(
        worker_artifact.transitions.len(),
        2,
        "bounded model sequence {sequence:?} should emit one typed artifact transition per payload"
    );
    for transition in &worker_artifact.transitions {
        assert_eq!(
            artifact_action_shapes(&transition.actions),
            expected,
            "bounded model sequence {sequence:?} should lower to exact artifact action shape"
        );
        assert_artifact_terminal(worker_artifact, transition, terminal_profile);
        assert_nested_artifact_send_actions_use_ids(
            &artifact,
            worker_artifact,
            &transition.actions,
        );
    }
    assert_no_source_binding_dispatch(&artifact);
    if case_index == 0 {
        assert_no_encoded_source_binding_leak(&artifact);
    }
}

fn expected_action_shapes(sequence: &[ModelStatement]) -> Vec<ActionShape> {
    let mut shapes = Vec::with_capacity(sequence.len() + 1);
    shapes.push(ActionShape::Spawn);
    shapes.extend(sequence.iter().map(|statement| statement.shape()));
    shapes
}

fn valid_source_for_sequence(
    module_name: &str,
    sequence: &[ModelStatement],
    terminal_profile: TerminalProfile,
) -> String {
    let statements = sequence
        .iter()
        .enumerate()
        .map(|(index, statement)| statement.source(index))
        .collect::<String>();
    let effects = effects_source(effects_for_sequence(sequence));
    source_with_arm_bodies(
        module_name,
        &effects,
        &statements,
        &statements,
        terminal_profile.ready.source(),
        terminal_profile.done.source(),
        terminal_profile.send_done_message(),
    )
}

fn invalid_source_case(case: InvalidSourceCase) -> String {
    source_with_arm_bodies(
        &format!("process_return_match_arm_assurance_invalid_{}", case.name),
        case.effects,
        case.ready_statements,
        case.done_statements,
        case.ready_return,
        case.done_return,
        case.send_done_message,
    )
}

fn source_with_arm_bodies(
    module_name: &str,
    effects: &str,
    ready_statements: &str,
    done_statements: &str,
    ready_return: &str,
    done_return: &str,
    send_done_message: bool,
) -> String {
    let done_send = if send_done_message {
        "        send worker Envelope(Assign(Assignment {\n            phase: Done,\n            enabled: False,\n            jobs: List<Job,1>[Job { phase: Done, urgent: False }],\n        }));\n"
    } else {
        ""
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
{done_send}        return Stop(state);
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
{ready_statements}                return {ready_return};
            }}
            Done => {{
{done_statements}                return {done_return};
            }}
        }};
    }}
}}

proc Sink mailbox bounded(64) {{
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

fn effects_for_sequence(sequence: &[ModelStatement]) -> Vec<Effect> {
    let mut effects = Vec::with_capacity(3);
    if sequence.iter().any(|statement| statement.uses_emit()) {
        effects.push(Effect::Emit);
    }
    effects.push(Effect::Spawn);
    if sequence.iter().any(|statement| statement.uses_send()) {
        effects.push(Effect::Send);
    }
    effects
}

fn effects_source(effects: Vec<Effect>) -> String {
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

impl ModelStatement {
    fn source(self, index: usize) -> String {
        match self {
            Self::Emit => format!("                emit \"assurance emit {index}\";\n"),
            Self::Send => "                send sink Ack;\n".to_string(),
            Self::IfElse => format!(
                "                if (selected_enabled == True) {{\n                    emit \"assurance if {index} then\";\n                    send sink Ack;\n                }} else {{\n                    emit \"assurance if {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForEach => format!(
                "                for Job {{ phase: assurance_item_phase, urgent: assurance_item_urgent }} in selected_jobs {{\n                    emit \"assurance for {index} item\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::IfWithFor => format!(
                "                if (selected_enabled == True) {{\n                    emit \"assurance if-for {index} branch\";\n                    for Job {{ phase: assurance_item_phase, urgent: assurance_item_urgent }} in selected_jobs {{\n                        emit \"assurance if-for {index} item\";\n                        send sink Ack;\n                    }}\n                }} else {{\n                    emit \"assurance if-for {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForWithIf => format!(
                "                for Job {{ phase: assurance_item_phase, urgent: assurance_item_urgent }} in selected_jobs {{\n                    if (assurance_item_urgent == True) {{\n                        emit \"assurance for-if {index} then\";\n                        send sink Ack;\n                    }} else {{\n                        emit \"assurance for-if {index} else\";\n                        send sink Ack;\n                    }}\n                }}\n"
            ),
            Self::IfWithForNestedIf => format!(
                "                if (selected_enabled == True) {{\n                    emit \"assurance nested if-for {index} branch\";\n                    for Job {{ phase: assurance_item_phase, urgent: assurance_item_urgent }} in selected_jobs {{\n                        if (assurance_item_phase == Ready) {{\n                            if (selected_enabled == True) {{\n                                emit \"assurance nested if-for {index} item\";\n                                send sink Ack;\n                            }} else {{\n                                emit \"assurance nested if-for {index} inner fallback\";\n                                send sink Ack;\n                            }}\n                        }} else {{\n                            emit \"assurance nested if-for {index} outer fallback\";\n                            send sink Ack;\n                        }}\n                    }}\n                }} else {{\n                    emit \"assurance nested if-for {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
        }
    }

    fn shape(self) -> ActionShape {
        match self {
            Self::Emit => ActionShape::Emit,
            Self::Send => ActionShape::Send,
            Self::IfElse => ActionShape::IfElse {
                then_actions: vec![ActionShape::Emit, ActionShape::Send],
                else_actions: vec![ActionShape::Emit, ActionShape::Send],
            },
            Self::ForEach => ActionShape::ForEach {
                body: vec![ActionShape::Emit, ActionShape::Send],
            },
            Self::IfWithFor => ActionShape::IfElse {
                then_actions: vec![
                    ActionShape::Emit,
                    ActionShape::ForEach {
                        body: vec![ActionShape::Emit, ActionShape::Send],
                    },
                ],
                else_actions: vec![ActionShape::Emit, ActionShape::Send],
            },
            Self::ForWithIf => ActionShape::ForEach {
                body: vec![ActionShape::IfElse {
                    then_actions: vec![ActionShape::Emit, ActionShape::Send],
                    else_actions: vec![ActionShape::Emit, ActionShape::Send],
                }],
            },
            Self::IfWithForNestedIf => ActionShape::IfElse {
                then_actions: vec![
                    ActionShape::Emit,
                    ActionShape::ForEach {
                        body: vec![ActionShape::IfElse {
                            then_actions: vec![ActionShape::IfElse {
                                then_actions: vec![ActionShape::Emit, ActionShape::Send],
                                else_actions: vec![ActionShape::Emit, ActionShape::Send],
                            }],
                            else_actions: vec![ActionShape::Emit, ActionShape::Send],
                        }],
                    },
                ],
                else_actions: vec![ActionShape::Emit, ActionShape::Send],
            },
        }
    }

    const fn uses_emit(self) -> bool {
        !matches!(self, Self::Send)
    }

    const fn uses_send(self) -> bool {
        !matches!(self, Self::Emit)
    }
}

fn checked_process<'a>(checked: &'a CheckedProgram, name: &str) -> &'a CheckedProcess {
    checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == name)
        .unwrap_or_else(|| panic!("checked process {name} should exist"))
}

fn artifact_process<'a>(artifact: &'a MantleArtifact, name: &str) -> &'a ArtifactProcess {
    artifact
        .processes
        .iter()
        .find(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"))
}

fn artifact_process_id(artifact: &MantleArtifact, name: &str) -> ProcessId {
    let index = artifact
        .processes
        .iter()
        .position(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"));
    ProcessId::from_index(index).expect("artifact process index should fit")
}

fn artifact_process_mut<'a>(
    artifact: &'a mut MantleArtifact,
    name: &str,
) -> &'a mut ArtifactProcess {
    artifact
        .processes
        .iter_mut()
        .find(|process| process.debug_name == name)
        .unwrap_or_else(|| panic!("artifact process {name} should exist"))
}

fn checked_state_id_by_label(process: &CheckedProcess, label: &str) -> CheckedStateId {
    let index = process
        .state_values()
        .iter()
        .position(|state| state.label() == label)
        .unwrap_or_else(|| panic!("checked state {label} should exist"));
    CheckedStateId::from_index(index).expect("checked state index should fit")
}

fn artifact_state_id_by_label(process: &ArtifactProcess, label: &str) -> StateId {
    let index = process
        .state_values
        .iter()
        .position(|state| state.label == label)
        .unwrap_or_else(|| panic!("artifact state {label} should exist"));
    StateId::from_index(index).expect("artifact state index should fit")
}

fn artifact_message_id_by_label(process: &ArtifactProcess, label: &str) -> MessageId {
    let index = process
        .message_variants
        .iter()
        .position(|message| message.label == label)
        .unwrap_or_else(|| panic!("artifact message {label} should exist"));
    MessageId::from_index(index).expect("artifact message index should fit")
}

fn checked_selected_arm(transition: &CheckedTransition) -> SelectedArm {
    selected_arm_from_label(
        transition
            .payload_guard()
            .expect("selected return-match transition should have a payload guard")
            .label(),
    )
}

fn artifact_selected_arm(transition: &ArtifactTransition) -> SelectedArm {
    selected_arm_from_label(
        &transition
            .payload_guard
            .as_ref()
            .expect("selected artifact transition should have a payload guard")
            .label(),
    )
}

fn selected_arm_from_label(label: &str) -> SelectedArm {
    if label.contains("Ready") {
        SelectedArm::Ready
    } else if label.contains("Done") {
        SelectedArm::Done
    } else {
        panic!("payload guard label should identify selected arm: {label}");
    }
}

fn assert_checked_terminal(
    process: &CheckedProcess,
    transition: &CheckedTransition,
    terminal_profile: TerminalProfile,
) {
    let terminal = terminal_profile.for_arm(checked_selected_arm(transition));
    assert_eq!(transition.step_result(), terminal.checked_step_result());
    assert_eq!(
        transition.next_state(),
        CheckedNextState::Value(checked_state_id_by_label(process, terminal.state_label()))
    );
}

fn assert_artifact_terminal(
    process: &ArtifactProcess,
    transition: &ArtifactTransition,
    terminal_profile: TerminalProfile,
) {
    let terminal = terminal_profile.for_arm(artifact_selected_arm(transition));
    assert_eq!(transition.step_result, terminal.artifact_step_result());
    assert_eq!(
        &transition.next_state,
        &NextState::Value(artifact_state_id_by_label(process, terminal.state_label()))
    );
}

fn checked_action_shapes(actions: &[CheckedAction]) -> Vec<ActionShape> {
    actions
        .iter()
        .map(|action| match action {
            CheckedAction::Emit { .. } => ActionShape::Emit,
            CheckedAction::Spawn { .. } => ActionShape::Spawn,
            CheckedAction::Send { .. } => ActionShape::Send,
            CheckedAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => ActionShape::IfElse {
                then_actions: checked_action_shapes(then_actions),
                else_actions: checked_action_shapes(else_actions),
            },
            CheckedAction::ForEach { body, .. } => ActionShape::ForEach {
                body: checked_action_shapes(body),
            },
        })
        .collect()
}

fn artifact_action_shapes(actions: &[ArtifactAction]) -> Vec<ActionShape> {
    actions
        .iter()
        .map(|action| match action {
            ArtifactAction::Emit { .. } => ActionShape::Emit,
            ArtifactAction::Spawn { .. } => ActionShape::Spawn,
            ArtifactAction::Send { .. } => ActionShape::Send,
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => ActionShape::IfElse {
                then_actions: artifact_action_shapes(then_actions),
                else_actions: artifact_action_shapes(else_actions),
            },
            ArtifactAction::ForEach { body, .. } => ActionShape::ForEach {
                body: artifact_action_shapes(body),
            },
        })
        .collect()
}

fn assert_nested_artifact_send_actions_use_ids(
    artifact: &MantleArtifact,
    owner_process: &ArtifactProcess,
    actions: &[ArtifactAction],
) {
    let sink_process = artifact_process(artifact, "Sink");
    let sink_id = artifact_process_id(artifact, "Sink");
    let ack_id = artifact_message_id_by_label(sink_process, "Ack");

    for action in actions {
        match action {
            ArtifactAction::Send {
                target, message, ..
            } => {
                let ArtifactSendTarget::ProcessRef(process_ref) = target else {
                    panic!("selected-arm send should lower through a typed process-ref id");
                };
                let resolved = owner_process
                    .process_refs
                    .get(process_ref.index())
                    .unwrap_or_else(|| panic!("process ref id {process_ref:?} should resolve"));
                assert_eq!(
                    resolved.target, sink_id,
                    "selected-arm send should resolve typed process-ref id to Sink"
                );
                assert_eq!(
                    *message, ack_id,
                    "selected-arm send should use typed Ack id"
                );
            }
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, then_actions);
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, else_actions);
            }
            ArtifactAction::ForEach { body, .. } => {
                assert_nested_artifact_send_actions_use_ids(artifact, owner_process, body);
            }
            ArtifactAction::Emit { .. } | ArtifactAction::Spawn { .. } => {}
        }
    }
}

fn assert_no_source_binding_dispatch(artifact: &MantleArtifact) {
    for process in &artifact.processes {
        for transition in &process.transitions {
            if let Some(payload) = &transition.payload_guard {
                assert_artifact_payload_has_no_source_bindings(payload);
            }
            assert_actions_have_no_source_bindings(&transition.actions);
            assert_next_state_has_no_source_bindings(&transition.next_state);
        }
    }
}

fn assert_no_encoded_source_binding_leak(artifact: &MantleArtifact) {
    let encoded = artifact.encode();
    for name in SOURCE_ONLY_BINDINGS {
        assert!(
            !encoded.lines().any(|line| line.contains(name)),
            "artifact must not lower source binding name {name} as executable dispatch"
        );
    }
}

fn assert_actions_have_no_source_bindings(actions: &[ArtifactAction]) {
    for action in actions {
        match action {
            ArtifactAction::Emit { .. } | ArtifactAction::Spawn { .. } => {}
            ArtifactAction::Send { payload, .. } => {
                if let Some(payload) = payload {
                    assert_template_has_no_source_bindings(payload);
                }
            }
            ArtifactAction::IfElse {
                condition,
                then_actions,
                else_actions,
            } => {
                assert_template_has_no_source_bindings(condition);
                assert_actions_have_no_source_bindings(then_actions);
                assert_actions_have_no_source_bindings(else_actions);
            }
            ArtifactAction::ForEach {
                collection, body, ..
            } => {
                assert_template_has_no_source_bindings(collection);
                assert_actions_have_no_source_bindings(body);
            }
        }
    }
}

fn assert_next_state_has_no_source_bindings(next_state: &NextState) {
    match next_state {
        NextState::Current | NextState::Value(_) => {}
        NextState::Template(template) => assert_template_has_no_source_bindings(template),
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            assert_template_has_no_source_bindings(condition);
            assert_next_state_has_no_source_bindings(then_state);
            assert_next_state_has_no_source_bindings(else_state);
        }
    }
}

fn assert_template_has_no_source_bindings(template: &ArtifactValueTemplate) {
    match template {
        ArtifactValueTemplate::Literal { value, .. } => {
            assert_value_has_no_source_bindings(value);
        }
        ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::LoopElement { .. } => {}
        ArtifactValueTemplate::EnumPayload { value, .. }
        | ArtifactValueTemplate::ListElement { list: value, .. }
        | ArtifactValueTemplate::ListPrefixElement { list: value, .. }
        | ArtifactValueTemplate::ListRest { list: value, .. }
        | ArtifactValueTemplate::MapRest { map: value, .. }
        | ArtifactValueTemplate::BooleanNot { operand: value, .. }
        | ArtifactValueTemplate::EnumVariant { payload: value, .. } => {
            assert_template_has_no_source_bindings(value);
        }
        ArtifactValueTemplate::RecordField { record, field, .. } => {
            assert_no_source_binding_string(field);
            assert_template_has_no_source_bindings(record);
        }
        ArtifactValueTemplate::MapValue { map, key, keys, .. } => {
            assert_template_has_no_source_bindings(map);
            assert_value_has_no_source_bindings(key);
            for key in keys {
                assert_value_has_no_source_bindings(key);
            }
        }
        ArtifactValueTemplate::Record { fields, .. } => {
            for field in fields {
                assert_no_source_binding_string(&field.name);
                assert_template_has_no_source_bindings(&field.value);
            }
        }
        ArtifactValueTemplate::List { items, .. } => {
            for item in items {
                assert_template_has_no_source_bindings(item);
            }
        }
        ArtifactValueTemplate::Map { entries, .. } => {
            for entry in entries {
                assert_template_has_no_source_bindings(&entry.key);
                assert_template_has_no_source_bindings(&entry.value);
            }
        }
        ArtifactValueTemplate::Equality { left, right, .. }
        | ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
            assert_template_has_no_source_bindings(left);
            assert_template_has_no_source_bindings(right);
        }
    }
}

fn assert_artifact_payload_has_no_source_bindings(payload: &ArtifactPayload) {
    assert_value_has_no_source_bindings(&payload.value);
}

fn assert_value_has_no_source_bindings(value: &ArtifactValue) {
    match value {
        ArtifactValue::Atom(value) => assert_no_source_binding_string(value),
        ArtifactValue::EnumVariant { variant, payload } => {
            assert_no_source_binding_string(variant);
            assert_value_has_no_source_bindings(payload);
        }
        ArtifactValue::Record {
            constructor,
            fields,
        } => {
            assert_no_source_binding_string(constructor);
            for field in fields {
                assert_no_source_binding_string(&field.name);
                assert_value_has_no_source_bindings(&field.value);
            }
        }
        ArtifactValue::List(items) => {
            for item in items {
                assert_value_has_no_source_bindings(item);
            }
        }
        ArtifactValue::Map(entries) => {
            for entry in entries {
                assert_value_has_no_source_bindings(&entry.key);
                assert_value_has_no_source_bindings(&entry.value);
            }
        }
        ArtifactValue::ProcessRef { .. } => {}
    }
}

fn assert_no_source_binding_string(value: &str) {
    for name in SOURCE_ONLY_BINDINGS {
        assert!(
            !value.contains(name),
            "artifact must not lower source binding name {name} as executable dispatch"
        );
    }
}

fn assert_artifact_mutation_rejected(
    mut artifact: MantleArtifact,
    mutate: fn(&mut MantleArtifact),
    expected: &str,
) {
    mutate(&mut artifact);
    let err = match artifact.validate() {
        Ok(_) => panic!("mutated artifact should reject with {expected}"),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(
        message.contains(expected),
        "mutated artifact rejected with unexpected diagnostic: {message}"
    );
}

fn first_worker_transition_mut(artifact: &mut MantleArtifact) -> &mut ArtifactTransition {
    artifact_process_mut(artifact, "Worker")
        .transitions
        .first_mut()
        .expect("Worker artifact process should have transitions")
}

fn insert_nested_for_each_into_first_worker_loop(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    let nested = transition
        .actions
        .iter()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("seed artifact should contain a top-level for_each action")
        .clone();
    let ArtifactAction::ForEach { body, .. } = transition
        .actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("seed artifact should contain a mutable top-level for_each action")
    else {
        unreachable!("find predicate already matched for_each");
    };
    body.push(nested);
}

fn deepen_first_worker_runtime_if(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    assert!(
        deepen_first_nested_if(&mut transition.actions),
        "seed artifact should contain nested runtime if actions"
    );
}

fn insert_spawn_inside_first_worker_runtime_if(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    let spawn = transition
        .actions
        .iter()
        .find(|action| matches!(action, ArtifactAction::Spawn { .. }))
        .expect("seed artifact should contain a spawn action")
        .clone();
    assert!(
        push_into_first_if_branch(&mut transition.actions, spawn),
        "seed artifact should contain a runtime if action"
    );
}

fn empty_first_worker_runtime_if_branches(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    assert!(
        empty_first_if(&mut transition.actions),
        "seed artifact should contain a runtime if action"
    );
}

fn remove_send_effect_from_worker_transition(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    transition
        .effects
        .retain(|effect| *effect != ArtifactEffect::Send);
}

fn deepen_first_nested_if(actions: &mut [ArtifactAction]) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                if insert_extra_if_inside_first_if(then_actions) {
                    return true;
                }
                if insert_extra_if_inside_first_if(else_actions) {
                    return true;
                }
                if deepen_first_nested_if(then_actions) {
                    return true;
                }
                if deepen_first_nested_if(else_actions) {
                    return true;
                }
            }
            ArtifactAction::ForEach { body, .. } => {
                if deepen_first_nested_if(body) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}

fn insert_extra_if_inside_first_if(actions: &mut [ArtifactAction]) -> bool {
    let Some(action) = actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::IfElse { .. }))
    else {
        return false;
    };
    let ArtifactAction::IfElse {
        condition,
        then_actions,
        ..
    } = action
    else {
        unreachable!("find predicate already matched if_else");
    };
    let nested_then = std::mem::take(then_actions);
    then_actions.push(ArtifactAction::IfElse {
        condition: condition.clone(),
        then_actions: nested_then,
        else_actions: Vec::new(),
    });
    true
}

fn push_into_first_if_branch(actions: &mut [ArtifactAction], inserted: ArtifactAction) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse { then_actions, .. } => {
                then_actions.push(inserted);
                return true;
            }
            ArtifactAction::ForEach { body, .. } => {
                if push_into_first_if_branch(body, inserted.clone()) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}

fn empty_first_if(actions: &mut [ArtifactAction]) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                then_actions.clear();
                else_actions.clear();
                return true;
            }
            ArtifactAction::ForEach { body, .. } => {
                if empty_first_if(body) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}
