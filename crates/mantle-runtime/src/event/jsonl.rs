use super::RuntimeEvent;

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
            queue_depth,
            sender_pid,
        } => match sender_pid {
            Some(sender_pid) => format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"queue_depth\":{},\"sender_pid\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                message_id.as_u32(),
                json_escape(message),
                queue_depth,
                sender_pid.as_u64()
            ),
            None => format!(
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"queue_depth\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                json_escape(process),
                message_id.as_u32(),
                json_escape(message),
                queue_depth
            ),
        },
        RuntimeEvent::MessageDequeued {
            pid,
            process_id,
            process,
            message_id,
            message,
            queue_depth,
        } => format!(
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"queue_depth\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
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
            result,
            state_id,
            state,
        } => format!(
            "{{\"event\":\"process_stepped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"result\":\"{}\",\"state_id\":{},\"state\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            json_escape(process),
            message_id.as_u32(),
            json_escape(message),
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

#[cfg(test)]
mod tests {
    use mantle_artifact::{MessageId, OutputId, ProcessId};

    use super::*;
    use crate::{RuntimeEvent, RuntimeOutputStream, RuntimeProcessId};

    #[test]
    fn artifact_loaded_trace_includes_entry_ids() {
        let event = RuntimeEvent::ArtifactLoaded {
            format: "mantle-target-artifact".to_string(),
            schema_version: "1".to_string(),
            source_language: "strata".to_string(),
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
