use std::collections::BTreeSet;

use crate::artifact::{ArtifactValueShape, MantleArtifact};
use crate::{
    ArtifactCapabilityDescriptor, ComponentId, Error, MAX_COMPONENT_COUNT, MAX_PORT_COUNT,
    MAX_PROTOCOL_COUNT, MessageId, PortId, ProcessId, ProtocolId, Result, TypeId,
    validation::validate_count, validation::validate_ident_field,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactProtocol {
    pub debug_name: String,
    pub message_type: TypeId,
    pub required_authority: ArtifactCapabilityDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPort {
    pub debug_name: String,
    pub protocol: ProtocolId,
    pub target_process: ProcessId,
    pub required_authority: ArtifactCapabilityDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactComponent {
    pub debug_name: String,
    pub export_port: PortId,
    pub required_authority: ArtifactCapabilityDescriptor,
}

impl MantleArtifact {
    pub(in crate::artifact) fn validate_boundaries(&self) -> Result<()> {
        validate_count(
            "protocol_count",
            self.protocols.len(),
            0,
            MAX_PROTOCOL_COUNT,
        )?;
        validate_count("port_count", self.ports.len(), 0, MAX_PORT_COUNT)?;
        validate_count(
            "component_count",
            self.components.len(),
            0,
            MAX_COMPONENT_COUNT,
        )?;
        let mut protocol_names = BTreeSet::new();
        for (index, protocol) in self.protocols.iter().enumerate() {
            validate_ident_field(
                &format!("protocol.{index}.debug_name"),
                &protocol.debug_name,
            )?;
            if !protocol_names.insert(protocol.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate protocol debug_name {}",
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
            validate_ident_field(&format!("port.{index}.debug_name"), &port.debug_name)?;
            if !port_names.insert(port.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate port debug_name {}",
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
            validate_ident_field(
                &format!("component.{index}.debug_name"),
                &component.debug_name,
            )?;
            if !component_names.insert(component.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate component debug_name {}",
                    component.debug_name
                )));
            }
            self.ports
                .get(component.export_port.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "component {} exports undefined port id {}",
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

    pub(in crate::artifact) fn validate_send_port(
        &self,
        process: &str,
        port: PortId,
        target_process: ProcessId,
        message: MessageId,
    ) -> Result<()> {
        let contract = self.validate_port_contract(port)?;
        if contract.target_process != target_process {
            return Err(Error::new(format!(
                "process {process} sends through port id {} targeting process id {}, expected {}",
                port.as_u32(),
                target_process.as_u32(),
                contract.target_process.as_u32()
            )));
        }
        let target = self.processes.get(target_process.index()).ok_or_else(|| {
            Error::new(format!(
                "process {process} sends through port id {} to undefined process id {}",
                port.as_u32(),
                target_process.as_u32()
            ))
        })?;
        let protocol = self
            .protocols
            .get(contract.protocol.index())
            .ok_or_else(|| Error::new("port protocol table is inconsistent"))?;
        if target.message_type != protocol.message_type {
            return Err(Error::new(format!(
                "process {process} sends through port id {} to process id {} message type id {}, expected protocol message type id {}",
                port.as_u32(),
                target_process.as_u32(),
                target.message_type.as_u32(),
                protocol.message_type.as_u32()
            )));
        }
        if message.index() >= target.message_variants.len() {
            return Err(Error::new(format!(
                "process {process} sends through port id {} message id {} not accepted by process id {}",
                port.as_u32(),
                message.as_u32(),
                target_process.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_protocol_message_type(&self, message_type: TypeId) -> Result<()> {
        let entry = self.type_entry(message_type)?;
        let ArtifactValueShape::Enum { .. } = entry.value_shape()? else {
            return Err(Error::new(format!(
                "protocol message type id {} must be an enum value type",
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
                "protocol id {} required authority must be protocol_boundary for the same protocol id",
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
                "port id {} required authority must be port_connect for the same port id",
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
                "component id {} required authority must be component_export for the same component id",
                component.as_u32()
            ))),
        }
    }

    fn validate_port_contract(&self, port: PortId) -> Result<&ArtifactPort> {
        let contract = self
            .ports
            .get(port.index())
            .ok_or_else(|| Error::new(format!("port id {} is not defined", port.as_u32())))?;
        let protocol = self
            .protocols
            .get(contract.protocol.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "port {} references undefined protocol id {}",
                    contract.debug_name,
                    contract.protocol.as_u32()
                ))
            })?;
        let target = self
            .processes
            .get(contract.target_process.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "port {} targets undefined process id {}",
                    contract.debug_name,
                    contract.target_process.as_u32()
                ))
            })?;
        if target.message_type != protocol.message_type {
            return Err(Error::new(format!(
                "port {} targets process id {} message type id {}, expected protocol message type id {}",
                contract.debug_name,
                contract.target_process.as_u32(),
                target.message_type.as_u32(),
                protocol.message_type.as_u32()
            )));
        }
        Ok(contract)
    }
}
