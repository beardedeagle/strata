use std::collections::BTreeSet;

use crate::artifact::{ArtifactValueShape, MantleArtifact};
use crate::{
    ArtifactCapabilityDescriptor, ComponentId, ComponentInstanceId, Error, MAX_COMPONENT_COUNT,
    MAX_COMPONENT_INSTANCE_COUNT, MAX_COMPOSITION_COUNT, MAX_PORT_BINDING_COUNT, MAX_PORT_COUNT,
    MAX_PROTOCOL_COUNT, MessageId, PortBindingId, PortId, ProcessId, ProtocolId, Result, TypeId,
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
    pub import_ports: Vec<PortId>,
    pub required_authority: ArtifactCapabilityDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactComposition {
    pub debug_name: String,
    pub component_instances: Vec<ArtifactComponentInstance>,
    pub port_bindings: Vec<ArtifactPortBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactComponentInstance {
    pub debug_name: String,
    pub component: ComponentId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPortBinding {
    pub importer: ComponentInstanceId,
    pub imported_port: PortId,
    pub exporter: ComponentInstanceId,
    pub exported_port: PortId,
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
            Self::validate_component_import_count(index, component.import_ports.len())?;
            let mut import_ports = BTreeSet::new();
            for imported_port in &component.import_ports {
                self.ports.get(imported_port.index()).ok_or_else(|| {
                    Error::new(format!(
                        "component {} imports undefined port id {}",
                        component.debug_name,
                        imported_port.as_u32()
                    ))
                })?;
                if *imported_port == component.export_port {
                    return Err(Error::new(format!(
                        "component {} cannot import its exported port id {}",
                        component.debug_name,
                        imported_port.as_u32()
                    )));
                }
                if !import_ports.insert(*imported_port) {
                    return Err(Error::new(format!(
                        "component {} imports port id {} more than once",
                        component.debug_name,
                        imported_port.as_u32()
                    )));
                }
            }
            Self::validate_component_required_authority(
                ComponentId::from_index(index)?,
                component.required_authority,
            )?;
        }
        self.validate_compositions()?;
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

    fn validate_component_import_count(component_index: usize, count: usize) -> Result<()> {
        if count > MAX_PORT_COUNT {
            return Err(Error::new(format!(
                "component.{component_index}.import_count must be no greater than {MAX_PORT_COUNT}"
            )));
        }
        Ok(())
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

    fn validate_compositions(&self) -> Result<()> {
        validate_count(
            "composition_count",
            self.compositions.len(),
            0,
            MAX_COMPOSITION_COUNT,
        )?;
        let mut composition_names = BTreeSet::new();
        for (composition_index, composition) in self.compositions.iter().enumerate() {
            validate_ident_field(
                &format!("composition.{composition_index}.debug_name"),
                &composition.debug_name,
            )?;
            if !composition_names.insert(composition.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate composition debug_name {}",
                    composition.debug_name
                )));
            }
            self.validate_composition(composition_index, composition)?;
        }
        Ok(())
    }

    fn validate_composition(
        &self,
        composition_index: usize,
        composition: &ArtifactComposition,
    ) -> Result<()> {
        validate_count(
            &format!("composition.{composition_index}.component_instance_count"),
            composition.component_instances.len(),
            1,
            MAX_COMPONENT_INSTANCE_COUNT,
        )?;
        validate_count(
            &format!("composition.{composition_index}.port_binding_count"),
            composition.port_bindings.len(),
            0,
            MAX_PORT_BINDING_COUNT,
        )?;

        let mut instance_names = BTreeSet::new();
        for (instance_index, instance) in composition.component_instances.iter().enumerate() {
            validate_ident_field(
                &format!("composition.{composition_index}.instance.{instance_index}.debug_name"),
                &instance.debug_name,
            )?;
            if !instance_names.insert(instance.debug_name.as_str()) {
                return Err(Error::new(format!(
                    "composition {} declares component instance {} more than once",
                    composition.debug_name, instance.debug_name
                )));
            }
            self.components
                .get(instance.component.index())
                .ok_or_else(|| {
                    Error::new(format!(
                        "composition {} instance {} references undefined component id {}",
                        composition.debug_name,
                        instance.debug_name,
                        instance.component.as_u32()
                    ))
                })?;
        }

        let mut seen_import_bindings = BTreeSet::new();
        for (binding_index, binding) in composition.port_bindings.iter().enumerate() {
            let binding_id = PortBindingId::from_index(binding_index)?;
            self.validate_port_binding(
                composition,
                binding_id,
                binding,
                &mut seen_import_bindings,
            )?;
        }
        self.validate_composition_imports_satisfied(composition, &seen_import_bindings)
    }

    fn validate_port_binding(
        &self,
        composition: &ArtifactComposition,
        binding_id: PortBindingId,
        binding: &ArtifactPortBinding,
        seen_import_bindings: &mut BTreeSet<(ComponentInstanceId, PortId)>,
    ) -> Result<()> {
        let importer = composition
            .component_instances
            .get(binding.importer.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "composition {} port binding id {} references undefined importer instance id {}",
                    composition.debug_name,
                    binding_id.as_u32(),
                    binding.importer.as_u32()
                ))
            })?;
        let exporter = composition
            .component_instances
            .get(binding.exporter.index())
            .ok_or_else(|| {
                Error::new(format!(
                    "composition {} port binding id {} references undefined exporter instance id {}",
                    composition.debug_name,
                    binding_id.as_u32(),
                    binding.exporter.as_u32()
                ))
            })?;
        if binding.importer == binding.exporter {
            return Err(Error::new(format!(
                "composition {} port binding id {} binds instance {} to itself",
                composition.debug_name,
                binding_id.as_u32(),
                importer.debug_name
            )));
        }
        let importer_component = self.component(importer.component)?;
        let exporter_component = self.component(exporter.component)?;
        if !importer_component
            .import_ports
            .contains(&binding.imported_port)
        {
            return Err(Error::new(format!(
                "composition {} instance {} component {} does not import port id {}",
                composition.debug_name,
                importer.debug_name,
                importer_component.debug_name,
                binding.imported_port.as_u32()
            )));
        }
        if exporter_component.export_port != binding.exported_port {
            return Err(Error::new(format!(
                "composition {} instance {} component {} does not export port id {}",
                composition.debug_name,
                exporter.debug_name,
                exporter_component.debug_name,
                binding.exported_port.as_u32()
            )));
        }
        let imported_contract = self.validate_port_contract(binding.imported_port)?;
        let exported_contract = self.validate_port_contract(binding.exported_port)?;
        if imported_contract.protocol != exported_contract.protocol {
            return Err(Error::new(format!(
                "composition {} port binding id {} connects port ids {} and {} with different protocols",
                composition.debug_name,
                binding_id.as_u32(),
                binding.imported_port.as_u32(),
                binding.exported_port.as_u32()
            )));
        }
        if imported_contract.required_authority != exported_contract.required_authority {
            return Err(Error::new(format!(
                "composition {} port binding id {} connects port ids {} and {} with different port authorities",
                composition.debug_name,
                binding_id.as_u32(),
                binding.imported_port.as_u32(),
                binding.exported_port.as_u32()
            )));
        }
        if !seen_import_bindings.insert((binding.importer, binding.imported_port)) {
            return Err(Error::new(format!(
                "composition {} binds importer instance id {} port id {} more than once",
                composition.debug_name,
                binding.importer.as_u32(),
                binding.imported_port.as_u32()
            )));
        }
        Ok(())
    }

    fn validate_composition_imports_satisfied(
        &self,
        composition: &ArtifactComposition,
        seen_import_bindings: &BTreeSet<(ComponentInstanceId, PortId)>,
    ) -> Result<()> {
        for (instance_index, instance) in composition.component_instances.iter().enumerate() {
            let instance_id = ComponentInstanceId::from_index(instance_index)?;
            let component = self.component(instance.component)?;
            for imported_port in &component.import_ports {
                if seen_import_bindings.contains(&(instance_id, *imported_port)) {
                    continue;
                }
                return Err(Error::new(format!(
                    "composition {} instance {} component {} import port id {} is not bound",
                    composition.debug_name,
                    instance.debug_name,
                    component.debug_name,
                    imported_port.as_u32()
                )));
            }
        }
        Ok(())
    }

    fn component(&self, component: ComponentId) -> Result<&ArtifactComponent> {
        self.components.get(component.index()).ok_or_else(|| {
            Error::new(format!(
                "component id {} is not defined",
                component.as_u32()
            ))
        })
    }
}
