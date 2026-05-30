use std::fmt::Write as _;

use super::checked::{CheckedComponentInstance, CheckedPortBinding, CheckedProgram};
use super::checked_render::{
    checked_component_authority, checked_component_label, checked_port_authority,
    checked_port_label, checked_protocol_label, port_protocol, push_checked_descriptor_json,
    push_checked_descriptor_text, push_component_ref_json, push_json_field, push_port_ref_json,
    push_protocol_ref_json,
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
