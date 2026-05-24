use crate::support::*;

const MAX_RUNTIME_SEQUENCE_LEN: usize = 2;

const RUNTIME_STATEMENTS: [RuntimeStatement; 7] = [
    RuntimeStatement::Emit,
    RuntimeStatement::Send,
    RuntimeStatement::IfElse,
    RuntimeStatement::ForEach,
    RuntimeStatement::IfWithFor,
    RuntimeStatement::ForWithIf,
    RuntimeStatement::IfWithForNestedIf,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeStatement {
    Emit,
    Send,
    IfElse,
    ForEach,
    IfWithFor,
    ForWithIf,
    IfWithForNestedIf,
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedOutput {
    text: String,
    count: usize,
}

#[test]
fn process_return_match_arm_bounded_runtime_models_execute_on_mantle() {
    let gate = GateHarness::new();
    let mut sequence = Vec::with_capacity(MAX_RUNTIME_SEQUENCE_LEN);
    let mut case_index = 0usize;

    for len in 0..=MAX_RUNTIME_SEQUENCE_LEN {
        visit_runtime_sequences(len, &mut sequence, &mut |sequence| {
            let stem = format!("process_return_match_arm_bounded_runtime_{case_index}");
            assert_runtime_sequence(&gate, &stem, sequence);
            case_index = case_index
                .checked_add(1)
                .expect("bounded runtime case count should not overflow");
        });
    }

    assert_eq!(case_index, expected_runtime_case_count());
}

fn visit_runtime_sequences(
    remaining: usize,
    sequence: &mut Vec<RuntimeStatement>,
    visit: &mut impl FnMut(&[RuntimeStatement]),
) {
    if remaining == 0 {
        visit(sequence);
        return;
    }
    for statement in RUNTIME_STATEMENTS {
        sequence.push(statement);
        visit_runtime_sequences(remaining - 1, sequence, visit);
        sequence.pop();
    }
}

fn expected_runtime_case_count() -> usize {
    let mut total = 0usize;
    let mut cases_for_len = 1usize;
    for _ in 0..=MAX_RUNTIME_SEQUENCE_LEN {
        total = total
            .checked_add(cases_for_len)
            .expect("bounded runtime total should not overflow");
        cases_for_len = cases_for_len
            .checked_mul(RUNTIME_STATEMENTS.len())
            .expect("bounded runtime multiplier should not overflow");
    }
    total
}

fn assert_runtime_sequence(gate: &GateHarness, stem: &str, sequence: &[RuntimeStatement]) {
    let source = runtime_source(stem, sequence);
    let source_path = gate.write_target_source(stem, &source);
    let source_path = source_path.to_string_lossy().into_owned();
    let artifact_path = format!("target/strata/{stem}.mta");

    gate.remove_trace(stem);
    let run = gate.check_build_run(&source_path, &artifact_path);
    let stdout = String::from_utf8_lossy(&run.stdout);

    assert!(stdout.contains("mantle: stopped Main normally"));
    assert!(stdout.contains("mantle: stopped Worker normally"));
    assert_eq!(
        stdout.matches("bounded runtime uniform prefix").count(),
        1,
        "uniform action prefix should execute exactly once for {sequence:?}"
    );
    assert!(
        !stdout.contains("bounded runtime unselected done"),
        "unselected return-match arm must not execute for {sequence:?}"
    );

    let expected_outputs = expected_worker_outputs(sequence);
    assert_worker_outputs_in_order(&stdout, &expected_outputs, sequence);
    assert_eq!(
        stdout.matches("bounded runtime sink received").count(),
        expected_send_count(sequence),
        "selected-arm sends should execute with typed process-ref authority for {sequence:?}"
    );
}

fn runtime_source(module_name: &str, sequence: &[RuntimeStatement]) -> String {
    let statements = sequence
        .iter()
        .enumerate()
        .map(|(index, statement)| statement.source(index))
        .collect::<String>();
    let done_send = if sequence.iter().any(|statement| statement.uses_send()) {
        "                send sink Ack;\n"
    } else {
        ""
    };
    let effects = effects_source(sequence);

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
    jobs: List<Job,2>,
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
            jobs: List<Job,2>[
                Job {{ phase: Ready, urgent: True }},
                Job {{ phase: Done, urgent: False }},
            ],
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
        emit "bounded runtime uniform prefix";
        let sink: ProcessRef<Sink> = spawn Sink;
        return match selected_phase {{
            Ready => {{
{statements}                return Stop(SawReady);
            }}
            Done => {{
                emit "bounded runtime unselected done";
{done_send}                return Stop(SawDone);
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

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [emit] ~ [] @det {{
        emit "bounded runtime sink received";
        return Continue(state);
    }}
}}
"#
    )
}

fn effects_source(sequence: &[RuntimeStatement]) -> &'static str {
    if sequence.iter().any(|statement| statement.uses_send()) {
        "[emit, spawn, send]"
    } else {
        "[emit, spawn]"
    }
}

fn expected_worker_outputs(sequence: &[RuntimeStatement]) -> Vec<ExpectedOutput> {
    let mut outputs = Vec::new();
    for (index, statement) in sequence.iter().enumerate() {
        statement.push_expected_outputs(index, &mut outputs);
    }
    outputs
}

fn expected_send_count(sequence: &[RuntimeStatement]) -> usize {
    sequence
        .iter()
        .map(|statement| statement.selected_send_count())
        .sum()
}

fn assert_worker_outputs_in_order(
    stdout: &str,
    expected: &[ExpectedOutput],
    sequence: &[RuntimeStatement],
) {
    let mut previous_index = stdout
        .find("bounded runtime uniform prefix")
        .expect("uniform prefix should be present in stdout");
    for output in expected {
        let indices = stdout.match_indices(&output.text).collect::<Vec<_>>();
        let Some((first_index, _)) = indices.first() else {
            panic!(
                "runtime output {:?} should be present for {sequence:?}\n{stdout}",
                output.text
            );
        };
        assert!(
            previous_index < *first_index,
            "runtime output {:?} should preserve generated action order for {sequence:?}",
            output.text
        );
        assert_eq!(
            indices.len(),
            output.count,
            "runtime output {:?} should execute expected count for {sequence:?}",
            output.text
        );
        previous_index = indices
            .last()
            .expect("non-empty indices should have a last entry")
            .0;
    }
}

impl RuntimeStatement {
    fn source(self, index: usize) -> String {
        match self {
            Self::Emit => format!("                emit \"bounded runtime emit {index}\";\n"),
            Self::Send => "                send sink Ack;\n".to_string(),
            Self::IfElse => format!(
                "                if (selected_enabled == True) {{\n                    emit \"bounded runtime if {index} then\";\n                    send sink Ack;\n                }} else {{\n                    emit \"bounded runtime if {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForEach => format!(
                "                for Job {{ phase: runtime_item_phase, urgent: runtime_item_urgent }} in selected_jobs {{\n                    emit \"bounded runtime for {index} item\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::IfWithFor => format!(
                "                if (selected_enabled == True) {{\n                    emit \"bounded runtime if-for {index} branch\";\n                    for Job {{ phase: runtime_item_phase, urgent: runtime_item_urgent }} in selected_jobs {{\n                        emit \"bounded runtime if-for {index} item\";\n                        send sink Ack;\n                    }}\n                }} else {{\n                    emit \"bounded runtime if-for {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
            Self::ForWithIf => format!(
                "                for Job {{ phase: runtime_item_phase, urgent: runtime_item_urgent }} in selected_jobs {{\n                    if (runtime_item_urgent == True) {{\n                        emit \"bounded runtime for-if {index} then\";\n                        send sink Ack;\n                    }} else {{\n                        emit \"bounded runtime for-if {index} else\";\n                        send sink Ack;\n                    }}\n                }}\n"
            ),
            Self::IfWithForNestedIf => format!(
                "                if (selected_enabled == True) {{\n                    emit \"bounded runtime nested if-for {index} branch\";\n                    for Job {{ phase: runtime_item_phase, urgent: runtime_item_urgent }} in selected_jobs {{\n                        if (runtime_item_phase == Ready) {{\n                            if (selected_enabled == True) {{\n                                emit \"bounded runtime nested if-for {index} item\";\n                                send sink Ack;\n                            }} else {{\n                                emit \"bounded runtime nested if-for {index} inner fallback\";\n                                send sink Ack;\n                            }}\n                        }} else {{\n                            emit \"bounded runtime nested if-for {index} outer fallback\";\n                            send sink Ack;\n                        }}\n                    }}\n                }} else {{\n                    emit \"bounded runtime nested if-for {index} else\";\n                    send sink Ack;\n                }}\n"
            ),
        }
    }

    const fn uses_send(self) -> bool {
        !matches!(self, Self::Emit)
    }

    const fn selected_send_count(self) -> usize {
        match self {
            Self::Emit => 0,
            Self::Send | Self::IfElse => 1,
            Self::ForEach | Self::IfWithFor | Self::ForWithIf | Self::IfWithForNestedIf => 2,
        }
    }

    fn push_expected_outputs(self, index: usize, outputs: &mut Vec<ExpectedOutput>) {
        match self {
            Self::Emit => outputs.push(ExpectedOutput::once(format!(
                "bounded runtime emit {index}"
            ))),
            Self::Send => {}
            Self::IfElse => {
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime if {index} then"
                )));
            }
            Self::ForEach => {
                outputs.push(ExpectedOutput::twice(format!(
                    "bounded runtime for {index} item"
                )));
            }
            Self::IfWithFor => {
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime if-for {index} branch"
                )));
                outputs.push(ExpectedOutput::twice(format!(
                    "bounded runtime if-for {index} item"
                )));
            }
            Self::ForWithIf => {
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime for-if {index} then"
                )));
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime for-if {index} else"
                )));
            }
            Self::IfWithForNestedIf => {
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime nested if-for {index} branch"
                )));
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime nested if-for {index} item"
                )));
                outputs.push(ExpectedOutput::once(format!(
                    "bounded runtime nested if-for {index} outer fallback"
                )));
            }
        }
    }
}

impl ExpectedOutput {
    fn new(text: String, count: usize) -> Self {
        Self { text, count }
    }

    fn once(text: String) -> Self {
        Self::new(text, 1)
    }

    fn twice(text: String) -> Self {
        Self::new(text, 2)
    }
}
