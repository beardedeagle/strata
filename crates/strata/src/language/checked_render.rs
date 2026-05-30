use std::fmt::Write as _;

use super::checked::{
    CheckedCapabilityDescriptor, CheckedComponentId, CheckedComponentInstance,
    CheckedComponentInstanceId, CheckedPortId, CheckedProcessId, CheckedProgram, CheckedProtocolId,
};

pub(super) fn push_checked_descriptor_text(
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
        CheckedCapabilityDescriptor::ProtocolBoundary { protocol } => {
            out.push_str("Cap<ProtocolBoundary<");
            out.push_str(checked_protocol_label(program, protocol));
            out.push_str(">>");
        }
        CheckedCapabilityDescriptor::PortConnect { port } => {
            out.push_str("Cap<PortConnect<");
            out.push_str(checked_port_label(program, port));
            out.push_str(">>");
        }
        CheckedCapabilityDescriptor::ComponentExport { component } => {
            out.push_str("Cap<ComponentExport<");
            out.push_str(checked_component_label(program, component));
            out.push_str(">>");
        }
    }
}

pub(super) fn push_checked_descriptor_json(
    out: &mut String,
    program: &CheckedProgram,
    descriptor: CheckedCapabilityDescriptor,
) {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { target } => {
            out.push_str("{\"kind\":\"spawn\",\"target_process_id\":");
            let _ = write!(out, "{}", target.as_u32());
            out.push(',');
            push_json_field(
                out,
                "target_process",
                checked_process_label(program, target),
            );
            out.push('}');
        }
        CheckedCapabilityDescriptor::ProtocolBoundary { protocol } => {
            out.push_str("{\"kind\":\"protocol_boundary\",\"protocol_id\":");
            let _ = write!(out, "{}", protocol.as_u32());
            out.push(',');
            push_json_field(out, "protocol", checked_protocol_label(program, protocol));
            out.push('}');
        }
        CheckedCapabilityDescriptor::PortConnect { port } => {
            out.push_str("{\"kind\":\"port_connect\",\"port_id\":");
            let _ = write!(out, "{}", port.as_u32());
            out.push(',');
            push_json_field(out, "port", checked_port_label(program, port));
            out.push('}');
        }
        CheckedCapabilityDescriptor::ComponentExport { component } => {
            out.push_str("{\"kind\":\"component_export\",\"component_id\":");
            let _ = write!(out, "{}", component.as_u32());
            out.push(',');
            push_json_field(
                out,
                "component",
                checked_component_label(program, component),
            );
            out.push('}');
        }
    }
}

pub(super) fn push_component_instance_ref_json(
    out: &mut String,
    prefix: &str,
    instances: &[CheckedComponentInstance],
    id: CheckedComponentInstanceId,
) {
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_instance_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_instance\":");
    push_json_string(out, checked_component_instance_label(instances, id));
}

pub(super) fn push_component_ref_json(
    out: &mut String,
    prefix: &str,
    program: &CheckedProgram,
    id: CheckedComponentId,
) {
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("\":");
    push_json_string(out, checked_component_label(program, id));
}

pub(super) fn push_port_ref_json(
    out: &mut String,
    prefix: &str,
    program: &CheckedProgram,
    id: CheckedPortId,
) {
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("\":");
    push_json_string(out, checked_port_label(program, id));
}

pub(super) fn push_protocol_ref_json(
    out: &mut String,
    program: &CheckedProgram,
    id: CheckedProtocolId,
) {
    out.push_str(",\"protocol_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"protocol\":");
    push_json_string(out, checked_protocol_label(program, id));
}

pub(super) fn port_protocol(program: &CheckedProgram, id: CheckedPortId) -> CheckedProtocolId {
    program.ports()[id.index()].protocol()
}

pub(super) fn checked_component_authority(
    program: &CheckedProgram,
    id: CheckedComponentId,
) -> CheckedCapabilityDescriptor {
    program.components()[id.index()].required_authority()
}

pub(super) fn checked_port_authority(
    program: &CheckedProgram,
    id: CheckedPortId,
) -> CheckedCapabilityDescriptor {
    program.ports()[id.index()].required_authority()
}

pub(super) fn checked_process_label(program: &CheckedProgram, id: CheckedProcessId) -> &str {
    program
        .processes()
        .get(id.index())
        .map(|process| process.debug_name().as_str())
        .unwrap_or("<invalid-process>")
}

pub(super) fn checked_protocol_label(program: &CheckedProgram, id: CheckedProtocolId) -> &str {
    program
        .protocols()
        .get(id.index())
        .map(|protocol| protocol.debug_name().as_str())
        .unwrap_or("<invalid-protocol>")
}

pub(super) fn checked_port_label(program: &CheckedProgram, id: CheckedPortId) -> &str {
    program
        .ports()
        .get(id.index())
        .map(|port| port.debug_name().as_str())
        .unwrap_or("<invalid-port>")
}

pub(super) fn checked_component_label(program: &CheckedProgram, id: CheckedComponentId) -> &str {
    program
        .components()
        .get(id.index())
        .map(|component| component.debug_name().as_str())
        .unwrap_or("<invalid-component>")
}

pub(super) fn push_json_field(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    push_json_string(out, value);
}

pub(super) fn push_text_metadata_value(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
}

fn checked_component_instance_label(
    instances: &[CheckedComponentInstance],
    id: CheckedComponentInstanceId,
) -> &str {
    instances
        .get(id.index())
        .map(|instance| instance.debug_name().as_str())
        .unwrap_or("<invalid-component-instance>")
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
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}
