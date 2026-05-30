use std::fmt::Write as _;

use super::checked::{
    CheckedCapabilityDescriptor, CheckedComponentId, CheckedComponentInstance,
    CheckedComponentInstanceId, CheckedPortBinding, CheckedPortId, CheckedProgram,
};
use super::diagnostic::Result;
use super::source_program::{SourceProgram, SourceProvenanceHash, check_source_program};

const REPORT_FORMAT: &str = "strata.component_composition_admission_report";
const REPORT_VERSION: u32 = 1;
const SOURCE_LANGUAGE: &str = "strata";
const SOURCE_HASH_ALGORITHM: &str = "fnv1a64-diagnostic";
const ADMISSION_RESULT_ADMITTED: &str = "admitted";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionAdmissionReportFormat {
    Text,
    Json,
}

pub struct CompositionAdmissionReport {
    checked: CheckedProgram,
    source_hash: SourceProvenanceHash,
}

impl CompositionAdmissionReport {
    pub fn from_source_program(program: SourceProgram) -> Result<Self> {
        let source_hash = program.source_provenance_hash();
        let checked = check_source_program(program)?;
        Ok(Self::from_checked_parts(checked, source_hash))
    }

    pub(crate) fn from_checked_parts(
        checked: CheckedProgram,
        source_hash: SourceProvenanceHash,
    ) -> Self {
        Self {
            checked,
            source_hash,
        }
    }

    pub fn checked_program(&self) -> &CheckedProgram {
        &self.checked
    }

    pub fn source_hash(&self) -> &SourceProvenanceHash {
        &self.source_hash
    }
}

pub fn render_composition_admission_report(
    report: &CompositionAdmissionReport,
    source_path: &str,
    format: CompositionAdmissionReportFormat,
) -> String {
    match format {
        CompositionAdmissionReportFormat::Text => {
            render_text(report.checked_program(), source_path, report.source_hash())
        }
        CompositionAdmissionReportFormat::Json => {
            render_json(report.checked_program(), source_path, report.source_hash())
        }
    }
}

fn render_text(
    program: &CheckedProgram,
    source_path: &str,
    source_hash: &SourceProvenanceHash,
) -> String {
    let mut out = String::new();
    out.push_str("strata composition admission report ");
    push_escaped_text_metadata(&mut out, source_path);
    out.push('\n');
    out.push_str("format: ");
    out.push_str(REPORT_FORMAT);
    out.push('\n');
    let _ = writeln!(out, "version: {REPORT_VERSION}");
    out.push_str("source_language: ");
    out.push_str(SOURCE_LANGUAGE);
    out.push('\n');
    out.push_str("module: ");
    out.push_str(program.module_name());
    out.push('\n');
    out.push_str("source_hash_fnv1a64: ");
    out.push_str(source_hash.fnv1a64());
    out.push('\n');
    out.push_str("source_hash_algorithm: ");
    out.push_str(SOURCE_HASH_ALGORITHM);
    out.push('\n');

    for (composition_index, composition) in program.compositions().iter().enumerate() {
        let _ = writeln!(
            out,
            "composition {composition_index} {}",
            composition.debug_name()
        );
        out.push_str("  admission_result: ");
        out.push_str(ADMISSION_RESULT_ADMITTED);
        out.push('\n');
        out.push_str("  unsatisfied_imports: []\n");
        for (instance_index, instance) in composition.component_instances().iter().enumerate() {
            let component = instance.component();
            let _ = write!(
                out,
                "  instance {instance_index} {} component={} {} component_authority=",
                instance.debug_name(),
                component.as_u32(),
                checked_component_label(program, component)
            );
            push_checked_descriptor_text(
                &mut out,
                program,
                checked_component_authority(program, component),
            );
            out.push('\n');
        }
        for binding in composition.port_bindings() {
            push_binding_text(
                &mut out,
                program,
                composition.component_instances(),
                binding,
            );
        }
        for binding in composition.port_bindings() {
            push_authority_edge_text(
                &mut out,
                program,
                composition.component_instances(),
                binding,
            );
        }
    }

    out
}

fn push_binding_text(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    binding: &CheckedPortBinding,
) {
    let importer = instances[binding.importer().index()].debug_name();
    let exporter = instances[binding.exporter().index()].debug_name();
    let protocol = port_protocol(program, binding.imported_port());
    let _ = write!(
        out,
        "  binding {} importer={} {} imported_port={} {} exporter={} {} exported_port={} {} protocol={} {} binding_result=admitted imported_port_authority=",
        binding.id().as_u32(),
        binding.importer().as_u32(),
        importer,
        binding.imported_port().as_u32(),
        checked_port_label(program, binding.imported_port()),
        binding.exporter().as_u32(),
        exporter,
        binding.exported_port().as_u32(),
        checked_port_label(program, binding.exported_port()),
        protocol.as_u32(),
        checked_protocol_label(program, protocol),
    );
    push_checked_descriptor_text(
        out,
        program,
        checked_port_authority(program, binding.imported_port()),
    );
    out.push_str(" exported_port_authority=");
    push_checked_descriptor_text(
        out,
        program,
        checked_port_authority(program, binding.exported_port()),
    );
    out.push('\n');
}

fn push_authority_edge_text(
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
        "  authority_edge {} exporter_component={} {} -> importer_component={} {} exported_port={} {} imported_port={} {} protocol={} {} export_authority=",
        binding.id().as_u32(),
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
    out.push('\n');
}

fn render_json(
    program: &CheckedProgram,
    source_path: &str,
    source_hash: &SourceProvenanceHash,
) -> String {
    let mut out = String::new();
    out.push('{');
    push_json_field(&mut out, "report_format", REPORT_FORMAT);
    out.push_str(",\"report_version\":");
    let _ = write!(out, "{REPORT_VERSION}");
    out.push(',');
    push_json_field(&mut out, "source_language", SOURCE_LANGUAGE);
    out.push(',');
    push_json_field(&mut out, "source", source_path);
    out.push(',');
    push_json_field(&mut out, "module", program.module_name());
    out.push(',');
    push_json_field(&mut out, "source_hash_fnv1a64", source_hash.fnv1a64());
    out.push(',');
    push_json_field(&mut out, "source_hash_algorithm", SOURCE_HASH_ALGORITHM);
    out.push_str(",\"compositions\":[");

    for (composition_index, composition) in program.compositions().iter().enumerate() {
        if composition_index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"composition_id\":");
        let _ = write!(out, "{composition_index}");
        out.push(',');
        push_json_field(&mut out, "composition", composition.debug_name().as_str());
        out.push(',');
        push_json_field(&mut out, "admission_result", ADMISSION_RESULT_ADMITTED);
        out.push_str(",\"unsatisfied_imports\":[]");
        out.push_str(",\"component_instances\":[");
        for (instance_index, instance) in composition.component_instances().iter().enumerate() {
            if instance_index > 0 {
                out.push(',');
            }
            push_instance_json(&mut out, program, instance_index, instance);
        }
        out.push_str("],\"port_bindings\":[");
        for (binding_index, binding) in composition.port_bindings().iter().enumerate() {
            if binding_index > 0 {
                out.push(',');
            }
            push_binding_json(
                &mut out,
                program,
                composition.component_instances(),
                binding,
            );
        }
        out.push_str("],\"authority_edges\":[");
        for (binding_index, binding) in composition.port_bindings().iter().enumerate() {
            if binding_index > 0 {
                out.push(',');
            }
            push_authority_edge_json(
                &mut out,
                program,
                composition.component_instances(),
                binding,
            );
        }
        out.push_str("]}");
    }

    out.push_str("]}");
    out
}

fn push_instance_json(
    out: &mut String,
    program: &CheckedProgram,
    instance_index: usize,
    instance: &CheckedComponentInstance,
) {
    out.push('{');
    out.push_str("\"component_instance_id\":");
    let _ = write!(out, "{instance_index}");
    out.push(',');
    push_json_field(out, "instance", instance.debug_name().as_str());
    out.push_str(",\"component_id\":");
    let _ = write!(out, "{}", instance.component().as_u32());
    out.push(',');
    push_json_field(
        out,
        "component",
        checked_component_label(program, instance.component()),
    );
    out.push_str(",\"component_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_component_authority(program, instance.component()),
    );
    out.push('}');
}

fn push_binding_json(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    binding: &CheckedPortBinding,
) {
    let protocol = port_protocol(program, binding.imported_port());
    out.push('{');
    out.push_str("\"port_binding_id\":");
    let _ = write!(out, "{}", binding.id().as_u32());
    push_component_instance_ref_json(out, "importer", instances, binding.importer());
    push_port_ref_json(out, "imported_port", program, binding.imported_port());
    push_component_instance_ref_json(out, "exporter", instances, binding.exporter());
    push_port_ref_json(out, "exported_port", program, binding.exported_port());
    push_protocol_ref_json(out, program, protocol);
    out.push(',');
    push_json_field(out, "binding_result", ADMISSION_RESULT_ADMITTED);
    out.push_str(",\"imported_port_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_port_authority(program, binding.imported_port()),
    );
    out.push_str(",\"exported_port_authority\":");
    push_checked_descriptor_json(
        out,
        program,
        checked_port_authority(program, binding.exported_port()),
    );
    out.push('}');
}

fn push_authority_edge_json(
    out: &mut String,
    program: &CheckedProgram,
    instances: &[CheckedComponentInstance],
    binding: &CheckedPortBinding,
) {
    let importer = &instances[binding.importer().index()];
    let exporter = &instances[binding.exporter().index()];
    let protocol = port_protocol(program, binding.imported_port());
    out.push('{');
    out.push_str("\"port_binding_id\":");
    let _ = write!(out, "{}", binding.id().as_u32());
    out.push(',');
    push_json_field(out, "edge_kind", "port_binding");
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

fn push_component_instance_ref_json(
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
    out.push_str("_instance\":\"");
    push_escaped_json_str(out, instances[id.index()].debug_name().as_str());
    out.push('"');
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
    out.push_str("\":\"");
    push_escaped_json_str(out, checked_component_label(program, id));
    out.push('"');
}

fn push_port_ref_json(out: &mut String, prefix: &str, program: &CheckedProgram, id: CheckedPortId) {
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"");
    out.push_str(prefix);
    out.push_str("\":\"");
    push_escaped_json_str(out, checked_port_label(program, id));
    out.push('"');
}

fn push_protocol_ref_json(
    out: &mut String,
    program: &CheckedProgram,
    id: super::checked::CheckedProtocolId,
) {
    out.push_str(",\"protocol_id\":");
    let _ = write!(out, "{}", id.as_u32());
    out.push_str(",\"protocol\":\"");
    push_escaped_json_str(out, checked_protocol_label(program, id));
    out.push('"');
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

fn port_protocol(program: &CheckedProgram, id: CheckedPortId) -> super::checked::CheckedProtocolId {
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
        .unwrap_or("<invalid-process>")
}

fn checked_protocol_label(program: &CheckedProgram, id: super::checked::CheckedProtocolId) -> &str {
    program
        .protocols()
        .get(id.index())
        .map(|protocol| protocol.debug_name().as_str())
        .unwrap_or("<invalid-protocol>")
}

fn checked_port_label(program: &CheckedProgram, id: CheckedPortId) -> &str {
    program
        .ports()
        .get(id.index())
        .map(|port| port.debug_name().as_str())
        .unwrap_or("<invalid-port>")
}

fn checked_component_label(program: &CheckedProgram, id: CheckedComponentId) -> &str {
    program
        .components()
        .get(id.index())
        .map(|component| component.debug_name().as_str())
        .unwrap_or("<invalid-component>")
}

fn push_json_field(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    push_escaped_json_str(out, value);
    out.push('"');
}

fn push_escaped_text_metadata(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
}

fn push_escaped_json_str(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
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
