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
        expected: "nested for loops are not supported",
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
        expected: "statement-level if branches cannot bind local values or process references",
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

