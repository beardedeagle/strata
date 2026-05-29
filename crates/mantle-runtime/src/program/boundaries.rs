use std::collections::BTreeSet;

use super::*;

pub(crate) struct LoadedBoundary<'a> {
    pub(crate) port_id: PortId,
    pub(crate) port: &'a ArtifactPort,
    pub(crate) protocol_id: ProtocolId,
    pub(crate) protocol: &'a ArtifactProtocol,
}

impl LoadedProgram {
    pub(crate) fn boundary_for_port(&self, port_id: PortId) -> Result<LoadedBoundary<'_>> {
        let port = self.ports.get(port_id.index()).ok_or_else(|| {
            Error::new(format!("loaded port id {} is not loaded", port_id.as_u32()))
        })?;
        let protocol = self.protocols.get(port.protocol.index()).ok_or_else(|| {
            Error::new(format!(
                "loaded port {} references unloaded protocol id {}",
                port.debug_name,
                port.protocol.as_u32()
            ))
        })?;
        Ok(LoadedBoundary {
            port_id,
            port,
            protocol_id: port.protocol,
            protocol,
        })
    }

    pub(crate) fn validate_boundary_send(
        &self,
        process: &str,
        port_id: PortId,
        target_process: ProcessId,
        message: MessageId,
    ) -> Result<LoadedBoundary<'_>> {
        let boundary = self.boundary_for_port(port_id)?;
        if boundary.port.target_process != target_process {
            return Err(Error::new(format!(
                "process {process} sends through loaded port id {} targeting process id {}, expected {}",
                port_id.as_u32(),
                target_process.as_u32(),
                boundary.port.target_process.as_u32()
            )));
        }
        let target = self.process(target_process)?;
        if target.message_type != boundary.protocol.message_type {
            return Err(Error::new(format!(
                "process {process} sends through loaded port id {} to process id {} message type id {}, expected protocol message type id {}",
                port_id.as_u32(),
                target_process.as_u32(),
                target.message_type.as_u32(),
                boundary.protocol.message_type.as_u32()
            )));
        }
        if message.index() >= target.message_variants.len() {
            return Err(Error::new(format!(
                "process {process} sends through loaded port id {} message id {} not accepted by process id {}",
                port_id.as_u32(),
                message.as_u32(),
                target_process.as_u32()
            )));
        }
        Ok(boundary)
    }

    pub(in crate::program) fn validate_boundaries(&self) -> Result<()> {
        validate_loaded_count("protocol_count", self.protocols.len(), MAX_PROTOCOL_COUNT)?;
        validate_loaded_count("port_count", self.ports.len(), MAX_PORT_COUNT)?;
        validate_loaded_count(
            "component_count",
            self.components.len(),
            MAX_COMPONENT_COUNT,
        )?;

        let mut protocol_names = BTreeSet::new();
        for (index, protocol) in self.protocols.iter().enumerate() {
            validate_loaded_ident_field(
                &format!("protocol.{index}.debug_name"),
                &protocol.debug_name,
            )?;
            if !protocol_names.insert(protocol.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded protocol debug_name {}",
                    protocol.debug_name
                )));
            }
            Self::validate_protocol_required_authority(
                ProtocolId::from_index(index)?,
                protocol.required_authority,
            )?;
            self.validate_protocol_message_type(protocol.message_type)?;
        }

        let mut port_names = BTreeSet::new();
        for (index, port) in self.ports.iter().enumerate() {
            validate_loaded_ident_field(&format!("port.{index}.debug_name"), &port.debug_name)?;
            if !port_names.insert(port.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded port debug_name {}",
                    port.debug_name
                )));
            }
            self.validate_port_contract(PortId::from_index(index)?)?;
            Self::validate_port_required_authority(
                PortId::from_index(index)?,
                port.required_authority,
            )?;
        }

        let mut component_names = BTreeSet::new();
        for (index, component) in self.components.iter().enumerate() {
            validate_loaded_ident_field(
                &format!("component.{index}.debug_name"),
                &component.debug_name,
            )?;
            if !component_names.insert(component.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate loaded component debug_name {}",
                    component.debug_name
                )));
            }
            self.ports
                .get(component.export_port.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "loaded component {} exports undefined port id {}",
                        component.debug_name,
                        component.export_port.as_u32()
                    ))
                })?;
            Self::validate_component_required_authority(
                ComponentId::from_index(index)?,
                component.required_authority,
            )?;
        }
        Ok(())
    }

    fn validate_protocol_message_type(&self, message_type: TypeId) -> Result<()> {
        let type_entry = self.type_entry(message_type)?;
        let ArtifactValueShape::Enum { .. } = type_entry.value_shape()? else {
            return Err(Error::new(format!(
                "loaded protocol message type id {} must be an enum value type",
                message_type.as_u32()
            )));
        };
        Ok(())
    }

    fn validate_protocol_required_authority(
        protocol: ProtocolId,
        descriptor: ArtifactCapabilityDescriptor,
    ) -> Result<()> {
        match descriptor {
            ArtifactCapabilityDescriptor::ProtocolBoundary {
                protocol: authority_protocol,
            } if authority_protocol == protocol => Ok(()),
            _ => Err(Error::new(format!(
                "loaded protocol id {} required authority must be protocol_boundary for the same protocol id",
                protocol.as_u32()
            ))),
        }
    }

    fn validate_port_required_authority(
        port: PortId,
        descriptor: ArtifactCapabilityDescriptor,
    ) -> Result<()> {
        match descriptor {
            ArtifactCapabilityDescriptor::PortConnect {
                port: authority_port,
            } if authority_port == port => Ok(()),
            _ => Err(Error::new(format!(
                "loaded port id {} required authority must be port_connect for the same port id",
                port.as_u32()
            ))),
        }
    }

    fn validate_component_required_authority(
        component: ComponentId,
        descriptor: ArtifactCapabilityDescriptor,
    ) -> Result<()> {
        match descriptor {
            ArtifactCapabilityDescriptor::ComponentExport {
                component: authority_component,
            } if authority_component == component => Ok(()),
            _ => Err(Error::new(format!(
                "loaded component id {} required authority must be component_export for the same component id",
                component.as_u32()
            ))),
        }
    }

    fn validate_port_contract(&self, port_id: PortId) -> Result<()> {
        let boundary = self.boundary_for_port(port_id)?;
        let target = self.process(boundary.port.target_process)?;
        if target.message_type != boundary.protocol.message_type {
            return Err(Error::new(format!(
                "loaded port {} targets process id {} message type id {}, expected protocol message type id {}",
                boundary.port.debug_name,
                boundary.port.target_process.as_u32(),
                target.message_type.as_u32(),
                boundary.protocol.message_type.as_u32()
            )));
        }
        Ok(())
    }
}

fn validate_loaded_count(field: &str, count: usize, max: usize) -> Result<()> {
    if count > max {
        return Err(Error::new(format!(
            "loaded {field} must be no greater than {max}"
        )));
    }
    Ok(())
}
