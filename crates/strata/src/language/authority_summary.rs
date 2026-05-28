use super::checked::{
    CheckedAuthorityId, CheckedCapabilityDescriptor, CheckedProgram, CheckedSpawnKind,
    CheckedSpawnSite, CheckedSupervisorChildMode, CheckedSupervisorStrategy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritySummaryFormat {
    Text,
    Json,
}

pub fn render_authority_summary(
    program: &CheckedProgram,
    source_path: &str,
    format: AuthoritySummaryFormat,
) -> String {
    match format {
        AuthoritySummaryFormat::Text => render_text(program, source_path),
        AuthoritySummaryFormat::Json => render_json(program, source_path),
    }
}

fn render_text(program: &CheckedProgram, source_path: &str) -> String {
    let mut out = String::new();
    out.push_str("strata authority summary ");
    out.push_str(source_path);
    out.push('\n');
    out.push_str("module: ");
    out.push_str(program.module_name());
    out.push('\n');

    for (process_index, process) in program.processes().iter().enumerate() {
        out.push_str("process ");
        out.push_str(&process_index.to_string());
        out.push(' ');
        out.push_str(process.debug_name().as_str());
        if program.entry_process().index() == process_index {
            out.push_str(" (entry)");
        }
        out.push('\n');

        if process.authorities().is_empty()
            && process.spawn_sites().is_empty()
            && process.supervisor_plans().is_empty()
        {
            out.push_str("  no local spawn authority\n");
            continue;
        }

        for (authority_index, authority) in process.authorities().iter().enumerate() {
            out.push_str("  authority ");
            out.push_str(&authority_index.to_string());
            out.push(' ');
            out.push_str(authority.debug_name().as_str());
            out.push_str(": ");
            push_checked_descriptor_text(&mut out, program, authority.descriptor());
            out.push_str(" used_by_spawn_sites=");
            push_checked_used_spawn_sites(
                &mut out,
                process.spawn_sites(),
                CheckedAuthorityId::from_index(authority_index).ok(),
            );
            out.push('\n');
        }

        for (site_index, site) in process.spawn_sites().iter().enumerate() {
            out.push_str("  spawn_site ");
            out.push_str(&site_index.to_string());
            out.push(' ');
            out.push_str(checked_spawn_kind_str(site.kind()));
            out.push_str(" target=");
            out.push_str(checked_process_label(program, site.target()));
            match site.authority() {
                Some(authority_id) => {
                    out.push_str(" authority=");
                    out.push_str(&authority_id.as_u32().to_string());
                    if let Some(authority) = process.authorities().get(authority_id.index()) {
                        out.push(' ');
                        out.push_str(authority.debug_name().as_str());
                    }
                }
                None => {
                    out.push_str(" supervisor=");
                    push_optional_id(&mut out, site.supervisor().map(|id| id.as_u32()));
                    out.push_str(" child=");
                    push_optional_id(&mut out, site.child().map(|id| id.as_u32()));
                }
            }
            out.push('\n');
        }

        for (supervisor_index, supervisor) in process.supervisor_plans().iter().enumerate() {
            out.push_str("  supervisor ");
            out.push_str(&supervisor_index.to_string());
            out.push_str(" strategy=");
            out.push_str(checked_supervisor_strategy_str(supervisor.strategy()));
            out.push_str(" max_restarts=");
            out.push_str(&supervisor.intensity().max_restarts().to_string());
            out.push_str(" within_ms=");
            out.push_str(&supervisor.intensity().within_ms().to_string());
            out.push('\n');

            for (child_index, child) in supervisor.children().iter().enumerate() {
                out.push_str("    child ");
                out.push_str(&child_index.to_string());
                out.push(' ');
                out.push_str(child.debug_name().as_str());
                out.push_str(" mode=");
                out.push_str(checked_supervisor_child_mode_str(child.mode()));
                out.push_str(" target=");
                out.push_str(checked_process_label(program, child.target()));
                out.push_str(" spawn_site=");
                out.push_str(&child.spawn_site().as_u32().to_string());
                out.push('\n');
            }
        }
    }

    out
}

fn render_json(program: &CheckedProgram, source_path: &str) -> String {
    let mut out = String::new();
    out.push('{');
    push_json_field(&mut out, "source", source_path);
    out.push(',');
    push_json_field(&mut out, "module", program.module_name());
    out.push_str(",\"entry_process_id\":");
    out.push_str(&program.entry_process().as_u32().to_string());
    out.push_str(",\"processes\":[");

    for (process_index, process) in program.processes().iter().enumerate() {
        if process_index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"process_id\":");
        out.push_str(&process_index.to_string());
        out.push(',');
        push_json_field(&mut out, "process", process.debug_name().as_str());
        out.push_str(",\"entry\":");
        out.push_str(if program.entry_process().index() == process_index {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"authorities\":[");

        for (authority_index, authority) in process.authorities().iter().enumerate() {
            if authority_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"authority_id\":");
            out.push_str(&authority_index.to_string());
            out.push(',');
            push_json_field(&mut out, "name", authority.debug_name().as_str());
            out.push_str(",\"descriptor\":");
            push_checked_descriptor_json(&mut out, program, authority.descriptor());
            out.push_str(",\"used_by_spawn_site_ids\":");
            push_checked_used_spawn_sites(
                &mut out,
                process.spawn_sites(),
                CheckedAuthorityId::from_index(authority_index).ok(),
            );
            out.push('}');
        }

        out.push_str("],\"spawn_sites\":[");
        for (site_index, site) in process.spawn_sites().iter().enumerate() {
            if site_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"spawn_site_id\":");
            out.push_str(&site_index.to_string());
            out.push(',');
            push_json_field(&mut out, "kind", checked_spawn_kind_str(site.kind()));
            out.push_str(",\"target_process_id\":");
            out.push_str(&site.target().as_u32().to_string());
            out.push(',');
            push_json_field(
                &mut out,
                "target_process",
                checked_process_label(program, site.target()),
            );
            out.push_str(",\"authority_id\":");
            match site.authority() {
                Some(authority_id) => {
                    out.push_str(&authority_id.as_u32().to_string());
                    if let Some(authority) = process.authorities().get(authority_id.index()) {
                        out.push(',');
                        push_json_field(
                            &mut out,
                            "authority_name",
                            authority.debug_name().as_str(),
                        );
                    }
                }
                None => out.push_str("null"),
            }
            if let Some(supervisor) = site.supervisor() {
                out.push_str(",\"supervisor_id\":");
                out.push_str(&supervisor.as_u32().to_string());
            }
            if let Some(child) = site.child() {
                out.push_str(",\"supervisor_child_id\":");
                out.push_str(&child.as_u32().to_string());
            }
            out.push('}');
        }
        out.push_str("],\"supervisors\":[");
        for (supervisor_index, supervisor) in process.supervisor_plans().iter().enumerate() {
            if supervisor_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"supervisor_id\":");
            out.push_str(&supervisor_index.to_string());
            out.push(',');
            push_json_field(
                &mut out,
                "strategy",
                checked_supervisor_strategy_str(supervisor.strategy()),
            );
            out.push_str(",\"max_restarts\":");
            out.push_str(&supervisor.intensity().max_restarts().to_string());
            out.push_str(",\"within_ms\":");
            out.push_str(&supervisor.intensity().within_ms().to_string());
            out.push_str(",\"children\":[");
            for (child_index, child) in supervisor.children().iter().enumerate() {
                if child_index > 0 {
                    out.push(',');
                }
                out.push('{');
                out.push_str("\"child_id\":");
                out.push_str(&child_index.to_string());
                out.push(',');
                push_json_field(&mut out, "child", child.debug_name().as_str());
                out.push(',');
                push_json_field(
                    &mut out,
                    "mode",
                    checked_supervisor_child_mode_str(child.mode()),
                );
                out.push_str(",\"target_process_id\":");
                out.push_str(&child.target().as_u32().to_string());
                out.push(',');
                push_json_field(
                    &mut out,
                    "target_process",
                    checked_process_label(program, child.target()),
                );
                out.push_str(",\"spawn_site_id\":");
                out.push_str(&child.spawn_site().as_u32().to_string());
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }

    out.push_str("]}");
    out
}

fn push_checked_descriptor_text(
    out: &mut String,
    program: &CheckedProgram,
    descriptor: CheckedCapabilityDescriptor,
) {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { target } => {
            out.push_str("Cap<Spawn<");
            out.push_str(checked_process_label(program, target));
            out.push_str(">>");
        }
    }
}

fn push_checked_descriptor_json(
    out: &mut String,
    program: &CheckedProgram,
    descriptor: CheckedCapabilityDescriptor,
) {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { target } => {
            out.push_str("{\"kind\":\"spawn\",\"target_process_id\":");
            out.push_str(&target.as_u32().to_string());
            out.push(',');
            push_json_field(
                out,
                "target_process",
                checked_process_label(program, target),
            );
            out.push('}');
        }
    }
}

fn push_checked_used_spawn_sites(
    out: &mut String,
    sites: &[CheckedSpawnSite],
    authority: Option<CheckedAuthorityId>,
) {
    out.push('[');
    let Some(authority) = authority else {
        out.push(']');
        return;
    };
    let mut needs_separator = false;
    for (site_index, site) in sites.iter().enumerate() {
        if site.authority() == Some(authority) {
            if needs_separator {
                out.push(',');
            }
            out.push_str(&site_index.to_string());
            needs_separator = true;
        }
    }
    out.push(']');
}

fn checked_process_label(program: &CheckedProgram, id: super::checked::CheckedProcessId) -> &str {
    program
        .processes()
        .get(id.index())
        .map(|process| process.debug_name().as_str())
        .unwrap_or("<invalid>")
}

fn checked_spawn_kind_str(kind: CheckedSpawnKind) -> &'static str {
    match kind {
        CheckedSpawnKind::DynamicLocal => "dynamic_local",
        CheckedSpawnKind::LexicalSupervisorChild => "lexical_supervisor_child",
    }
}

fn checked_supervisor_strategy_str(strategy: CheckedSupervisorStrategy) -> &'static str {
    match strategy {
        CheckedSupervisorStrategy::OneForOne => "one_for_one",
    }
}

fn checked_supervisor_child_mode_str(mode: CheckedSupervisorChildMode) -> &'static str {
    match mode {
        CheckedSupervisorChildMode::Permanent => "permanent",
        CheckedSupervisorChildMode::Transient => "transient",
        CheckedSupervisorChildMode::Temporary => "temporary",
    }
}

fn push_optional_id(out: &mut String, id: Option<u32>) {
    match id {
        Some(id) => out.push_str(&id.to_string()),
        None => out.push_str("none"),
    }
}

fn push_json_field(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    push_json_string(out, value);
}

fn push_json_string(out: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                let value = ch as usize;
                out.push_str("\\u00");
                out.push(HEX[value >> 4] as char);
                out.push(HEX[value & 0x0f] as char);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::check_source;

    const SOURCE: &str = r#"
module summary;

record MainState;
enum MainMsg { Start }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child supervised_worker: Worker = spawn Worker as permanent;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

    #[test]
    fn text_summary_reports_checked_authority_and_spawn_site_ids() {
        let checked = check_source(SOURCE).expect("summary source should check");

        let summary =
            render_authority_summary(&checked, "summary.str", AuthoritySummaryFormat::Text);

        assert!(summary.contains("strata authority summary summary.str"));
        assert!(
            summary
                .contains("authority 0 spawn_worker: Cap<Spawn<Worker>> used_by_spawn_sites=[0]")
                || summary.contains(
                    "authority 0 spawn_worker: Cap<Spawn<Worker>> used_by_spawn_sites=[1]"
                )
        );
        assert!(
            summary.contains("spawn_site 0 dynamic_local target=Worker authority=0 spawn_worker")
                || summary
                    .contains("spawn_site 1 dynamic_local target=Worker authority=0 spawn_worker")
        );
        assert!(
            summary.contains("supervisor 0 strategy=one_for_one max_restarts=2 within_ms=1000")
        );
        assert!(
            summary.contains("child 0 supervised_worker mode=permanent target=Worker spawn_site=")
        );
    }

    #[test]
    fn json_summary_reports_checked_authority_and_spawn_site_ids() {
        let checked = check_source(SOURCE).expect("summary source should check");

        let summary =
            render_authority_summary(&checked, "summary.str", AuthoritySummaryFormat::Json);

        assert!(summary.contains("\"module\":\"summary\""));
        assert!(summary.contains("\"authority_id\":0"));
        assert!(summary.contains("\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}"));
        assert!(
            summary.contains("\"used_by_spawn_site_ids\":[0]")
                || summary.contains("\"used_by_spawn_site_ids\":[1]")
        );
        assert!(summary.contains("\"supervisors\":[{\"supervisor_id\":0"));
        assert!(summary.contains("\"strategy\":\"one_for_one\""));
        assert!(summary.contains("\"max_restarts\":2"));
        assert!(summary.contains("\"within_ms\":1000"));
        assert!(summary.contains("\"child\":\"supervised_worker\""));
        assert!(summary.contains("\"mode\":\"permanent\""));
    }
}
