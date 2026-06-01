use super::RuntimeEvent;
use format::{
    BranchPathJson, IoFmtWriter, JsonLenCounter, JsonStr, LoopContextJson, NullableProcessId,
    NullableU64, PayloadJson, TraceSchemaJson,
};
use mantle_artifact::{Error, Result};
use std::fmt;
use std::io;

mod format;

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
            "{{\"event\":\"artifact_loaded\",\"format\":\"{}\",\"schema_version\":\"{}\",\"source_language\":\"{}\",\"module\":\"{}\",\"entry_process_id\":{},\"entry_process\":\"{}\",\"entry_message_id\":{},\"process_count\":{}{}",
            JsonStr(format),
            JsonStr(schema_version),
            JsonStr(source_language),
            JsonStr(module),
            entry_process_id.as_u32(),
            JsonStr(entry_process),
            entry_message_id.as_u32(),
            process_count,
            TraceSchemaJson
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
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{},\"spawned_by_pid\":{}{}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                state_id.as_u32(),
                JsonStr(state),
                mailbox_bound,
                parent_pid.as_u64(),
                TraceSchemaJson
            ),
            None => write!(
                output,
                "{{\"event\":\"process_spawned\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"mailbox_bound\":{}{}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                state_id.as_u32(),
                JsonStr(state),
                mailbox_bound,
                TraceSchemaJson
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
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{},\"sender_pid\":{}{}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                message_id.as_u32(),
                JsonStr(message),
                PayloadJson(payload),
                queue_depth,
                sender_pid.as_u64(),
                TraceSchemaJson
            ),
            None => write!(
                output,
                "{{\"event\":\"message_accepted\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}{}",
                pid.as_u64(),
                process_id.as_u32(),
                JsonStr(process),
                message_id.as_u32(),
                JsonStr(message),
                PayloadJson(payload),
                queue_depth,
                TraceSchemaJson
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
            "{{\"event\":\"message_dequeued\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"queue_depth\":{}{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            PayloadJson(payload),
            queue_depth,
            TraceSchemaJson
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
            "{{\"event\":\"program_output\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"stream\":\"{}\",\"output_id\":{},\"text\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            stream.as_str(),
            output_id.as_u32(),
            JsonStr(text),
            TraceSchemaJson
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
            "{{\"event\":\"spawn_authority_checked\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"target_process_id\":{},\"spawn_site_id\":{},\"authority_id\":{},\"spawn_kind\":\"{}\",\"authority_result\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            target_process_id.as_u32(),
            spawn_site_id.as_u32(),
            authority_id.as_u32(),
            spawn_kind.as_str(),
            authority_result.as_str(),
            TraceSchemaJson
        ),
        RuntimeEvent::BoundarySendChecked {
            pid,
            process_id,
            process,
            port_id,
            port,
            protocol_id,
            protocol,
            target_process_id,
            target_process,
            message_id,
            message,
            boundary_result,
        } => write!(
            output,
            "{{\"event\":\"boundary_send_checked\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"port_id\":{},\"port\":\"{}\",\"protocol_id\":{},\"protocol\":\"{}\",\"target_process_id\":{},\"target_process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"boundary_result\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            port_id.as_u32(),
            JsonStr(port),
            protocol_id.as_u32(),
            JsonStr(protocol),
            target_process_id.as_u32(),
            JsonStr(target_process),
            message_id.as_u32(),
            JsonStr(message),
            boundary_result.as_str(),
            TraceSchemaJson
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
            "{{\"event\":\"branch_selected\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"branch\":\"{}\",\"scope\":\"{}\",\"branch_path\":{}{},\"condition_type_id\":{},\"condition\":\"{}\"{}",
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
            JsonStr(condition),
            TraceSchemaJson
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
            "{{\"event\":\"loop_started\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"collection_type_id\":{},\"max_items\":{},\"item_count\":{}{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            element_id.as_u32(),
            collection_type_id.as_u32(),
            max_items,
            item_count,
            TraceSchemaJson
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
            "{{\"event\":\"loop_iteration\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"index\":{},\"element_type_id\":{},\"element\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            element_id.as_u32(),
            index,
            element_type_id.as_u32(),
            JsonStr(element),
            TraceSchemaJson
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
            "{{\"event\":\"loop_completed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\",\"element_id\":{},\"iteration_count\":{}{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            element_id.as_u32(),
            iteration_count,
            TraceSchemaJson
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
            "{{\"event\":\"state_updated\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"from_state_id\":{},\"from\":\"{}\",\"to_state_id\":{},\"to\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            from_state_id.as_u32(),
            JsonStr(from),
            to_state_id.as_u32(),
            JsonStr(to),
            TraceSchemaJson
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
            "{{\"event\":\"process_stepped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"message_id\":{},\"message\":\"{}\"{},\"result\":\"{}\",\"state_id\":{},\"state\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            message_id.as_u32(),
            JsonStr(message),
            PayloadJson(payload),
            result.as_str(),
            state_id.as_u32(),
            JsonStr(state),
            TraceSchemaJson
        ),
        RuntimeEvent::ProcessStopped {
            pid,
            process_id,
            process,
            reason,
        } => write!(
            output,
            "{{\"event\":\"process_stopped\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"reason\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            reason.as_str(),
            TraceSchemaJson
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
            "{{\"event\":\"process_failed\",\"pid\":{},\"process_id\":{},\"process\":\"{}\",\"state_id\":{},\"state\":\"{}\",\"reason\":\"{}\"{}",
            pid.as_u64(),
            process_id.as_u32(),
            JsonStr(process),
            state_id.as_u32(),
            JsonStr(state),
            reason.as_str(),
            TraceSchemaJson
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
            "{{\"event\":\"supervisor_child_started\",\"supervisor_pid\":{},\"supervisor_process_id\":{},\"supervisor_process\":\"{}\",\"supervisor_id\":{},\"child_id\":{},\"child\":\"{}\",\"child_pid\":{},\"child_process_id\":{},\"child_process\":\"{}\",\"spawn_site_id\":{},\"spawn_kind\":\"{}\"{}",
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
            spawn_kind.as_str(),
            TraceSchemaJson
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
            "{{\"event\":\"supervisor_restart_decision\",\"supervisor_pid\":{},\"supervisor_process_id\":{},\"supervisor_process\":\"{}\",\"supervisor_id\":{},\"child_id\":{},\"child\":\"{}\",\"child_pid\":{},\"child_process_id\":{},\"child_process\":\"{}\",\"reason\":\"{}\",\"decision\":\"{}\",\"restart_time_ms\":{},\"restart_window_count\":{},\"restart_window_limit\":{},\"restart_window_ms\":{},\"new_child_pid\":{}{}",
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
            NullableProcessId(*new_child_pid),
            TraceSchemaJson
        ),
    }
}

#[cfg(test)]
mod tests;
