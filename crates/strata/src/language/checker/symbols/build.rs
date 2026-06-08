use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::{ArtifactScalarType, MAX_PORT_COUNT};

use super::boundaries::{
    port_id_from_map, process_id_from_map, protocol_id_from_map, reject_boundary_name_conflict,
    reject_duplicate_boundary_name, same_type_with_symbols, validate_boundary_authority,
    validate_protocol_message_type,
};
use super::builtins::is_builtin_value_constructor_name;
use super::type_decls::{TypeDecl, TypeDeclMap};
use super::type_validation::{
    BuiltinTypeSymbols, MessagePayloadTypeContext, SourceValueTypeContext,
    reject_internal_type_label_prefix, reject_reserved_type_name,
    reject_reserved_type_name_literal, validate_message_payload_type, validate_record_fields,
};
use super::{ComponentContract, PortContract, SemanticIndex, SymbolTable};
use crate::language::ast::Module;
use crate::language::checked::{
    CheckedComponentId, CheckedPortId, CheckedProcessId, CheckedProtocolId,
};
use crate::language::diagnostic::{Error, Result};
use crate::language::{
    BOOL_TYPE, CAP_TYPE, COMPONENT_EXPORT_TYPE, LIST_TYPE, MAP_TYPE, OPTION_TYPE,
    PORT_CONNECT_TYPE, PROC_RESULT_TYPE, PROCESS_REF_TYPE, PROTOCOL_BOUNDARY_TYPE, RESULT_TYPE,
    SEND_ERROR_TYPE, SPAWN_ERROR_TYPE, SPAWN_TYPE, UNIT_TYPE,
};

impl SemanticIndex {
    pub(crate) fn build(module: &Module) -> Result<Self> {
        let mut symbols = SymbolTable::default();
        let mut types = TypeDeclMap::default();
        let mut records = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut enum_variants = Vec::with_capacity(module.enums.len());
        let mut processes = BTreeMap::new();
        let mut protocols = BTreeMap::new();
        let mut ports = BTreeMap::new();
        let mut components = BTreeMap::new();
        let mut boundary_names = BTreeSet::new();
        let mut protocol_message_types = Vec::with_capacity(module.protocols.len());
        let mut port_contracts = Vec::with_capacity(module.ports.len());
        let mut component_contracts = Vec::with_capacity(module.components.len());

        let _module_symbol = symbols.intern(&module.name)?;
        let proc_result_type = symbols.intern_str(PROC_RESULT_TYPE)?;
        let process_ref_type = symbols.intern_str(PROCESS_REF_TYPE)?;
        let list_type = symbols.intern_str(LIST_TYPE)?;
        let map_type = symbols.intern_str(MAP_TYPE)?;
        let unit_type = symbols.intern_str(UNIT_TYPE)?;
        let option_type = symbols.intern_str(OPTION_TYPE)?;
        let result_type = symbols.intern_str(RESULT_TYPE)?;
        let send_error_type = symbols.intern_str(SEND_ERROR_TYPE)?;
        let spawn_error_type = symbols.intern_str(SPAWN_ERROR_TYPE)?;
        types.insert(unit_type, TypeDecl::Unit);
        let mut scalar_type_symbols = Vec::with_capacity(ArtifactScalarType::ALL.len());
        for scalar in ArtifactScalarType::ALL {
            let symbol = symbols.intern_str(scalar.source_name())?;
            types.insert(symbol, TypeDecl::Scalar(scalar));
            scalar_type_symbols.push(symbol);
        }
        let builtin_types = BuiltinTypeSymbols {
            option: option_type,
            result: result_type,
            send_error: send_error_type,
            spawn_error: spawn_error_type,
        };

        for (index, record) in module.records.iter().enumerate() {
            let symbol = symbols.intern(&record.name)?;
            reject_reserved_type_name(record.name.as_str(), symbol, proc_result_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, process_ref_type)?;
            reject_reserved_type_name_literal(record.name.as_str(), CAP_TYPE)?;
            reject_reserved_type_name_literal(record.name.as_str(), SPAWN_TYPE)?;
            reject_reserved_type_name_literal(record.name.as_str(), PROTOCOL_BOUNDARY_TYPE)?;
            reject_reserved_type_name_literal(record.name.as_str(), PORT_CONNECT_TYPE)?;
            reject_reserved_type_name_literal(record.name.as_str(), COMPONENT_EXPORT_TYPE)?;
            reject_reserved_type_name(record.name.as_str(), symbol, list_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, map_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, unit_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, option_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, result_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, send_error_type)?;
            reject_reserved_type_name(record.name.as_str(), symbol, spawn_error_type)?;
            for scalar_symbol in &scalar_type_symbols {
                reject_reserved_type_name(record.name.as_str(), symbol, *scalar_symbol)?;
            }
            reject_internal_type_label_prefix(record.name.as_str())?;
            if records.insert(symbol, index).is_some() {
                return Err(Error::new(format!(
                    "duplicate record declaration {}",
                    record.name
                )));
            }
            if let Some(previous) = types.insert(symbol, TypeDecl::Record(index)) {
                return Err(Error::new(format!(
                    "duplicate type declaration {} used by {} and record",
                    record.name,
                    previous.kind()
                )));
            }
            for field in &record.fields {
                symbols.intern(&field.name)?;
            }
        }

        for (index, item) in module.enums.iter().enumerate() {
            let symbol = symbols.intern(&item.name)?;
            reject_reserved_type_name(item.name.as_str(), symbol, proc_result_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, process_ref_type)?;
            reject_reserved_type_name_literal(item.name.as_str(), CAP_TYPE)?;
            reject_reserved_type_name_literal(item.name.as_str(), SPAWN_TYPE)?;
            reject_reserved_type_name_literal(item.name.as_str(), PROTOCOL_BOUNDARY_TYPE)?;
            reject_reserved_type_name_literal(item.name.as_str(), PORT_CONNECT_TYPE)?;
            reject_reserved_type_name_literal(item.name.as_str(), COMPONENT_EXPORT_TYPE)?;
            reject_reserved_type_name(item.name.as_str(), symbol, list_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, map_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, unit_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, option_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, result_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, send_error_type)?;
            reject_reserved_type_name(item.name.as_str(), symbol, spawn_error_type)?;
            for scalar_symbol in &scalar_type_symbols {
                reject_reserved_type_name(item.name.as_str(), symbol, *scalar_symbol)?;
            }
            reject_internal_type_label_prefix(item.name.as_str())?;
            if enums.insert(symbol, index).is_some() {
                return Err(Error::new(format!(
                    "duplicate enum declaration {}",
                    item.name
                )));
            }
            if let Some(previous) = types.insert(symbol, TypeDecl::Enum(index)) {
                return Err(Error::new(format!(
                    "duplicate type declaration {} used by {} and enum",
                    item.name,
                    previous.kind()
                )));
            }

            let mut variants = BTreeMap::new();
            let is_core_bool_enum = index == 0 && item.name.as_str() == BOOL_TYPE;
            for (variant_index, variant) in item.variants.iter().enumerate() {
                if !is_core_bool_enum && is_builtin_value_constructor_name(variant.name.as_str()) {
                    return Err(Error::new(format!(
                        "enum {} variant {} uses reserved builtin value constructor name",
                        item.name, variant.name
                    )));
                }
                let variant_symbol = symbols.intern(&variant.name)?;
                if variants.insert(variant_symbol, variant_index).is_some() {
                    return Err(Error::new(format!(
                        "duplicate variant in enum {} declaration {}",
                        item.name, variant.name
                    )));
                }
            }
            enum_variants.push(variants);
        }

        for (index, process) in module.processes.iter().enumerate() {
            let symbol = symbols.intern(&process.name)?;
            if processes
                .insert(symbol, CheckedProcessId::from_index(index)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate process declaration {}",
                    process.name
                )));
            }
        }

        for (index, protocol) in module.protocols.iter().enumerate() {
            let symbol = symbols.intern(&protocol.name)?;
            reject_boundary_name_conflict(&protocol.name, symbol, &types, &processes)?;
            reject_duplicate_boundary_name(&mut boundary_names, symbol, &protocol.name)?;
            if protocols
                .insert(symbol, CheckedProtocolId::from_index(index)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate protocol declaration {}",
                    protocol.name
                )));
            }
            validate_protocol_message_type(&symbols, &types, protocol)?;
            validate_boundary_authority(
                &protocol.authority,
                PROTOCOL_BOUNDARY_TYPE,
                &protocol.name,
                "protocol",
            )?;
            protocol_message_types.push(protocol.message_type.clone());
        }

        for (index, port) in module.ports.iter().enumerate() {
            let symbol = symbols.intern(&port.name)?;
            reject_boundary_name_conflict(&port.name, symbol, &types, &processes)?;
            reject_duplicate_boundary_name(&mut boundary_names, symbol, &port.name)?;
            if ports
                .insert(symbol, CheckedPortId::from_index(index)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate port declaration {}",
                    port.name
                )));
            }
            let protocol = protocol_id_from_map(&symbols, &protocols, &port.protocol)?;
            let target_process = process_id_from_map(&symbols, &processes, &port.target)?;
            validate_boundary_authority(&port.authority, PORT_CONNECT_TYPE, &port.name, "port")?;
            let message_type = protocol_message_types
                .get(protocol.index())
                .ok_or_else(|| Error::new("protocol message table is inconsistent"))?
                .clone();
            let target = module
                .processes
                .get(target_process.index())
                .ok_or_else(|| Error::new("port target process table is inconsistent"))?;
            if !same_type_with_symbols(&symbols, &target.msg_type, &message_type) {
                return Err(Error::new(format!(
                    "port {} targets process {} with message type {}, expected protocol {} message type {}",
                    port.name, port.target, target.msg_type, port.protocol, message_type
                )));
            }
            port_contracts.push(PortContract {
                protocol,
                target_process,
                message_type,
            });
        }

        for (index, component) in module.components.iter().enumerate() {
            let symbol = symbols.intern(&component.name)?;
            reject_boundary_name_conflict(&component.name, symbol, &types, &processes)?;
            reject_duplicate_boundary_name(&mut boundary_names, symbol, &component.name)?;
            if components
                .insert(symbol, CheckedComponentId::from_index(index)?)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate component declaration {}",
                    component.name
                )));
            }
            let export_port = port_id_from_map(&symbols, &ports, &component.export)?;
            super::super::validate_count(
                "component_import_count",
                component.imports.len(),
                0,
                MAX_PORT_COUNT,
            )?;
            let mut import_ports = Vec::with_capacity(component.imports.len());
            let mut seen_import_ports = BTreeSet::new();
            for imported_port in &component.imports {
                let imported_port_id = port_id_from_map(&symbols, &ports, imported_port)?;
                if imported_port_id == export_port {
                    return Err(Error::new(format!(
                        "component {} cannot import its exported port {}",
                        component.name, imported_port
                    )));
                }
                if !seen_import_ports.insert(imported_port_id) {
                    return Err(Error::new(format!(
                        "component {} imports port {} more than once",
                        component.name, imported_port
                    )));
                }
                import_ports.push(imported_port_id);
            }
            validate_boundary_authority(
                &component.authority,
                COMPONENT_EXPORT_TYPE,
                &component.name,
                "component",
            )?;
            component_contracts.push(ComponentContract {
                export_port,
                import_ports,
            });
        }

        for item in &module.enums {
            for variant in &item.variants {
                if let Some(payload_type) = &variant.payload_type {
                    validate_message_payload_type(
                        MessagePayloadTypeContext {
                            module,
                            symbols: &symbols,
                            types: &types,
                            processes: &processes,
                            process_ref_type,
                            list_type,
                            map_type,
                            builtin_types,
                        },
                        item,
                        variant,
                        payload_type,
                    )?;
                }
            }
        }

        for record in &module.records {
            validate_record_fields(
                SourceValueTypeContext {
                    module,
                    symbols: &symbols,
                    types: &types,
                    process_ref_type,
                    list_type,
                    map_type,
                    builtin_types,
                },
                record,
            )?;
        }

        Ok(Self {
            symbols,
            proc_result_type,
            process_ref_type,
            list_type,
            map_type,
            unit_type,
            option_type,
            result_type,
            send_error_type,
            spawn_error_type,
            types,
            processes,
            protocols,
            ports,
            components,
            port_contracts,
            component_contracts,
            enum_variants,
        })
    }
}
