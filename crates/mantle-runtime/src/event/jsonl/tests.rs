use mantle_artifact::{
    ARTIFACT_SCHEMA_VERSION, ArtifactBranch, AuthorityId, LoopElementId, MessageId, OutputId,
    PortId, ProcessId, ProtocolId, SpawnSiteId, StateId, SupervisorChildId, SupervisorId, TypeId,
};

use super::*;
use crate::event::{
    RuntimeAuthorityResult, RuntimeLoopContext, RuntimeSpawnKind, RuntimeSupervisorExitReason,
    RuntimeSupervisorRestartDecision,
};
use crate::{
    RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeFailureReason, RuntimeOutputStream,
    RuntimeProcessId, RuntimeStepResult, RuntimeStopReason, RuntimeTraceEventKind,
    validate_runtime_trace_jsonl,
};

#[test]
fn artifact_loaded_trace_includes_entry_ids() {
    let event = RuntimeEvent::ArtifactLoaded {
        format: "mantle-target-artifact".to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
        source_language: "test_frontend".to_string(),
        module: "actor_sequence".to_string(),
        entry_process_id: ProcessId::new(7),
        entry_process: "Main".to_string(),
        entry_message_id: MessageId::new(3),
        process_count: 9,
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#""event":"artifact_loaded""#));
    assert!(line.contains(r#""entry_process_id":7"#));
    assert!(line.contains(r#""entry_message_id":3"#));
    assert!(line.contains(r#""trace_schema":"mantle-runtime-observability""#));
    assert!(line.contains(r#""trace_schema_version":1"#));
    assert_eq!(event.trace_kind(), RuntimeTraceEventKind::ArtifactLoaded);
}

#[test]
fn encoded_json_line_len_counts_trace_schema_fields() {
    let event = RuntimeEvent::ProcessStopped {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(0),
        process: "Main".to_string(),
        reason: RuntimeStopReason::Normal,
    };
    let line = encode_json_line(&event);
    let counted_len = encoded_json_line_len(&event).expect("trace line length should count");

    assert_eq!(counted_len, line.len());
    assert!(
        line.ends_with(
            r#","trace_schema":"mantle-runtime-observability","trace_schema_version":1}"#
        )
    );
}

#[test]
fn program_output_trace_includes_output_id() {
    let event = RuntimeEvent::ProgramOutput {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(2),
        process: "Worker".to_string(),
        stream: RuntimeOutputStream::Stdout,
        output_id: OutputId::new(13),
        text: "worker handled Second".to_string(),
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#""event":"program_output""#));
    assert!(line.contains(r#""process_id":2"#));
    assert!(line.contains(r#""output_id":13"#));
}

#[test]
fn supervisor_child_started_trace_includes_spawn_kind() {
    let event = RuntimeEvent::SupervisorChildStarted {
        supervisor_pid: RuntimeProcessId::FIRST,
        supervisor_process_id: ProcessId::new(0),
        supervisor_process: "Main".to_string(),
        supervisor_id: SupervisorId::new(0),
        child_id: SupervisorChildId::new(0),
        child: "worker".to_string(),
        child_pid: RuntimeProcessId::from_u64(2).expect("test pid should be nonzero"),
        child_process_id: ProcessId::new(1),
        child_process: "Worker".to_string(),
        spawn_site_id: SpawnSiteId::new(0),
        spawn_kind: RuntimeSpawnKind::LexicalSupervisorChild,
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#""spawn_kind":"lexical_supervisor_child""#));
}

#[test]
fn process_stopped_trace_includes_supervisor_stop_reason() {
    let event = RuntimeEvent::ProcessStopped {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(1),
        process: "Worker".to_string(),
        reason: RuntimeStopReason::SupervisorFailure,
    };

    let line = encode_json_line(&event);

    assert!(line.contains(r#""event":"process_stopped""#));
    assert!(line.contains(r#""reason":"supervisor_failure""#));
}

#[test]
fn branch_selected_trace_includes_typed_scope() {
    let event = RuntimeEvent::BranchSelected {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(2),
        process: "Worker".to_string(),
        message_id: MessageId::new(0),
        message: "Branch".to_string(),
        branch: ArtifactBranch::Then,
        scope: RuntimeBranchScope::Action,
        branch_path: RuntimeBranchPath::root(),
        loop_context: None,
        condition_type_id: TypeId::new(1),
        condition: "True".to_string(),
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#""event":"branch_selected""#));
    assert!(line.contains(r#""branch":"then""#));
    assert!(line.contains(r#""scope":"action""#));
    assert!(line.contains(r#""branch_path":[]"#));
    assert!(line.contains(r#""condition_type_id":1"#));
}

#[test]
fn branch_selected_trace_includes_typed_loop_context() {
    let event = RuntimeEvent::BranchSelected {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(2),
        process: "Worker".to_string(),
        message_id: MessageId::new(0),
        message: "Branch".to_string(),
        branch: ArtifactBranch::Else,
        scope: RuntimeBranchScope::Action,
        branch_path: RuntimeBranchPath::root(),
        loop_context: Some(RuntimeLoopContext {
            element_id: LoopElementId::new(3),
            index: 5,
        }),
        condition_type_id: TypeId::new(1),
        condition: "False".to_string(),
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#""loop_element_id":3"#));
    assert!(line.contains(r#""loop_index":5"#));
}

#[test]
fn trace_output_escapes_all_control_characters() {
    let event = RuntimeEvent::ProgramOutput {
        pid: RuntimeProcessId::FIRST,
        process_id: ProcessId::new(0),
        process: "Main".to_string(),
        stream: RuntimeOutputStream::Stdout,
        output_id: OutputId::new(0),
        text:
            "quote\" slash\\ newline\n carriage\r tab\t backspace\u{08} formfeed\u{0c} unit\u{1f}"
                .to_string(),
    };
    let line = encode_json_line(&event);
    assert!(line.contains(r#"quote\""#));
    assert!(line.contains(r#"slash\\"#));
    assert!(line.contains(r#"newline\n"#));
    assert!(line.contains(r#"carriage\r"#));
    assert!(line.contains(r#"tab\t"#));
    assert!(line.contains(r#"backspace\b"#));
    assert!(line.contains(r#"formfeed\f"#));
    assert!(line.contains(r#"unit\u001f"#));
    assert!(!line.contains('\u{1f}'));
}

#[test]
fn rendered_all_event_trace_validates_against_contract() {
    let events = all_event_trace_events();
    let mut trace = String::new();

    for event in &events {
        let line = encode_json_line(event);
        assert_eq!(
            encoded_json_line_len(event).expect("trace line length should count"),
            line.len()
        );
        assert_rendered_required_contract_fields(event.trace_kind(), &line);
        trace.push_str(&line);
        trace.push('\n');
    }

    let summary =
        validate_runtime_trace_jsonl(&trace).expect("renderer output should validate as JSONL");
    assert_eq!(summary.event_count(), events.len());
    assert_eq!(summary.process_count(), 3);
    assert_eq!(summary.first_event(), RuntimeTraceEventKind::ArtifactLoaded);
    assert_eq!(summary.last_event(), RuntimeTraceEventKind::ProcessStopped);

    for kind in RuntimeTraceEventKind::ALL {
        assert!(
            events.iter().any(|event| event.trace_kind() == *kind),
            "renderer parity trace omitted {kind:?}"
        );
    }
}

fn all_event_trace_events() -> Vec<RuntimeEvent> {
    let main_pid = RuntimeProcessId::FIRST;
    let worker_pid = RuntimeProcessId::from_u64(2).expect("test pid should be nonzero");
    let restarted_pid = RuntimeProcessId::from_u64(3).expect("test pid should be nonzero");

    vec![
        RuntimeEvent::ArtifactLoaded {
            format: "mantle-target-artifact".to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
            source_language: "test_frontend".to_string(),
            module: "all_events".to_string(),
            entry_process_id: ProcessId::new(0),
            entry_process: "Main".to_string(),
            entry_message_id: MessageId::new(0),
            process_count: 2,
        },
        RuntimeEvent::ProcessSpawned {
            pid: main_pid,
            process_id: ProcessId::new(0),
            process: "Main".to_string(),
            state_id: StateId::new(0),
            state: "Ready".to_string(),
            mailbox_bound: 4,
            spawned_by_pid: None,
        },
        RuntimeEvent::ProcessSpawned {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            state_id: StateId::new(0),
            state: "Waiting".to_string(),
            mailbox_bound: 4,
            spawned_by_pid: Some(main_pid),
        },
        RuntimeEvent::MessageAccepted {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            payload: None,
            queue_depth: 1,
            sender_pid: Some(main_pid),
        },
        RuntimeEvent::MessageDequeued {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            payload: None,
            queue_depth: 0,
        },
        RuntimeEvent::ProgramOutput {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            stream: RuntimeOutputStream::Stdout,
            output_id: OutputId::new(0),
            text: "worker handled Work".to_string(),
        },
        RuntimeEvent::SpawnAuthorityChecked {
            pid: main_pid,
            process_id: ProcessId::new(0),
            process: "Main".to_string(),
            target_process_id: ProcessId::new(1),
            spawn_site_id: SpawnSiteId::new(0),
            authority_id: AuthorityId::new(0),
            spawn_kind: RuntimeSpawnKind::DynamicLocal,
            authority_result: RuntimeAuthorityResult::Accepted,
        },
        RuntimeEvent::BoundarySendChecked {
            pid: main_pid,
            process_id: ProcessId::new(0),
            process: "Main".to_string(),
            port_id: PortId::new(0),
            port: "WorkerPort".to_string(),
            protocol_id: ProtocolId::new(0),
            protocol: "WorkerProtocol".to_string(),
            target_process_id: ProcessId::new(1),
            target_process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            boundary_result: RuntimeAuthorityResult::Accepted,
        },
        RuntimeEvent::BranchSelected {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            branch: ArtifactBranch::Then,
            scope: RuntimeBranchScope::Action,
            branch_path: RuntimeBranchPath::root(),
            loop_context: Some(RuntimeLoopContext {
                element_id: LoopElementId::new(0),
                index: 0,
            }),
            condition_type_id: TypeId::new(0),
            condition: "True".to_string(),
        },
        RuntimeEvent::LoopStarted {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            element_id: LoopElementId::new(0),
            collection_type_id: TypeId::new(1),
            max_items: 2,
            item_count: 2,
        },
        RuntimeEvent::LoopIteration {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            element_id: LoopElementId::new(0),
            index: 0,
            element_type_id: TypeId::new(2),
            element: "True".to_string(),
        },
        RuntimeEvent::LoopCompleted {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            element_id: LoopElementId::new(0),
            iteration_count: 1,
        },
        RuntimeEvent::StateUpdated {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            from_state_id: StateId::new(0),
            from: "Waiting".to_string(),
            to_state_id: StateId::new(1),
            to: "Done".to_string(),
        },
        RuntimeEvent::ProcessStepped {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            message_id: MessageId::new(0),
            message: "Work".to_string(),
            payload: None,
            result: RuntimeStepResult::Continue,
            state_id: StateId::new(1),
            state: "Done".to_string(),
        },
        RuntimeEvent::SupervisorChildStarted {
            supervisor_pid: main_pid,
            supervisor_process_id: ProcessId::new(0),
            supervisor_process: "Main".to_string(),
            supervisor_id: SupervisorId::new(0),
            child_id: SupervisorChildId::new(0),
            child: "worker".to_string(),
            child_pid: worker_pid,
            child_process_id: ProcessId::new(1),
            child_process: "Worker".to_string(),
            spawn_site_id: SpawnSiteId::new(0),
            spawn_kind: RuntimeSpawnKind::LexicalSupervisorChild,
        },
        RuntimeEvent::ProcessFailed {
            pid: worker_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            state_id: StateId::new(1),
            state: "Done".to_string(),
            reason: RuntimeFailureReason::Panic,
        },
        RuntimeEvent::ProcessSpawned {
            pid: restarted_pid,
            process_id: ProcessId::new(1),
            process: "Worker".to_string(),
            state_id: StateId::new(0),
            state: "Waiting".to_string(),
            mailbox_bound: 4,
            spawned_by_pid: Some(main_pid),
        },
        RuntimeEvent::SupervisorRestartDecision {
            supervisor_pid: main_pid,
            supervisor_process_id: ProcessId::new(0),
            supervisor_process: "Main".to_string(),
            supervisor_id: SupervisorId::new(0),
            child_id: SupervisorChildId::new(0),
            child: "worker".to_string(),
            child_pid: worker_pid,
            child_process_id: ProcessId::new(1),
            child_process: "Worker".to_string(),
            reason: RuntimeSupervisorExitReason::Panic,
            decision: RuntimeSupervisorRestartDecision::Restarted,
            restart_time_ms: Some(0),
            restart_window_count: 1,
            restart_window_limit: 3,
            restart_window_ms: 1000,
            new_child_pid: Some(restarted_pid),
        },
        RuntimeEvent::ProcessStopped {
            pid: main_pid,
            process_id: ProcessId::new(0),
            process: "Main".to_string(),
            reason: RuntimeStopReason::Normal,
        },
    ]
}

fn assert_rendered_required_contract_fields(kind: RuntimeTraceEventKind, line: &str) {
    for field in kind.contract().required_fields() {
        assert!(
            line.contains(&format!("\"{field}\":")),
            "{kind:?} rendered trace omitted required field {field:?}: {line}"
        );
    }
}
