pub const RUNTIME_TRACE_SCHEMA_ID: &str = "mantle-runtime-observability";
pub const RUNTIME_TRACE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeTraceEventKind {
    ArtifactLoaded,
    ProcessSpawned,
    MessageAccepted,
    MessageDequeued,
    ProgramOutput,
    SpawnAuthorityChecked,
    BoundarySendChecked,
    EffectOutcomeBound,
    BranchSelected,
    LoopStarted,
    LoopIteration,
    LoopCompleted,
    StateUpdated,
    ProcessStepped,
    ProcessStopped,
    ProcessFailed,
    SupervisorChildStarted,
    SupervisorRestartDecision,
}

impl RuntimeTraceEventKind {
    pub const ALL: &[Self] = &[
        Self::ArtifactLoaded,
        Self::ProcessSpawned,
        Self::MessageAccepted,
        Self::MessageDequeued,
        Self::ProgramOutput,
        Self::SpawnAuthorityChecked,
        Self::BoundarySendChecked,
        Self::EffectOutcomeBound,
        Self::BranchSelected,
        Self::LoopStarted,
        Self::LoopIteration,
        Self::LoopCompleted,
        Self::StateUpdated,
        Self::ProcessStepped,
        Self::ProcessStopped,
        Self::ProcessFailed,
        Self::SupervisorChildStarted,
        Self::SupervisorRestartDecision,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactLoaded => "artifact_loaded",
            Self::ProcessSpawned => "process_spawned",
            Self::MessageAccepted => "message_accepted",
            Self::MessageDequeued => "message_dequeued",
            Self::ProgramOutput => "program_output",
            Self::SpawnAuthorityChecked => "spawn_authority_checked",
            Self::BoundarySendChecked => "boundary_send_checked",
            Self::EffectOutcomeBound => "effect_outcome_bound",
            Self::BranchSelected => "branch_selected",
            Self::LoopStarted => "loop_started",
            Self::LoopIteration => "loop_iteration",
            Self::LoopCompleted => "loop_completed",
            Self::StateUpdated => "state_updated",
            Self::ProcessStepped => "process_stepped",
            Self::ProcessStopped => "process_stopped",
            Self::ProcessFailed => "process_failed",
            Self::SupervisorChildStarted => "supervisor_child_started",
            Self::SupervisorRestartDecision => "supervisor_restart_decision",
        }
    }

    pub fn from_event_name(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }

    pub const fn contract(self) -> RuntimeTraceEventContract {
        match self {
            Self::ArtifactLoaded => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "format",
                    "schema_version",
                    "source_language",
                    "module",
                    "entry_process_id",
                    "entry_process",
                    "entry_message_id",
                    "process_count",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["entry_process_id", "entry_message_id"],
                &[],
                &[
                    "format",
                    "schema_version",
                    "source_language",
                    "module",
                    "entry_process",
                ],
            ),
            Self::ProcessSpawned => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "state_id",
                    "state",
                    "mailbox_bound",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "state_id"],
                &["spawned_by_pid"],
                &["process", "state"],
            ),
            Self::MessageAccepted => mailbox_contract(&[
                "payload_type_id",
                "payload_process_id",
                "payload_pid",
                "sender_pid",
            ]),
            Self::MessageDequeued => {
                mailbox_contract(&["payload_type_id", "payload_process_id", "payload_pid"])
            }
            Self::ProgramOutput => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "stream",
                    "output_id",
                    "text",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "output_id"],
                &[],
                &["process", "stream", "text"],
            ),
            Self::SpawnAuthorityChecked => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "target_process_id",
                    "spawn_site_id",
                    "authority_id",
                    "authority_policy_decision_id",
                    "spawn_kind",
                    "authority_result",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "pid",
                    "process_id",
                    "target_process_id",
                    "spawn_site_id",
                    "authority_id",
                ],
                &["authority_policy_decision_id"],
                &["process", "spawn_kind", "authority_result"],
            ),
            Self::BoundarySendChecked => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "port_id",
                    "port",
                    "protocol_id",
                    "protocol",
                    "authority_id",
                    "authority_policy_decision_id",
                    "target_process_id",
                    "target_process",
                    "message_id",
                    "message",
                    "boundary_result",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "pid",
                    "process_id",
                    "port_id",
                    "protocol_id",
                    "authority_id",
                    "target_process_id",
                    "message_id",
                ],
                &["authority_policy_decision_id"],
                &[
                    "process",
                    "port",
                    "protocol",
                    "target_process",
                    "message",
                    "boundary_result",
                ],
            ),
            Self::EffectOutcomeBound => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "outcome_id",
                    "action",
                    "target_process_id",
                    "outcome_result",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "outcome_id", "target_process_id"],
                &["spawn_site_id", "message_id", "port_id"],
                &["process", "action", "outcome_result"],
            ),
            Self::BranchSelected => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "message_id",
                    "message",
                    "branch",
                    "scope",
                    "branch_path",
                    "condition_type_id",
                    "condition",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "message_id", "condition_type_id"],
                &["loop_element_id"],
                &["process", "message", "branch", "scope", "condition"],
            ),
            Self::LoopStarted => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "message_id",
                    "message",
                    "element_id",
                    "collection_type_id",
                    "max_items",
                    "item_count",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "pid",
                    "process_id",
                    "message_id",
                    "element_id",
                    "collection_type_id",
                ],
                &[],
                &["process", "message"],
            ),
            Self::LoopIteration => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "message_id",
                    "message",
                    "element_id",
                    "index",
                    "element_type_id",
                    "element",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "pid",
                    "process_id",
                    "message_id",
                    "element_id",
                    "element_type_id",
                ],
                &[],
                &["process", "message", "element"],
            ),
            Self::LoopCompleted => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "message_id",
                    "message",
                    "element_id",
                    "iteration_count",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "message_id", "element_id"],
                &[],
                &["process", "message"],
            ),
            Self::StateUpdated => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "from_state_id",
                    "from",
                    "to_state_id",
                    "to",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "from_state_id", "to_state_id"],
                &[],
                &["process", "from", "to"],
            ),
            Self::ProcessStepped => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "message_id",
                    "message",
                    "result",
                    "state_id",
                    "state",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "message_id", "state_id"],
                &["payload_type_id", "payload_process_id", "payload_pid"],
                &["process", "message", "result", "state"],
            ),
            Self::ProcessStopped => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "reason",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id"],
                &[],
                &["process", "reason"],
            ),
            Self::ProcessFailed => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "pid",
                    "process_id",
                    "process",
                    "state_id",
                    "state",
                    "reason",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &["pid", "process_id", "state_id"],
                &[],
                &["process", "state", "reason"],
            ),
            Self::SupervisorChildStarted => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "supervisor_pid",
                    "supervisor_process_id",
                    "supervisor_process",
                    "supervisor_id",
                    "child_id",
                    "child",
                    "child_pid",
                    "child_process_id",
                    "child_process",
                    "spawn_site_id",
                    "spawn_kind",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "supervisor_pid",
                    "supervisor_process_id",
                    "supervisor_id",
                    "child_id",
                    "child_pid",
                    "child_process_id",
                    "spawn_site_id",
                ],
                &[],
                &["supervisor_process", "child", "child_process", "spawn_kind"],
            ),
            Self::SupervisorRestartDecision => RuntimeTraceEventContract::new(
                &[
                    "event",
                    "supervisor_pid",
                    "supervisor_process_id",
                    "supervisor_process",
                    "supervisor_id",
                    "child_id",
                    "child",
                    "child_pid",
                    "child_process_id",
                    "child_process",
                    "reason",
                    "decision",
                    "restart_time_ms",
                    "restart_window_count",
                    "restart_window_limit",
                    "restart_window_ms",
                    "new_child_pid",
                    "trace_schema",
                    "trace_schema_version",
                ],
                &[
                    "supervisor_pid",
                    "supervisor_process_id",
                    "supervisor_id",
                    "child_id",
                    "child_pid",
                    "child_process_id",
                ],
                &["new_child_pid"],
                &[
                    "supervisor_process",
                    "child",
                    "child_process",
                    "reason",
                    "decision",
                ],
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTraceEventContract {
    required_fields: &'static [&'static str],
    typed_id_fields: &'static [&'static str],
    optional_typed_id_fields: &'static [&'static str],
    metadata_fields: &'static [&'static str],
}

impl RuntimeTraceEventContract {
    const fn new(
        required_fields: &'static [&'static str],
        typed_id_fields: &'static [&'static str],
        optional_typed_id_fields: &'static [&'static str],
        metadata_fields: &'static [&'static str],
    ) -> Self {
        Self {
            required_fields,
            typed_id_fields,
            optional_typed_id_fields,
            metadata_fields,
        }
    }

    pub const fn required_fields(self) -> &'static [&'static str] {
        self.required_fields
    }

    pub const fn typed_id_fields(self) -> &'static [&'static str] {
        self.typed_id_fields
    }

    pub const fn optional_typed_id_fields(self) -> &'static [&'static str] {
        self.optional_typed_id_fields
    }

    pub const fn metadata_fields(self) -> &'static [&'static str] {
        self.metadata_fields
    }
}

const fn mailbox_contract(
    optional_typed_id_fields: &'static [&'static str],
) -> RuntimeTraceEventContract {
    RuntimeTraceEventContract::new(
        &[
            "event",
            "pid",
            "process_id",
            "process",
            "message_id",
            "message",
            "queue_depth",
            "trace_schema",
            "trace_schema_version",
        ],
        &["pid", "process_id", "message_id"],
        optional_typed_id_fields,
        &["process", "message"],
    )
}
