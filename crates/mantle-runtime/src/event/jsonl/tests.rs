use mantle_artifact::{
    ARTIFACT_SCHEMA_VERSION, ArtifactBranch, LoopElementId, MessageId, OutputId, ProcessId,
    SpawnSiteId, SupervisorChildId, SupervisorId, TypeId,
};

use super::*;
use crate::event::{RuntimeLoopContext, RuntimeSpawnKind};
use crate::{
    RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeOutputStream, RuntimeProcessId,
    RuntimeStopReason,
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
