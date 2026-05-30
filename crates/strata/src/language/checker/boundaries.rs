use std::collections::{BTreeMap, BTreeSet};

use super::super::ast::{Composition, Identifier, Module};
use super::super::checked::{
    CheckedCapabilityDescriptor, CheckedComponent, CheckedComponentInstance,
    CheckedComponentInstanceId, CheckedComposition, CheckedPort, CheckedPortBinding,
    CheckedPortBindingId, CheckedProtocol,
};
use super::super::diagnostic::{Error, Result};
use super::symbols::SemanticIndex;
use super::types::CheckedTypeInterner;
use mantle_artifact::{
    MAX_COMPONENT_COUNT, MAX_COMPONENT_INSTANCE_COUNT, MAX_COMPOSITION_COUNT,
    MAX_PORT_BINDING_COUNT, MAX_PORT_COUNT, MAX_PROTOCOL_COUNT,
};

pub(super) struct CheckedBoundaries {
    pub(super) protocols: Vec<CheckedProtocol>,
    pub(super) ports: Vec<CheckedPort>,
    pub(super) components: Vec<CheckedComponent>,
    pub(super) compositions: Vec<CheckedComposition>,
}

pub(super) fn validate_boundary_counts(module: &Module) -> Result<()> {
    super::validate_count(
        "protocol_count",
        module.protocols.len(),
        0,
        MAX_PROTOCOL_COUNT,
    )?;
    super::validate_count("port_count", module.ports.len(), 0, MAX_PORT_COUNT)?;
    super::validate_count(
        "component_count",
        module.components.len(),
        0,
        MAX_COMPONENT_COUNT,
    )?;
    super::validate_count(
        "composition_count",
        module.compositions.len(),
        0,
        MAX_COMPOSITION_COUNT,
    )
}

pub(super) fn check_boundaries(
    module: &Module,
    semantic_index: &SemanticIndex,
    types: &mut CheckedTypeInterner<'_>,
) -> Result<CheckedBoundaries> {
    let protocols = module
        .protocols
        .iter()
        .enumerate()
        .map(|(index, protocol)| {
            let protocol_id = semantic_index.protocol_id(&protocol.name)?;
            if protocol_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "protocol {} has inconsistent checked id {} at index {index}",
                    protocol.name,
                    protocol_id.as_u32()
                )));
            }
            Ok(CheckedProtocol::new(
                protocol.name.clone(),
                types.intern(&protocol.message_type)?,
                CheckedCapabilityDescriptor::ProtocolBoundary {
                    protocol: protocol_id,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let ports = module
        .ports
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let port_id = semantic_index.port_id(&port.name)?;
            if port_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "port {} has inconsistent checked id {} at index {index}",
                    port.name,
                    port_id.as_u32()
                )));
            }
            let contract = semantic_index.port_contract(port_id)?;
            Ok(CheckedPort::new(
                port.name.clone(),
                contract.protocol,
                contract.target_process,
                CheckedCapabilityDescriptor::PortConnect { port: port_id },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let components = module
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            let component_id = semantic_index.component_id(&component.name)?;
            if component_id.index() != index {
                return Err(super::super::diagnostic::Error::new(format!(
                    "component {} has inconsistent checked id {} at index {index}",
                    component.name,
                    component_id.as_u32()
                )));
            }
            Ok(CheckedComponent::new(
                component.name.clone(),
                semantic_index.component_contract(component_id)?.export_port,
                semantic_index
                    .component_contract(component_id)?
                    .import_ports
                    .clone(),
                CheckedCapabilityDescriptor::ComponentExport {
                    component: component_id,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let compositions = check_compositions(module, semantic_index)?;

    Ok(CheckedBoundaries {
        protocols,
        ports,
        components,
        compositions,
    })
}

fn check_compositions(
    module: &Module,
    semantic_index: &SemanticIndex,
) -> Result<Vec<CheckedComposition>> {
    let mut names = BTreeSet::new();
    module
        .compositions
        .iter()
        .map(|composition| {
            if !names.insert(composition.name.as_str()) {
                return Err(Error::new(format!(
                    "duplicate composition declaration {}",
                    composition.name
                )));
            }
            check_composition(module, semantic_index, composition)
        })
        .collect()
}

fn check_composition(
    module: &Module,
    semantic_index: &SemanticIndex,
    composition: &Composition,
) -> Result<CheckedComposition> {
    super::validate_count(
        "component_instance_count",
        composition.instances.len(),
        1,
        MAX_COMPONENT_INSTANCE_COUNT,
    )?;
    super::validate_count(
        "port_binding_count",
        composition.port_bindings.len(),
        0,
        MAX_PORT_BINDING_COUNT,
    )?;

    let mut instance_names = BTreeMap::new();
    let mut instances = Vec::with_capacity(composition.instances.len());
    for (index, instance) in composition.instances.iter().enumerate() {
        if instance_names
            .insert(
                instance.name.as_str(),
                CheckedComponentInstanceId::from_index(index)?,
            )
            .is_some()
        {
            return Err(Error::new(format!(
                "composition {} declares component instance {} more than once",
                composition.name, instance.name
            )));
        }
        instances.push(CheckedComponentInstance::new(
            instance.name.clone(),
            semantic_index.component_id(&instance.component)?,
        ));
    }

    let mut seen_import_bindings = BTreeSet::new();
    let mut port_bindings = Vec::with_capacity(composition.port_bindings.len());
    for (binding_index, binding) in composition.port_bindings.iter().enumerate() {
        let importer = component_instance_id(composition, &instance_names, &binding.importer)?;
        let exporter = component_instance_id(composition, &instance_names, &binding.exporter)?;
        if importer == exporter {
            return Err(Error::new(format!(
                "composition {} cannot bind instance {} to itself",
                composition.name, binding.importer
            )));
        }
        let imported_port = semantic_index.port_id(&binding.imported_port)?;
        let exported_port = semantic_index.port_id(&binding.exported_port)?;
        validate_imported_port(
            module,
            semantic_index,
            composition,
            &instances,
            importer,
            imported_port,
        )?;
        validate_exported_port(
            module,
            semantic_index,
            composition,
            &instances,
            exporter,
            exported_port,
        )?;
        validate_port_protocol_match(
            module,
            semantic_index,
            composition,
            imported_port,
            exported_port,
        )?;
        validate_port_authority_match(module, composition, imported_port, exported_port)?;
        if !seen_import_bindings.insert((importer, imported_port)) {
            return Err(Error::new(format!(
                "composition {} binds instance {} imported port {} more than once",
                composition.name, binding.importer, binding.imported_port
            )));
        }
        port_bindings.push(CheckedPortBinding::new(
            CheckedPortBindingId::from_index(binding_index)?,
            importer,
            imported_port,
            exporter,
            exported_port,
        ));
    }

    validate_all_imports_bound(
        module,
        semantic_index,
        composition,
        &instances,
        &seen_import_bindings,
    )?;

    Ok(CheckedComposition::new(
        composition.name.clone(),
        instances,
        port_bindings,
    ))
}

fn component_instance_id(
    composition: &Composition,
    instance_names: &BTreeMap<&str, CheckedComponentInstanceId>,
    name: &Identifier,
) -> Result<CheckedComponentInstanceId> {
    instance_names.get(name.as_str()).copied().ok_or_else(|| {
        Error::new(format!(
            "composition {} references unknown component instance {}",
            composition.name, name
        ))
    })
}

fn validate_imported_port(
    module: &Module,
    semantic_index: &SemanticIndex,
    composition: &Composition,
    instances: &[CheckedComponentInstance],
    importer: CheckedComponentInstanceId,
    imported_port: super::super::checked::CheckedPortId,
) -> Result<()> {
    let instance = &instances[importer.index()];
    let contract = semantic_index.component_contract(instance.component())?;
    if contract.import_ports.contains(&imported_port) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "composition {} instance {} component {} does not import port {}",
            composition.name,
            instance.debug_name(),
            component_name(module, instance.component()),
            port_name(module, imported_port)
        )))
    }
}

fn validate_exported_port(
    module: &Module,
    semantic_index: &SemanticIndex,
    composition: &Composition,
    instances: &[CheckedComponentInstance],
    exporter: CheckedComponentInstanceId,
    exported_port: super::super::checked::CheckedPortId,
) -> Result<()> {
    let instance = &instances[exporter.index()];
    let contract = semantic_index.component_contract(instance.component())?;
    if contract.export_port == exported_port {
        Ok(())
    } else {
        Err(Error::new(format!(
            "composition {} instance {} component {} does not export port {}",
            composition.name,
            instance.debug_name(),
            component_name(module, instance.component()),
            port_name(module, exported_port)
        )))
    }
}

fn validate_port_protocol_match(
    module: &Module,
    semantic_index: &SemanticIndex,
    composition: &Composition,
    imported_port: super::super::checked::CheckedPortId,
    exported_port: super::super::checked::CheckedPortId,
) -> Result<()> {
    let imported_contract = semantic_index.port_contract(imported_port)?;
    let exported_contract = semantic_index.port_contract(exported_port)?;
    if imported_contract.protocol == exported_contract.protocol {
        Ok(())
    } else {
        Err(Error::new(format!(
            "composition {} cannot bind imported port {} to exported port {} because their protocols differ",
            composition.name,
            port_name(module, imported_port),
            port_name(module, exported_port)
        )))
    }
}

fn validate_port_authority_match(
    module: &Module,
    composition: &Composition,
    imported_port: super::super::checked::CheckedPortId,
    exported_port: super::super::checked::CheckedPortId,
) -> Result<()> {
    let imported_authority = CheckedCapabilityDescriptor::PortConnect {
        port: imported_port,
    };
    let exported_authority = CheckedCapabilityDescriptor::PortConnect {
        port: exported_port,
    };
    if imported_authority == exported_authority {
        Ok(())
    } else {
        Err(Error::new(format!(
            "composition {} cannot bind imported port {} to exported port {} because their port authorities differ",
            composition.name,
            port_name(module, imported_port),
            port_name(module, exported_port)
        )))
    }
}

fn validate_all_imports_bound(
    module: &Module,
    semantic_index: &SemanticIndex,
    composition: &Composition,
    instances: &[CheckedComponentInstance],
    seen_import_bindings: &BTreeSet<(
        CheckedComponentInstanceId,
        super::super::checked::CheckedPortId,
    )>,
) -> Result<()> {
    for (index, instance) in instances.iter().enumerate() {
        let instance_id = CheckedComponentInstanceId::from_index(index)?;
        let contract = semantic_index.component_contract(instance.component())?;
        for imported_port in &contract.import_ports {
            if seen_import_bindings.contains(&(instance_id, *imported_port)) {
                continue;
            }
            return Err(Error::new(format!(
                "composition {} instance {} component {} import port {} is not bound",
                composition.name,
                instance.debug_name(),
                component_name(module, instance.component()),
                port_name(module, *imported_port)
            )));
        }
    }
    Ok(())
}

fn component_name(module: &Module, id: super::super::checked::CheckedComponentId) -> &Identifier {
    &module.components[id.index()].name
}

fn port_name(module: &Module, id: super::super::checked::CheckedPortId) -> &Identifier {
    &module.ports[id.index()].name
}
