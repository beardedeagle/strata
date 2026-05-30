use super::checked::{
    CheckedAction, CheckedAuthorityId, CheckedCapabilityDescriptor, CheckedPortId, CheckedProgram,
    CheckedSpawnKind, CheckedSpawnSite, CheckedSupervisorChildMode, CheckedSupervisorStrategy,
    CheckedTransition,
};
use super::checked_render::{
    checked_process_label, push_checked_descriptor_json, push_checked_descriptor_text,
    push_json_field,
};
use super::component_authority_edges::{
    push_component_authority_edges_json, push_component_authority_edges_text,
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
            out.push_str("  no local authority\n");
            continue;
        }

        for (authority_index, authority) in process.authorities().iter().enumerate() {
            out.push_str("  authority ");
            out.push_str(&authority_index.to_string());
            out.push(' ');
            out.push_str(authority.debug_name().as_str());
            out.push_str(": ");
            push_checked_descriptor_text(&mut out, program, authority.descriptor());
            push_checked_authority_usage_text(
                &mut out,
                process.spawn_sites(),
                process.transitions(),
                authority.descriptor(),
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

    push_component_authority_edges_text(&mut out, program);

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
            push_checked_authority_usage_json(
                &mut out,
                process.spawn_sites(),
                process.transitions(),
                authority.descriptor(),
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

    out.push_str("],\"component_authority_edges\":[");
    push_component_authority_edges_json(&mut out, program);
    out.push_str("]}");
    out
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

fn push_checked_authority_usage_text(
    out: &mut String,
    sites: &[CheckedSpawnSite],
    transitions: &[CheckedTransition],
    descriptor: CheckedCapabilityDescriptor,
    authority: Option<CheckedAuthorityId>,
) {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { .. } => {
            out.push_str(" used_by_spawn_sites=");
            push_checked_used_spawn_sites(out, sites, authority);
        }
        CheckedCapabilityDescriptor::PortConnect { port } => {
            out.push_str(" used_by_port_ids=");
            push_checked_used_port_sends(out, transitions, port);
        }
        CheckedCapabilityDescriptor::ProtocolBoundary { .. }
        | CheckedCapabilityDescriptor::ComponentExport { .. } => out.push_str(" used_by=[]"),
    }
}

fn push_checked_authority_usage_json(
    out: &mut String,
    sites: &[CheckedSpawnSite],
    transitions: &[CheckedTransition],
    descriptor: CheckedCapabilityDescriptor,
    authority: Option<CheckedAuthorityId>,
) {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { .. } => {
            out.push_str(",\"used_by_spawn_site_ids\":");
            push_checked_used_spawn_sites(out, sites, authority);
        }
        CheckedCapabilityDescriptor::PortConnect { port } => {
            out.push_str(",\"used_by_port_ids\":");
            push_checked_used_port_sends(out, transitions, port);
        }
        CheckedCapabilityDescriptor::ProtocolBoundary { .. }
        | CheckedCapabilityDescriptor::ComponentExport { .. } => out.push_str(",\"used_by\":[]"),
    }
}

fn push_checked_used_port_sends(
    out: &mut String,
    transitions: &[CheckedTransition],
    port: CheckedPortId,
) {
    out.push('[');
    if transitions
        .iter()
        .any(|transition| checked_actions_use_port(transition.actions(), port))
    {
        out.push_str(&port.as_u32().to_string());
    }
    out.push(']');
}

fn checked_actions_use_port(actions: &[CheckedAction], expected: CheckedPortId) -> bool {
    actions.iter().any(|action| match action {
        CheckedAction::Send { port, .. } | CheckedAction::SendOutcome { port, .. } => {
            port.is_some_and(|port| port == expected)
        }
        CheckedAction::IfElse {
            then_actions,
            else_actions,
            ..
        } => {
            checked_actions_use_port(then_actions, expected)
                || checked_actions_use_port(else_actions, expected)
        }
        CheckedAction::ForEach { body, .. } => checked_actions_use_port(body, expected),
        CheckedAction::Emit { .. }
        | CheckedAction::Spawn { .. }
        | CheckedAction::SpawnOutcome { .. } => false,
    })
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

protocol WorkerProtocol message WorkerMsg requires Cap<ProtocolBoundary<WorkerProtocol>>;
port WorkerPort protocol WorkerProtocol target Worker requires Cap<PortConnect<WorkerPort>>;
component WorkerComponent exports WorkerPort requires Cap<ComponentExport<WorkerComponent>>;

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;
    authority connect_worker: Cap<PortConnect<WorkerPort>>;
    supervise local one_for_one(max_restarts: 2_u32, within_ms: 1000_u64) {
        child supervised_worker: Worker = spawn Worker as permanent;
    }

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker via WorkerPort Ping;
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
        assert!(summary.contains(
            "authority 1 connect_worker: Cap<PortConnect<WorkerPort>> used_by_port_ids=[0]"
        ));
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
        assert!(summary.contains(
            "\"descriptor\":{\"kind\":\"port_connect\",\"port_id\":0,\"port\":\"WorkerPort\"}"
        ));
        assert!(summary.contains("\"used_by_port_ids\":[0]"));
        assert!(summary.contains("\"supervisors\":[{\"supervisor_id\":0"));
        assert!(summary.contains("\"strategy\":\"one_for_one\""));
        assert!(summary.contains("\"max_restarts\":2"));
        assert!(summary.contains("\"within_ms\":1000"));
        assert!(summary.contains("\"child\":\"supervised_worker\""));
        assert!(summary.contains("\"mode\":\"permanent\""));
    }
}
