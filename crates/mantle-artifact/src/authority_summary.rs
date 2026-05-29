use crate::{
    ArtifactAction, ArtifactCapabilityDescriptor, ArtifactSpawnKind, ArtifactTransition,
    AuthorityId, ComponentId, MantleArtifact, PortId, ProcessId, ProtocolId, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoritySummaryFormat {
    Text,
    Json,
}

pub fn render_artifact_authority_summary(
    artifact: &MantleArtifact,
    artifact_path: &str,
    format: AuthoritySummaryFormat,
) -> Result<String> {
    artifact.validate()?;
    Ok(match format {
        AuthoritySummaryFormat::Text => render_text(artifact, artifact_path),
        AuthoritySummaryFormat::Json => render_json(artifact, artifact_path),
    })
}

fn render_text(artifact: &MantleArtifact, artifact_path: &str) -> String {
    let mut out = String::new();
    out.push_str("mantle authority summary ");
    out.push_str(artifact_path);
    out.push('\n');
    out.push_str("format: ");
    out.push_str(&artifact.format);
    out.push('\n');
    out.push_str("schema_version: ");
    out.push_str(&artifact.schema_version);
    out.push('\n');
    out.push_str("source_language: ");
    out.push_str(&artifact.source_language);
    out.push('\n');
    out.push_str("module: ");
    out.push_str(&artifact.module);
    out.push('\n');

    for (process_index, process) in artifact.processes.iter().enumerate() {
        out.push_str("process ");
        out.push_str(&process_index.to_string());
        out.push(' ');
        out.push_str(&process.debug_name);
        if artifact.entry_process.index() == process_index {
            out.push_str(" (entry)");
        }
        out.push('\n');

        if process.authorities.is_empty()
            && process.spawn_sites.is_empty()
            && process.supervisor_plans.is_empty()
        {
            out.push_str("  no local authority\n");
            continue;
        }

        for (authority_index, authority) in process.authorities.iter().enumerate() {
            out.push_str("  authority ");
            out.push_str(&authority_index.to_string());
            out.push(' ');
            out.push_str(&authority.debug_name);
            out.push_str(": ");
            push_artifact_descriptor_text(&mut out, artifact, authority.descriptor);
            push_artifact_authority_usage_text(
                &mut out,
                &process.spawn_sites,
                &process.transitions,
                authority.descriptor,
                AuthorityId::from_index(authority_index).ok(),
            );
            out.push('\n');
        }

        for (site_index, site) in process.spawn_sites.iter().enumerate() {
            out.push_str("  spawn_site ");
            out.push_str(&site_index.to_string());
            out.push(' ');
            out.push_str(artifact_spawn_kind_str(site.kind));
            out.push_str(" target_process_id=");
            out.push_str(&site.target.as_u32().to_string());
            out.push_str(" target=");
            out.push_str(artifact_process_label(artifact, site.target));
            match site.authority {
                Some(authority_id) => {
                    out.push_str(" authority=");
                    out.push_str(&authority_id.as_u32().to_string());
                    if let Some(authority) = process.authorities.get(authority_id.index()) {
                        out.push(' ');
                        out.push_str(&authority.debug_name);
                    }
                }
                None => {
                    out.push_str(" supervisor=");
                    push_optional_id(&mut out, site.supervisor.map(|id| id.as_u32()));
                    out.push_str(" child=");
                    push_optional_id(&mut out, site.child.map(|id| id.as_u32()));
                }
            }
            out.push('\n');
        }

        for (supervisor_index, supervisor) in process.supervisor_plans.iter().enumerate() {
            out.push_str("  supervisor ");
            out.push_str(&supervisor_index.to_string());
            out.push_str(" strategy=");
            out.push_str(supervisor.strategy.as_str());
            out.push_str(" max_restarts=");
            out.push_str(&supervisor.intensity.max_restarts.to_string());
            out.push_str(" within_ms=");
            out.push_str(&supervisor.intensity.within_ms.to_string());
            out.push('\n');

            for (child_index, child) in supervisor.children.iter().enumerate() {
                out.push_str("    child ");
                out.push_str(&child_index.to_string());
                out.push(' ');
                out.push_str(&child.debug_name);
                out.push_str(" mode=");
                out.push_str(child.mode.as_str());
                out.push_str(" target_process_id=");
                out.push_str(&child.target.as_u32().to_string());
                out.push_str(" target=");
                out.push_str(artifact_process_label(artifact, child.target));
                out.push_str(" spawn_site=");
                out.push_str(&child.spawn_site.as_u32().to_string());
                out.push('\n');
            }
        }
    }

    out
}

fn render_json(artifact: &MantleArtifact, artifact_path: &str) -> String {
    let mut out = String::new();
    out.push('{');
    push_json_field(&mut out, "artifact", artifact_path);
    out.push(',');
    push_json_field(&mut out, "format", &artifact.format);
    out.push(',');
    push_json_field(&mut out, "schema_version", &artifact.schema_version);
    out.push(',');
    push_json_field(&mut out, "source_language", &artifact.source_language);
    out.push(',');
    push_json_field(&mut out, "module", &artifact.module);
    out.push_str(",\"entry_process_id\":");
    out.push_str(&artifact.entry_process.as_u32().to_string());
    out.push_str(",\"processes\":[");

    for (process_index, process) in artifact.processes.iter().enumerate() {
        if process_index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"process_id\":");
        out.push_str(&process_index.to_string());
        out.push(',');
        push_json_field(&mut out, "process", &process.debug_name);
        out.push_str(",\"entry\":");
        out.push_str(if artifact.entry_process.index() == process_index {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"authorities\":[");

        for (authority_index, authority) in process.authorities.iter().enumerate() {
            if authority_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"authority_id\":");
            out.push_str(&authority_index.to_string());
            out.push(',');
            push_json_field(&mut out, "name", &authority.debug_name);
            out.push_str(",\"descriptor\":");
            push_artifact_descriptor_json(&mut out, artifact, authority.descriptor);
            push_artifact_authority_usage_json(
                &mut out,
                &process.spawn_sites,
                &process.transitions,
                authority.descriptor,
                AuthorityId::from_index(authority_index).ok(),
            );
            out.push('}');
        }

        out.push_str("],\"spawn_sites\":[");
        for (site_index, site) in process.spawn_sites.iter().enumerate() {
            if site_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"spawn_site_id\":");
            out.push_str(&site_index.to_string());
            out.push(',');
            push_json_field(&mut out, "kind", artifact_spawn_kind_str(site.kind));
            out.push_str(",\"target_process_id\":");
            out.push_str(&site.target.as_u32().to_string());
            out.push(',');
            push_json_field(
                &mut out,
                "target_process",
                artifact_process_label(artifact, site.target),
            );
            out.push_str(",\"authority_id\":");
            match site.authority {
                Some(authority_id) => {
                    out.push_str(&authority_id.as_u32().to_string());
                    if let Some(authority) = process.authorities.get(authority_id.index()) {
                        out.push(',');
                        push_json_field(&mut out, "authority_name", &authority.debug_name);
                    }
                }
                None => out.push_str("null"),
            }
            if let Some(supervisor) = site.supervisor {
                out.push_str(",\"supervisor_id\":");
                out.push_str(&supervisor.as_u32().to_string());
            }
            if let Some(child) = site.child {
                out.push_str(",\"supervisor_child_id\":");
                out.push_str(&child.as_u32().to_string());
            }
            out.push('}');
        }
        out.push_str("],\"supervisors\":[");
        for (supervisor_index, supervisor) in process.supervisor_plans.iter().enumerate() {
            if supervisor_index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str("\"supervisor_id\":");
            out.push_str(&supervisor_index.to_string());
            out.push(',');
            push_json_field(&mut out, "strategy", supervisor.strategy.as_str());
            out.push_str(",\"max_restarts\":");
            out.push_str(&supervisor.intensity.max_restarts.to_string());
            out.push_str(",\"within_ms\":");
            out.push_str(&supervisor.intensity.within_ms.to_string());
            out.push_str(",\"children\":[");
            for (child_index, child) in supervisor.children.iter().enumerate() {
                if child_index > 0 {
                    out.push(',');
                }
                out.push('{');
                out.push_str("\"child_id\":");
                out.push_str(&child_index.to_string());
                out.push(',');
                push_json_field(&mut out, "child", &child.debug_name);
                out.push(',');
                push_json_field(&mut out, "mode", child.mode.as_str());
                out.push_str(",\"target_process_id\":");
                out.push_str(&child.target.as_u32().to_string());
                out.push(',');
                push_json_field(
                    &mut out,
                    "target_process",
                    artifact_process_label(artifact, child.target),
                );
                out.push_str(",\"spawn_site_id\":");
                out.push_str(&child.spawn_site.as_u32().to_string());
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }

    out.push_str("]}");
    out
}

fn push_artifact_descriptor_text(
    out: &mut String,
    artifact: &MantleArtifact,
    descriptor: ArtifactCapabilityDescriptor,
) {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { target } => {
            out.push_str("Cap<Spawn<");
            out.push_str(artifact_process_label(artifact, target));
            out.push_str(">>");
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { protocol } => {
            out.push_str("Cap<ProtocolBoundary<");
            out.push_str(artifact_protocol_label(artifact, protocol));
            out.push_str(">>");
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            out.push_str("Cap<PortConnect<");
            out.push_str(artifact_port_label(artifact, port));
            out.push_str(">>");
        }
        ArtifactCapabilityDescriptor::ComponentExport { component } => {
            out.push_str("Cap<ComponentExport<");
            out.push_str(artifact_component_label(artifact, component));
            out.push_str(">>");
        }
    }
}

fn push_artifact_descriptor_json(
    out: &mut String,
    artifact: &MantleArtifact,
    descriptor: ArtifactCapabilityDescriptor,
) {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { target } => {
            out.push_str("{\"kind\":\"spawn\",\"target_process_id\":");
            out.push_str(&target.as_u32().to_string());
            out.push(',');
            push_json_field(
                out,
                "target_process",
                artifact_process_label(artifact, target),
            );
            out.push('}');
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { protocol } => {
            out.push_str("{\"kind\":\"protocol_boundary\",\"protocol_id\":");
            out.push_str(&protocol.as_u32().to_string());
            out.push(',');
            push_json_field(out, "protocol", artifact_protocol_label(artifact, protocol));
            out.push('}');
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            out.push_str("{\"kind\":\"port_connect\",\"port_id\":");
            out.push_str(&port.as_u32().to_string());
            out.push(',');
            push_json_field(out, "port", artifact_port_label(artifact, port));
            out.push('}');
        }
        ArtifactCapabilityDescriptor::ComponentExport { component } => {
            out.push_str("{\"kind\":\"component_export\",\"component_id\":");
            out.push_str(&component.as_u32().to_string());
            out.push(',');
            push_json_field(
                out,
                "component",
                artifact_component_label(artifact, component),
            );
            out.push('}');
        }
    }
}

fn push_artifact_used_spawn_sites(
    out: &mut String,
    sites: &[crate::ArtifactSpawnSite],
    authority: Option<AuthorityId>,
) {
    out.push('[');
    let Some(authority) = authority else {
        out.push(']');
        return;
    };
    let mut needs_separator = false;
    for (site_index, site) in sites.iter().enumerate() {
        if site.authority == Some(authority) {
            if needs_separator {
                out.push(',');
            }
            out.push_str(&site_index.to_string());
            needs_separator = true;
        }
    }
    out.push(']');
}

fn push_artifact_authority_usage_text(
    out: &mut String,
    sites: &[crate::ArtifactSpawnSite],
    transitions: &[ArtifactTransition],
    descriptor: ArtifactCapabilityDescriptor,
    authority: Option<AuthorityId>,
) {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { .. } => {
            out.push_str(" used_by_spawn_sites=");
            push_artifact_used_spawn_sites(out, sites, authority);
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            out.push_str(" used_by_port_ids=");
            push_artifact_used_port_sends(out, transitions, port);
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { .. }
        | ArtifactCapabilityDescriptor::ComponentExport { .. } => out.push_str(" used_by=[]"),
    }
}

fn push_artifact_authority_usage_json(
    out: &mut String,
    sites: &[crate::ArtifactSpawnSite],
    transitions: &[ArtifactTransition],
    descriptor: ArtifactCapabilityDescriptor,
    authority: Option<AuthorityId>,
) {
    match descriptor {
        ArtifactCapabilityDescriptor::Spawn { .. } => {
            out.push_str(",\"used_by_spawn_site_ids\":");
            push_artifact_used_spawn_sites(out, sites, authority);
        }
        ArtifactCapabilityDescriptor::PortConnect { port } => {
            out.push_str(",\"used_by_port_ids\":");
            push_artifact_used_port_sends(out, transitions, port);
        }
        ArtifactCapabilityDescriptor::ProtocolBoundary { .. }
        | ArtifactCapabilityDescriptor::ComponentExport { .. } => out.push_str(",\"used_by\":[]"),
    }
}

fn push_artifact_used_port_sends(
    out: &mut String,
    transitions: &[ArtifactTransition],
    port: PortId,
) {
    out.push('[');
    if transitions
        .iter()
        .any(|transition| artifact_actions_use_port(&transition.actions, port))
    {
        out.push_str(&port.as_u32().to_string());
    }
    out.push(']');
}

fn artifact_actions_use_port(actions: &[ArtifactAction], expected: PortId) -> bool {
    actions.iter().any(|action| match action {
        ArtifactAction::Send { port, .. } | ArtifactAction::SendOutcome { port, .. } => {
            port.is_some_and(|port| port == expected)
        }
        ArtifactAction::IfElse {
            then_actions,
            else_actions,
            ..
        } => {
            artifact_actions_use_port(then_actions, expected)
                || artifact_actions_use_port(else_actions, expected)
        }
        ArtifactAction::ForEach { body, .. } => artifact_actions_use_port(body, expected),
        ArtifactAction::Emit { .. }
        | ArtifactAction::Spawn { .. }
        | ArtifactAction::SpawnOutcome { .. } => false,
    })
}

fn artifact_process_label(artifact: &MantleArtifact, id: ProcessId) -> &str {
    artifact
        .processes
        .get(id.index())
        .map(|process| process.debug_name.as_str())
        .unwrap_or("<invalid>")
}

fn artifact_protocol_label(artifact: &MantleArtifact, id: ProtocolId) -> &str {
    artifact
        .protocols
        .get(id.index())
        .map(|protocol| protocol.debug_name.as_str())
        .unwrap_or("<invalid>")
}

fn artifact_port_label(artifact: &MantleArtifact, id: PortId) -> &str {
    artifact
        .ports
        .get(id.index())
        .map(|port| port.debug_name.as_str())
        .unwrap_or("<invalid>")
}

fn artifact_component_label(artifact: &MantleArtifact, id: ComponentId) -> &str {
    artifact
        .components
        .get(id.index())
        .map(|component| component.debug_name.as_str())
        .unwrap_or("<invalid>")
}

fn artifact_spawn_kind_str(kind: ArtifactSpawnKind) -> &'static str {
    match kind {
        ArtifactSpawnKind::DynamicLocal => "dynamic_local",
        ArtifactSpawnKind::LexicalSupervisorChild => "lexical_supervisor_child",
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
mod tests;
