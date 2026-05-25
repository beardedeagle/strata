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
