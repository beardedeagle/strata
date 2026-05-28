use super::RuntimeEvent;
use mantle_artifact::{ArtifactValue, Error, Result};
use std::fmt::{self, Write as _};
use std::io;

#[cfg(test)]
fn encode_json_line(event: &RuntimeEvent) -> String {
    let mut line = String::with_capacity(encoded_json_line_len(event).unwrap_or(0));
    let _ = write_json_line(&mut line, event);
    line
}

pub(crate) fn encoded_json_line_len(event: &RuntimeEvent) -> Result<usize> {
    let mut counter = JsonLenCounter::default();
    write_json_line(&mut counter, event)
        .map_err(|_| Error::new("runtime trace event size overflowed"))?;
    Ok(counter.len)
}

pub(crate) fn write_json_line_to_io(
    event: &RuntimeEvent,
    writer: &mut impl io::Write,
) -> Result<()> {
    let mut writer = IoFmtWriter::new(writer);
    write_json_line(&mut writer, event).map_err(|_| {
        writer.take_error().map_or_else(
            || Error::new("runtime trace JSON write failed"),
            Error::from,
        )
    })
}

fn write_json_line(output: &mut impl fmt::Write, event: &RuntimeEvent) -> fmt::Result {
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
        } => write!(
            output,
            "{{\"event\":\"artifact_loaded\",\"format\":\"{}\",\"schema_version\":\"{}\",\"source_language\":\"{}\",\"module\":\"{}\",\"entry_process_id\":{},\"entry_process\":\"{}\",\"entry_message_id\":{},\"process_count\":{}}}",
            JsonStr(format),
            JsonStr(schema_version),
            JsonStr(source_language),
            JsonStr(module),
            entry_process_id.as_u32(),
            JsonStr(entry_process),
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
            Some(parent_pid) => write!(
                output,
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{},\"spawned_by_pid\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                state_id.as_u32(),
                JsonStr(state),
                mailbox_bound,
                parent_pid.as_u64()
            ),
            None => write!(
                output,
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                state_id.as_u32(),
                JsonStr(state),
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
            Some(sender_pid) => write!(
                output,
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{},\"sender_pid\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                message_id.as_u32(),
                JsonStr(message),
                PayloadJson(payload),
                queue_depth,
                sender_pid.as_u64()
            ),
            None => write!(
                output,
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}}}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                message_id.as_u32(),
                JsonStr(message),
                PayloadJson(payload),
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
        } => write!(
            output,
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            PayloadJson(payload),
            queue_depth
        ),
        RuntimeEvent::ProgramOutput {
            pid,
            process_id,
            process,
            stream,
            output_id,
            text,
        } => write!(
            output,
            "{{\"event\":\"program_output\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"stream\":\"{}\",\"output_id\":{},\"text\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            stream.as_str(),
            output_id.as_u32(),
            JsonStr(text)
        ),
        RuntimeEvent::SpawnAuthorityChecked {
            pid,
            process_id,
            process,
            target_process_id,
            spawn_site_id,
            authority_id,
            spawn_kind,
            authority_result,
        } => write!(
            output,
            "{{\"event\":\"spawn_authority_checked\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"target_process_id\":{},\"spawn_site_id\":{},\"authority_id\":{},\"spawn_kind\":\"{}\",\"authority_result\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            target_process_id.as_u32(),
            spawn_site_id.as_u32(),
            authority_id.as_u32(),
            spawn_kind.as_str(),
            authority_result.as_str()
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
        } => write!(
            output,
            "{{\"event\":\"branch_selected\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"branch\":\"{}\",\"scope\":\"{}\",\"branch_path\":{}{},\"condition_type_id\":{},\"condition\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            branch.as_str(),
            scope.as_str(),
            BranchPathJson(branch_path),
            LoopContextJson(*loop_context),
            condition_type_id.as_u32(),
            JsonStr(condition)
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
        } => write!(
            output,
            "{{\"event\":\"loop_started\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"collection_type_id\":{},\"max_items\":{},\"item_count\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
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
        } => write!(
            output,
            "{{\"event\":\"loop_iteration\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"index\":{},\"element_type_id\":{},\"element\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            element_id.as_u32(),
            index,
            element_type_id.as_u32(),
            JsonStr(element)
        ),
        RuntimeEvent::LoopCompleted {
            pid,
            process_id,
            process,
            message_id,
            message,
            element_id,
            iteration_count,
        } => write!(
            output,
            "{{\"event\":\"loop_completed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"iteration_count\":{}}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
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
        } => write!(
            output,
            "{{\"event\":\"state_updated\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"from_state_id\":{},\"from\":\"{}\",\"to_state_id\":{},\"to\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            from_state_id.as_u32(),
            JsonStr(from),
            to_state_id.as_u32(),
            JsonStr(to)
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
        } => write!(
            output,
            "{{\"event\":\"process_stepped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"result\":\"{}\",\"state_id\":{},\"state\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            PayloadJson(payload),
            result.as_str(),
            state_id.as_u32(),
            JsonStr(state)
        ),
        RuntimeEvent::ProcessStopped {
            pid,
            process_id,
            process,
            reason,
        } => write!(
            output,
            "{{\"event\":\"process_stopped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"reason\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            reason.as_str()
        ),
        RuntimeEvent::ProcessFailed {
            pid,
            process_id,
            process,
            state_id,
            state,
            reason,
        } => write!(
            output,
            "{{\"event\":\"process_failed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"reason\":\"{}\"}}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            state_id.as_u32(),
            JsonStr(state),
            reason.as_str()
        ),
        RuntimeEvent::SupervisorChildStarted {
            supervisor_pid,
            supervisor_process_id,
            supervisor_process,
            supervisor_id,
            child_id,
            child,
            child_pid,
            child_process_id,
            child_process,
            spawn_site_id,
            spawn_kind,
        } => write!(
            output,
            "{{\"event\":\"supervisor_child_started\",\"supervisor_pid\":{},\"supervisor_process_id\":{},\"supervisor_process\":\"{}\",\"supervisor_id\":{},\"child_id\":{},\"child\":\"{}\",\"child_pid\":{},\"child_process_id\":{},\"child_process\":\"{}\",\"spawn_site_id\":{},\"spawn_kind\":\"{}\"}}",
            supervisor_pid.as_u64(),
            supervisor_process_id.as_u32(),
            JsonStr(supervisor_process),
            supervisor_id.as_u32(),
            child_id.as_u32(),
            JsonStr(child),
            child_pid.as_u64(),
            child_process_id.as_u32(),
            JsonStr(child_process),
            spawn_site_id.as_u32(),
            spawn_kind.as_str()
        ),
        RuntimeEvent::SupervisorRestartDecision {
            supervisor_pid,
            supervisor_process_id,
            supervisor_process,
            supervisor_id,
            child_id,
            child,
            child_pid,
            child_process_id,
            child_process,
            reason,
            decision,
            restart_time_ms,
            restart_window_count,
            restart_window_limit,
            restart_window_ms,
            new_child_pid,
        } => write!(
            output,
            "{{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":{},\"supervisor_process_id\":{},\"supervisor_process\":\"{}\",\"supervisor_id\":{},\"child_id\":{},\"child\":\"{}\",\"child_pid\":{},\"child_process_id\":{},\"child_process\":\"{}\",\"reason\":\"{}\",\"decision\":\"{}\",\"restart_time_ms\":{},\"restart_window_count\":{},\"restart_window_limit\":{},\"restart_window_ms\":{},\"new_child_pid\":{}}}",
            supervisor_pid.as_u64(),
            supervisor_process_id.as_u32(),
            JsonStr(supervisor_process),
            supervisor_id.as_u32(),
            child_id.as_u32(),
            JsonStr(child),
            child_pid.as_u64(),
            child_process_id.as_u32(),
            JsonStr(child_process),
            reason.as_str(),
            decision.as_str(),
            NullableU64(*restart_time_ms),
            restart_window_count,
            restart_window_limit,
            restart_window_ms,
            NullableProcessId(*new_child_pid)
        ),
    }
}

struct JsonStr<'a>(&'a str);

impl fmt::Display for JsonStr<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_json_escaped(formatter, self.0)
    }
}

fn write_json_escaped(output: &mut impl fmt::Write, value: &str) -> fmt::Result {
    for ch in value.chars() {
        match ch {
            '"' => output.write_str("\\\"")?,
            '\\' => output.write_str("\\\\")?,
            '\u{08}' => output.write_str("\\b")?,
            '\u{0c}' => output.write_str("\\f")?,
            '\n' => output.write_str("\\n")?,
            '\r' => output.write_str("\\r")?,
            '\t' => output.write_str("\\t")?,
            control if control.is_control() => write!(output, "\\u{:04x}", control as u32)?,
            other => output.write_char(other)?,
        }
    }
    Ok(())
}

struct BranchPathJson<'a>(&'a super::RuntimeBranchPath);

impl fmt::Display for BranchPathJson<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_char('[')?;
        for (index, segment) in self.0.segments().iter().enumerate() {
            if index > 0 {
                formatter.write_char(',')?;
            }
            write!(formatter, "{segment}")?;
        }
        formatter.write_char(']')
    }
}

struct LoopContextJson(Option<super::RuntimeLoopContext>);

impl fmt::Display for LoopContextJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(context) = self.0 {
            write!(
                formatter,
                ",\"loop_element_id\":{},\"loop_index\":{}",
                context.element_id.as_u32(),
                context.index
            )?;
        }
        Ok(())
    }
}

struct PayloadJson<'a>(&'a Option<crate::program::RuntimePayload>);

impl fmt::Display for PayloadJson<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(payload) = self.0 {
            write!(
                formatter,
                ",\"payload_type_id\":{},\"payload\":\"{}\"",
                payload.ty.as_u32(),
                JsonValueLabel(&payload.value)
            )?;
            if let Some(process_ref) = payload.process_ref {
                write!(
                    formatter,
                    ",\"payload_process_id\":{},\"payload_pid\":{}",
                    process_ref.target_process.as_u32(),
                    process_ref.pid
                )?;
            }
        }
        Ok(())
    }
}

struct JsonValueLabel<'a>(&'a ArtifactValue);

impl fmt::Display for JsonValueLabel<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_value_label_json(formatter, self.0)
    }
}

fn write_value_label_json(output: &mut impl fmt::Write, value: &ArtifactValue) -> fmt::Result {
    match value {
        ArtifactValue::Atom(value) => write_json_escaped(output, value),
        ArtifactValue::Scalar(value) => write!(output, "{}{}", value.value(), value.ty().suffix()),
        ArtifactValue::EnumVariant { variant, payload } => {
            write_json_escaped(output, variant)?;
            output.write_char('(')?;
            write_value_label_json(output, payload)?;
            output.write_char(')')
        }
        ArtifactValue::Record {
            constructor,
            fields,
        } => {
            write_json_escaped(output, constructor)?;
            output.write_char('{')?;
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_json_escaped(output, &field.name)?;
                output.write_char(':')?;
                write_value_label_json(output, &field.value)?;
            }
            output.write_char('}')
        }
        ArtifactValue::List(items) => {
            output.write_str("List[")?;
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_value_label_json(output, item)?;
            }
            output.write_char(']')
        }
        ArtifactValue::Map(entries) => {
            output.write_str("Map[")?;
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    output.write_char(',')?;
                }
                write_value_label_json(output, &entry.key)?;
                output.write_str("=>")?;
                write_value_label_json(output, &entry.value)?;
            }
            output.write_char(']')
        }
        ArtifactValue::ProcessRef { type_id, pid } => {
            write!(output, "type{}#{pid}", type_id.as_u32())
        }
    }
}

struct NullableU64(Option<u64>);

impl fmt::Display for NullableU64 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "{value}"),
            None => formatter.write_str("null"),
        }
    }
}

struct NullableProcessId(Option<super::RuntimeProcessId>);

impl fmt::Display for NullableProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "{}", value.as_u64()),
            None => formatter.write_str("null"),
        }
    }
}

#[derive(Default)]
struct JsonLenCounter {
    len: usize,
}

impl fmt::Write for JsonLenCounter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.len = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

struct IoFmtWriter<'a, W: io::Write> {
    writer: &'a mut W,
    error: Option<io::Error>,
}

impl<'a, W: io::Write> IoFmtWriter<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            error: None,
        }
    }

    fn take_error(&mut self) -> Option<io::Error> {
        self.error.take()
    }
}

impl<W: io::Write> fmt::Write for IoFmtWriter<'_, W> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.error.is_some() {
            return Err(fmt::Error);
        }
        if let Err(err) = self.writer.write_all(value.as_bytes()) {
            self.error = Some(err);
            return Err(fmt::Error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
