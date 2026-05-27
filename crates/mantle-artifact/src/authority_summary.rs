use crate::{
    ArtifactCapabilityDescriptor, ArtifactSpawnKind, AuthorityId, MantleArtifact, ProcessId, Result,
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

        if process.authorities.is_empty() && process.spawn_sites.is_empty() {
            out.push_str("  no local spawn authority\n");
            continue;
        }

        for (authority_index, authority) in process.authorities.iter().enumerate() {
            out.push_str("  authority ");
            out.push_str(&authority_index.to_string());
            out.push(' ');
            out.push_str(&authority.debug_name);
            out.push_str(": ");
            push_artifact_descriptor_text(&mut out, artifact, authority.descriptor);
            out.push_str(" used_by_spawn_sites=");
            push_artifact_used_spawn_sites(
                &mut out,
                &process.spawn_sites,
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
            out.push_str(" authority=");
            out.push_str(&site.authority.as_u32().to_string());
            if let Some(authority) = process.authorities.get(site.authority.index()) {
                out.push(' ');
                out.push_str(&authority.debug_name);
            }
            out.push('\n');
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
            out.push_str(",\"used_by_spawn_site_ids\":");
            push_artifact_used_spawn_sites(
                &mut out,
                &process.spawn_sites,
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
            out.push_str(&site.authority.as_u32().to_string());
            if let Some(authority) = process.authorities.get(site.authority.index()) {
                out.push(',');
                push_json_field(&mut out, "authority_name", &authority.debug_name);
            }
            out.push('}');
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
        if site.authority == authority {
            if needs_separator {
                out.push(',');
            }
            out.push_str(&site_index.to_string());
            needs_separator = true;
        }
    }
    out.push(']');
}

fn artifact_process_label(artifact: &MantleArtifact, id: ProcessId) -> &str {
    artifact
        .processes
        .get(id.index())
        .map(|process| process.debug_name.as_str())
        .unwrap_or("<invalid>")
}

fn artifact_spawn_kind_str(kind: ArtifactSpawnKind) -> &'static str {
    match kind {
        ArtifactSpawnKind::DynamicLocal => "dynamic_local",
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
    use crate::{
        ARTIFACT_FORMAT, ARTIFACT_SCHEMA_VERSION, ArtifactAction, ArtifactAuthority,
        ArtifactEffect, ArtifactMessageVariant, ArtifactProcess, ArtifactProcessRef,
        ArtifactSendTarget, ArtifactSpawnSite, ArtifactStateValue, ArtifactTransition,
        ArtifactType, MessageId, NextState, ProcessRefId, SpawnSiteId, StateId, StepResult, TypeId,
    };

    #[test]
    fn text_summary_reports_artifact_authority_and_spawn_site_ids() {
        let artifact = artifact();

        let summary = render_artifact_authority_summary(
            &artifact,
            "summary.mta",
            AuthoritySummaryFormat::Text,
        )
        .expect("valid artifact authority summary should render");

        assert!(summary.contains("mantle authority summary summary.mta"));
        assert!(
            summary
                .contains("authority 0 spawn_worker: Cap<Spawn<Worker>> used_by_spawn_sites=[0]")
        );
        assert!(summary.contains(
            "spawn_site 0 dynamic_local target_process_id=1 target=Worker authority=0 spawn_worker"
        ));
    }

    #[test]
    fn json_summary_reports_artifact_authority_and_spawn_site_ids() {
        let artifact = artifact();

        let summary = render_artifact_authority_summary(
            &artifact,
            "summary.mta",
            AuthoritySummaryFormat::Json,
        )
        .expect("valid artifact authority summary should render");

        assert!(summary.contains("\"artifact\":\"summary.mta\""));
        assert!(summary.contains("\"authority_id\":0"));
        assert!(summary.contains("\"descriptor\":{\"kind\":\"spawn\",\"target_process_id\":1,\"target_process\":\"Worker\"}"));
        assert!(summary.contains("\"used_by_spawn_site_ids\":[0]"));
    }

    #[test]
    fn summary_rejects_invalid_artifact_before_rendering() {
        let mut artifact = artifact();
        artifact.processes[0].spawn_sites[0].target = ProcessId::new(99);

        let err = render_artifact_authority_summary(
            &artifact,
            "summary.mta",
            AuthoritySummaryFormat::Text,
        )
        .expect_err("invalid artifact authority summary should fail closed");

        assert!(
            err.to_string()
                .contains("spawn site 0 targets process id 99"),
            "{err}"
        );
    }

    fn artifact() -> MantleArtifact {
        MantleArtifact {
            format: ARTIFACT_FORMAT.to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION.to_string(),
            source_language: "example_lang".to_string(),
            module: "summary".to_string(),
            entry_process: ProcessId::new(0),
            entry_message: MessageId::new(0),
            types: vec![
                ArtifactType::value("MainState"),
                ArtifactType::enum_value("MainMsg", vec!["Start".to_string()]),
                ArtifactType::enum_value("WorkerState", vec!["Idle".to_string()]),
                ArtifactType::enum_value("WorkerMsg", vec!["Ping".to_string()]),
            ],
            outputs: Vec::new(),
            processes: vec![
                ArtifactProcess {
                    debug_name: "Main".to_string(),
                    state_type: TypeId::new(0),
                    state_values: vec![
                        ArtifactStateValue::new(
                            TypeId::new(0),
                            crate::ArtifactValue::Atom("MainState".to_string()),
                        )
                        .expect("state value should be valid"),
                    ],
                    message_type: TypeId::new(1),
                    message_variants: vec![ArtifactMessageVariant::unit("Start")],
                    authorities: vec![ArtifactAuthority {
                        debug_name: "spawn_worker".to_string(),
                        descriptor: ArtifactCapabilityDescriptor::Spawn {
                            target: ProcessId::new(1),
                        },
                    }],
                    spawn_sites: vec![ArtifactSpawnSite {
                        target: ProcessId::new(1),
                        authority: AuthorityId::new(0),
                        kind: ArtifactSpawnKind::DynamicLocal,
                    }],
                    process_refs: vec![ArtifactProcessRef {
                        debug_name: "worker".to_string(),
                        target: ProcessId::new(1),
                    }],
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(0),
                        payload_guard: None,
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        effects: vec![ArtifactEffect::Spawn, ArtifactEffect::Send],
                        actions: vec![
                            ArtifactAction::Spawn {
                                target: ProcessId::new(1),
                                process_ref: ProcessRefId::new(0),
                                spawn_site: SpawnSiteId::new(0),
                            },
                            ArtifactAction::Send {
                                target: ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
                                message: MessageId::new(0),
                                payload: None,
                            },
                        ],
                    }],
                },
                ArtifactProcess {
                    debug_name: "Worker".to_string(),
                    state_type: TypeId::new(2),
                    state_values: vec![
                        ArtifactStateValue::new(
                            TypeId::new(2),
                            crate::ArtifactValue::Atom("Idle".to_string()),
                        )
                        .expect("state value should be valid"),
                    ],
                    message_type: TypeId::new(3),
                    message_variants: vec![ArtifactMessageVariant::unit("Ping")],
                    authorities: Vec::new(),
                    spawn_sites: Vec::new(),
                    process_refs: Vec::new(),
                    mailbox_bound: 1,
                    init_state: StateId::new(0),
                    transitions: vec![ArtifactTransition {
                        current_state: None,
                        message: MessageId::new(0),
                        payload_guard: None,
                        step_result: StepResult::Stop,
                        next_state: NextState::Current,
                        effects: Vec::new(),
                        actions: Vec::new(),
                    }],
                },
            ],
            source_hash_fnv1a64: "0000000000000000".to_string(),
        }
    }
}
