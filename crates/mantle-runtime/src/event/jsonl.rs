use super::RuntimeEvent;
use std::fmt::Write as _;

pub(crate) fn encode_json_line(event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::ArtifactLoaded {
            format,
            schema_version,
            source_language,
            module,
            entry_process_id,
            entry_process,
            entry_message_id,
            process_count,
        } => format!(
            "{{\"event\":\"artifact_loaded\",\"format\":\"{}\",\"schema_version\":\"{}\",\"source_language\":\"{}\",\"module\":\"{}\",\"entry_process_id\":{},\"entry_process\":\"{}\",\"entry_message_id\":{},\"process_count\":{}}}",
            json_escape(format),
            json_escape(schema_version),
            json_escape(source_language),
            json_escape(module),
            entry_process_id.as_u32(),
            json_escape(entry_process),
            entry_message_id.as_u32(),
            process_count
        ),
        RuntimeEvent::ProcessSpawned {
            pid,
            process_id,
            process,
            state_id,
            state,
            mailbox_bound,
            spawned_by_pid,
        } => match spawned_by_pid {
            Some(parent_pid) => format!(
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{},\"spawned_by_pid\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                state_id.as_u32(),
                json_escape(state),
                mailbox_bound,
                parent_pid.as_u64()
            ),
            None => format!(
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                state_id.as_u32(),
                json_escape(state),
                mailbox_bound
            ),
        },
        RuntimeEvent::MessageAccepted {
            pid,
            process_id,
            process,
            message_id,
            message,
            payload,
            queue_depth,
            sender_pid,
        } => match sender_pid {
            Some(sender_pid) => format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{},\"sender_pid\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                message_id.as_u32(),
                json_escape(message),
                payload_json(payload),
                queue_depth,
                sender_pid.as_u64()
            ),
            None => format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                message_id.as_u32(),
                json_escape(message),
                payload_json(payload),
                queue_depth
            ),
        },
        RuntimeEvent::MessageDequeued {
            pid,
            process_id,
            process,
            message_id,
            message,
            payload,
            queue_depth,
        } => format!(
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            payload_json(payload),
            queue_depth
        ),
        RuntimeEvent::ProgramOutput {
            pid,
            process_id,
            process,
            stream,
            output_id,
            text,
        } => format!(
            "{{\"event\":\"program_output\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"stream\":\"{}\",\"output_id\":{},\"text\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            stream.as_str(),
            output_id.as_u32(),
            json_escape(text)
        ),
        RuntimeEvent::BranchSelected {
            pid,
            process_id,
            process,
            message_id,
            message,
            branch,
            scope,
            branch_path,
            loop_context,
            condition_type_id,
            condition,
        } => format!(
            "{{\"event\":\"branch_selected\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"branch\":\"{}\",\"scope\":\"{}\",\"branch_path\":{}{},\"condition_type_id\":{},\"condition\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            branch.as_str(),
            scope.as_str(),
            branch_path_json(branch_path),
            loop_context_json(*loop_context),
            condition_type_id.as_u32(),
            json_escape(condition)
        ),
        RuntimeEvent::LoopStarted {
            pid,
            process_id,
            process,
            message_id,
            message,
            element_id,
            collection_type_id,
            max_items,
            item_count,
        } => format!(
            "{{\"event\":\"loop_started\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"collection_type_id\":{},\"max_items\":{},\"item_count\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            element_id.as_u32(),
            collection_type_id.as_u32(),
            max_items,
            item_count
        ),
        RuntimeEvent::LoopIteration {
            pid,
            process_id,
            process,
            message_id,
            message,
            element_id,
            index,
            element_type_id,
            element,
        } => format!(
            "{{\"event\":\"loop_iteration\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"index\":{},\"element_type_id\":{},\"element\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            element_id.as_u32(),
            index,
            element_type_id.as_u32(),
            json_escape(element)
        ),
        RuntimeEvent::LoopCompleted {
            pid,
            process_id,
            process,
            message_id,
            message,
            element_id,
            iteration_count,
        } => format!(
            "{{\"event\":\"loop_completed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"iteration_count\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            element_id.as_u32(),
            iteration_count
        ),
        RuntimeEvent::StateUpdated {
            pid,
            process_id,
            process,
            from_state_id,
            from,
            to_state_id,
            to,
        } => format!(
            "{{\"event\":\"state_updated\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"from_state_id\":{},\"from\":\"{}\",\"to_state_id\":{},\"to\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            from_state_id.as_u32(),
            json_escape(from),
            to_state_id.as_u32(),
            json_escape(to)
        ),
        RuntimeEvent::ProcessStepped {
            pid,
            process_id,
            process,
            message_id,
            message,
            payload,
            result,
            state_id,
            state,
        } => format!(
            "{{\"event\":\"process_stepped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"result\":\"{}\",\"state_id\":{},\"state\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
            payload_json(payload),
            result.as_str(),
            state_id.as_u32(),
            json_escape(state)
        ),
        RuntimeEvent::ProcessStopped {
            pid,
            process_id,
            process,
            reason,
        } => format!(
            "{{\"event\":\"process_stopped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"reason\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            reason.as_str()
        ),
        RuntimeEvent::ProcessFailed {
            pid,
            process_id,
            process,
            state_id,
            state,
            reason,
        } => format!(
            "{{\"event\":\"process_failed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"reason\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            state_id.as_u32(),
            json_escape(state),
            reason.as_str()
        ),
    }
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                push_json_unicode_escape(&mut escaped, control as u32);
            }
            other => escaped.push(other),
        }
    }
    escaped
}

fn push_json_unicode_escape(output: &mut String, codepoint: u32) {
    output.push_str("\\u");
    for shift in [12, 8, 4, 0] {
        output.push(hex_digit(codepoint >> shift));
    }
}

fn hex_digit(value: u32) -> char {
    let nibble = value & 0x0f;
    match nibble {
        0..=9 => char::from(b'0' + nibble as u8),
        10..=15 => char::from(b'a' + (nibble as u8 - 10)),
        _ => '0',
    }
}

fn branch_path_json(path: &super::RuntimeBranchPath) -> String {
    let segments = path.segments();
    let mut json = String::with_capacity(2 + segments.len().saturating_mul(6));
    json.push('[');
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        let _ = write!(&mut json, "{segment}");
    }
    json.push(']');
    json
}

fn loop_context_json(context: Option<super::RuntimeLoopContext>) -> String {
    match context {
        Some(context) => {
            format!(
                ",\"loop_element_id\":{},\"loop_index\":{}",
                context.element_id.as_u32(),
                context.index
            )
        }
        None => String::new(),
    }
}

fn payload_json(payload: &Option<crate::program::RuntimePayload>) -> String {
    match payload {
        Some(payload) => {
            let mut json = format!(
                ",\"payload_type_id\":{},\"payload\":\"{}\"",
                payload.ty.as_u32(),
                json_escape(payload.label())
            );
            if let Some(process_ref) = payload.process_ref {
                json.push_str(&format!(
                    ",\"payload_process_id\":{},\"payload_pid\":{}",
                    process_ref.target_process.as_u32(),
                    process_ref.pid
                ));
            }
            json
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use mantle_artifact::{
        ARTIFACT_SCHEMA_VERSION, ArtifactBranch, LoopElementId, MessageId, OutputId, ProcessId,
        TypeId,
    };

    use super::*;
    use crate::event::RuntimeLoopContext;
    use crate::{
        RuntimeBranchPath, RuntimeBranchScope, RuntimeEvent, RuntimeOutputStream, RuntimeProcessId,
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
            text: "quote\" slash\\ newline\n carriage\r tab\t backspace\u{08} formfeed\u{0c} unit\u{1f}".to_string(),
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
}
