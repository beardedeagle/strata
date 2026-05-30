use std::fmt::Write as _;

use super::checked::{
    CheckedCapabilityDescriptor, CheckedComponentId, CheckedComponentInstance, CheckedPortBinding,
    CheckedPortId, CheckedProgram, CheckedProtocolId,
};

pub(super) fn push_component_authority_edges_text(out: &mut String, program: &CheckedProgram) {
    let mut found_edge = false;
    for (composition_index, composition) in program.compositions().iter().enumerate() {
        for binding in composition.port_bindings() {
            if !found_edge {
                out.push_str("component_authority_edges:\n");
                found_edge = true;
            }
            let _ = write!(
                out,
                "  edge composition={} {} port_binding={} ",
                composition_index,
                composition.debug_name(),
                binding.id().as_u32()
            );
            push_component_authority_edge_text(
                out,
                program,
                composition.component_instances(),
                binding,
            );
            out.push('\n');
        }
    }
    if !found_edge {
        out.push_str("component_authority_edges: []\n");
    }
}

pub(super) fn push_component_authority_edges_json(out: &mut String, program: &CheckedProgram) {
    let mut needs_separator = false;
    for (composition_index, composition) in program.compositions().iter().enumerate() {
        for binding in composition.port_bindings() {
            if needs_separator {
                out.push(',');
            }
            push_component_authority_edge_json(
                out,
                program,
                composition_index,
                composition.debug_name().as_str(),
                composition.component_instances(),
                binding,
            );
            needs_separator = true;
        }
    }
}

fn push_component_authority_edge_text(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    binding: &CheckedPortBinding,
) {
    let importer = &instances[binding.importer().index()];
    let exporter = &instances[binding.exporter().index()];
    let protocol = port_protocol(program, binding.imported_port());
    let _ = write!(
        out,
        "exporter_component={} {} -> importer_component={} {} exported_port={} {} imported_port={} {} protocol={} {} export_authority=",
        exporter.component().as_u32(),
        checked_component_label(program, exporter.component()),
        importer.component().as_u32(),
        checked_component_label(program, importer.component()),
        binding.exported_port().as_u32(),
        checked_port_label(program, binding.exported_port()),
        binding.imported_port().as_u32(),
        checked_port_label(program, binding.imported_port()),
        protocol.as_u32(),
        checked_protocol_label(program, protocol),
    );
    push_checked_descriptor_text(
        out,
        program,
        checked_component_authority(program, exporter.component()),
    );
    out.push_str(" exported_port_authority=");
    push_checked_descriptor_text(
        out,
        program,
        checked_port_authority(program, binding.exported_port()),
    );
    out.push_str(" imported_port_authority=");
    push_checked_descriptor_text(
        out,
        program,
        checked_port_authority(program, binding.imported_port()),
    );
}

fn push_component_authority_edge_json(
    out: &mut String,
    program: &CheckedProgram,
    composition_index: usize,
    composition_name: &str,
    instances: &[CheckedComponentInstance],
    binding: &CheckedPortBinding,
) {
    let importer = &instances[binding.importer().index()];
    let exporter = &instances[binding.exporter().index()];
    let protocol = port_protocol(program, binding.imported_port());
    out.push('{');
    out.push_str("\"composition_id\":");
    let _ = write!(out, "{composition_index}");
    out.push(',');
    push_json_field(out, "composition", composition_name);
    out.push_str(",\"port_binding_id\":");
    let _ = write!(out, "{}", binding.id().as_u32());
    out.push(',');
    push_json_field(out, "edge_kind", "component_port_binding");
    push_component_ref_json(out, "exporter_component", program, exporter.component());
    push_component_ref_json(out, "importer_component", program, importer.component());
    push_port_ref_json(out, "exported_port", program, binding.exported_port());
    push_port_ref_json(out, "imported_port", program, binding.imported_port());
    push_protocol_ref_json(out, program, protocol);
    out.push_str(",\"export_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_component_authority(program, exporter.component()),
    );
    out.push_str(",\"exported_port_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_port_authority(program, binding.exported_port()),
    );
    out.push_str(",\"imported_port_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_port_authority(program, binding.imported_port()),
    );
    out.push('}');
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

fn push_checked_descriptor_json(
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

fn push_component_ref_json(
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

fn push_port_ref_json(out: &mut String, prefix: &str, program: &CheckedProgram, id: CheckedPortId) {
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("\":");
    push_json_string(out, checked_port_label(program, id));
}

fn push_protocol_ref_json(out: &mut String, program: &CheckedProgram, id: CheckedProtocolId) {
    out.push_str(",\"protocol_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"protocol\":");
    push_json_string(out, checked_protocol_label(program, id));
}

fn port_protocol(program: &CheckedProgram, id: CheckedPortId) -> CheckedProtocolId {
    program.ports()[id.index()].protocol()
}

fn checked_component_authority(
    program: &CheckedProgram,
    id: CheckedComponentId,
) -> CheckedCapabilityDescriptor {
    program.components()[id.index()].required_authority()
}

fn checked_port_authority(
    program: &CheckedProgram,
    id: CheckedPortId,
) -> CheckedCapabilityDescriptor {
    program.ports()[id.index()].required_authority()
}

fn checked_process_label(program: &CheckedProgram, id: super::checked::CheckedProcessId) -> &str {
    program
        .processes()
        .get(id.index())
        .map(|process| process.debug_name().as_str())
        .unwrap_or("<invalid>")
}

fn checked_protocol_label(program: &CheckedProgram, id: CheckedProtocolId) -> &str {
    program
        .protocols()
        .get(id.index())
        .map(|protocol| protocol.debug_name().as_str())
        .unwrap_or("<invalid>")
}

fn checked_port_label(program: &CheckedProgram, id: CheckedPortId) -> &str {
    program
        .ports()
        .get(id.index())
        .map(|port| port.debug_name().as_str())
        .unwrap_or("<invalid>")
}

fn checked_component_label(program: &CheckedProgram, id: CheckedComponentId) -> &str {
    program
        .components()
        .get(id.index())
        .map(|component| component.debug_name().as_str())
        .unwrap_or("<invalid>")
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
