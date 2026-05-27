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
        expected: "nested for loops are not supported",
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
        expected: "statement-level if branches cannot bind local values or process references",
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

    authority spawn_worker: Cap<Spawn<Worker>>;

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

    authority spawn_sink: Cap<Spawn<Sink>>;

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
