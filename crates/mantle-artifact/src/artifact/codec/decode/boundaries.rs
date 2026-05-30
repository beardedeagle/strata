use super::super::super::*;
use super::capabilities::decode_capability_descriptor;
use crate::fields::ArtifactFields;

type DecodedBoundaryTables = (
    Vec<ArtifactProtocol>,
    Vec<ArtifactPort>,
    Vec<ArtifactComponent>,
    Vec<ArtifactComposition>,
);

pub(super) fn decode_boundaries(fields: &mut ArtifactFields) -> Result<DecodedBoundaryTables> {
    let protocol_count = fields.take_bounded_usize("protocol_count", 0, MAX_PROTOCOL_COUNT)?;
    let port_count = fields.take_bounded_usize("port_count", 0, MAX_PORT_COUNT)?;
    let component_count = fields.take_bounded_usize("component_count", 0, MAX_COMPONENT_COUNT)?;
    let composition_count =
        fields.take_bounded_usize("composition_count", 0, MAX_COMPOSITION_COUNT)?;

    let mut protocols = Vec::with_capacity(protocol_count);
    for index in 0..protocol_count {
        let prefix = format!("protocol.{index}");
        protocols.push(ArtifactProtocol {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            message_type: fields.take_type_id(&format!("{prefix}.message_type_id"))?,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    let mut ports = Vec::with_capacity(port_count);
    for index in 0..port_count {
        let prefix = format!("port.{index}");
        ports.push(ArtifactPort {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            protocol: fields.take_protocol_id(&format!("{prefix}.protocol"))?,
            target_process: fields.take_process_id(&format!("{prefix}.target_process"))?,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    let mut components = Vec::with_capacity(component_count);
    for index in 0..component_count {
        let prefix = format!("component.{index}");
        let import_count =
            fields.take_bounded_usize(&format!("{prefix}.import_count"), 0, MAX_PORT_COUNT)?;
        let mut import_ports = Vec::with_capacity(import_count);
        for import_index in 0..import_count {
            import_ports.push(fields.take_port_id(&format!("{prefix}.import.{import_index}"))?);
        }
        components.push(ArtifactComponent {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            export_port: fields.take_port_id(&format!("{prefix}.export_port"))?,
            import_ports,
            required_authority: decode_capability_descriptor(
                fields,
                &format!("{prefix}.required_authority"),
            )?,
        });
    }

    let mut compositions = Vec::with_capacity(composition_count);
    for index in 0..composition_count {
        let prefix = format!("composition.{index}");
        let component_instance_count = fields.take_bounded_usize(
            &format!("{prefix}.component_instance_count"),
            1,
            MAX_COMPONENT_INSTANCE_COUNT,
        )?;
        let mut component_instances = Vec::with_capacity(component_instance_count);
        for instance_index in 0..component_instance_count {
            let instance_prefix = format!("{prefix}.instance.{instance_index}");
            component_instances.push(ArtifactComponentInstance {
                debug_name: fields
                    .take_required_string(&format!("{instance_prefix}.debug_name"))?,
                component: fields.take_component_id(&format!("{instance_prefix}.component"))?,
            });
        }
        let port_binding_count = fields.take_bounded_usize(
            &format!("{prefix}.port_binding_count"),
            0,
            MAX_PORT_BINDING_COUNT,
        )?;
        let mut port_bindings = Vec::with_capacity(port_binding_count);
        for binding_index in 0..port_binding_count {
            let binding_prefix = format!("{prefix}.port_binding.{binding_index}");
            port_bindings.push(ArtifactPortBinding {
                importer: fields
                    .take_component_instance_id(&format!("{binding_prefix}.importer"))?,
                imported_port: fields.take_port_id(&format!("{binding_prefix}.imported_port"))?,
                exporter: fields
                    .take_component_instance_id(&format!("{binding_prefix}.exporter"))?,
                exported_port: fields.take_port_id(&format!("{binding_prefix}.exported_port"))?,
            });
        }
        compositions.push(ArtifactComposition {
            debug_name: fields.take_required_string(&format!("{prefix}.debug_name"))?,
            component_instances,
            port_bindings,
        });
    }

    Ok((protocols, ports, components, compositions))
}
