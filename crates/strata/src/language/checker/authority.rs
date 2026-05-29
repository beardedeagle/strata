use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::MAX_AUTHORITIES_PER_PROCESS;

use super::super::ast::{Identifier, Module, Process, TypeRef};
use super::super::checked::{
    CheckedAction, CheckedAuthority, CheckedAuthorityId, CheckedCapabilityDescriptor,
    CheckedPortId, CheckedProcessId, CheckedSpawnSite, CheckedSpawnSiteId,
    CheckedSupervisorChildId, CheckedSupervisorId, CheckedTransition,
};
use super::super::diagnostic::{Error, Result};
use super::super::{CAP_TYPE, PORT_CONNECT_TYPE, SPAWN_TYPE};
use super::symbols::SemanticIndex;
use super::validate_count;

#[derive(Debug, Clone, Copy)]
pub(in crate::language::checker) struct AuthorityBinding {
    pub(in crate::language::checker) id: CheckedAuthorityId,
    pub(in crate::language::checker) descriptor: CheckedCapabilityDescriptor,
}

#[derive(Debug, Default)]
pub(in crate::language::checker) struct SpawnSiteAllocator {
    sites: Vec<CheckedSpawnSite>,
}

impl SpawnSiteAllocator {
    pub(in crate::language::checker) fn push_dynamic_local(
        &mut self,
        target: CheckedProcessId,
        authority: CheckedAuthorityId,
    ) -> Result<CheckedSpawnSiteId> {
        let id = CheckedSpawnSiteId::from_index(self.sites.len())?;
        self.sites
            .push(CheckedSpawnSite::dynamic_local(target, authority));
        Ok(id)
    }

    pub(in crate::language::checker) fn push_lexical_supervisor_child(
        &mut self,
        target: CheckedProcessId,
        supervisor: CheckedSupervisorId,
        child: CheckedSupervisorChildId,
    ) -> Result<CheckedSpawnSiteId> {
        let id = CheckedSpawnSiteId::from_index(self.sites.len())?;
        self.sites.push(CheckedSpawnSite::lexical_supervisor_child(
            target, supervisor, child,
        ));
        Ok(id)
    }

    pub(in crate::language::checker) fn into_sites(self) -> Vec<CheckedSpawnSite> {
        self.sites
    }
}

pub(in crate::language::checker) fn validate_authority_declarations(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    entry_process: CheckedProcessId,
) -> Result<()> {
    validate_count(
        &format!("process {} authority_count", process.name),
        process.authorities.len(),
        0,
        MAX_AUTHORITIES_PER_PROCESS,
    )?;
    let mut names = BTreeSet::new();
    let mut descriptors = BTreeSet::new();
    for authority in &process.authorities {
        if !names.insert(authority.name.as_str()) {
            return Err(Error::new(format!(
                "process {} declares duplicate authority {}",
                process.name, authority.name
            )));
        }
        let descriptor = checked_capability_descriptor(
            module,
            semantic_index,
            process,
            entry_process,
            &authority.ty,
        )?;
        if !descriptors.insert(descriptor) {
            return Err(Error::new(format!(
                "process {} declares duplicate {}",
                process.name,
                authority_descriptor_label(descriptor)
            )));
        }
    }
    Ok(())
}

pub(in crate::language::checker) fn collect_authorities(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    entry_process: CheckedProcessId,
) -> Result<(
    Vec<CheckedAuthority>,
    BTreeMap<Identifier, AuthorityBinding>,
)> {
    let mut authorities = Vec::with_capacity(process.authorities.len());
    let mut index = BTreeMap::new();
    for authority in &process.authorities {
        let id = CheckedAuthorityId::from_index(authorities.len())?;
        let descriptor = checked_capability_descriptor(
            module,
            semantic_index,
            process,
            entry_process,
            &authority.ty,
        )?;
        authorities.push(CheckedAuthority::new(authority.name.clone(), descriptor));
        if index
            .insert(authority.name.clone(), AuthorityBinding { id, descriptor })
            .is_some()
        {
            return Err(Error::new(format!(
                "process {} declares duplicate authority {}",
                process.name, authority.name
            )));
        }
    }
    Ok((authorities, index))
}

pub(in crate::language::checker) fn validate_authority_usage(
    process: &Process,
    authorities: &[CheckedAuthority],
    spawn_sites: &[CheckedSpawnSite],
    transitions: &[CheckedTransition],
) -> Result<()> {
    for (authority_index, authority) in authorities.iter().enumerate() {
        let used = match authority.descriptor() {
            CheckedCapabilityDescriptor::Spawn { .. } => spawn_sites.iter().any(|site| {
                site.authority()
                    .is_some_and(|id| id.index() == authority_index)
            }),
            CheckedCapabilityDescriptor::PortConnect { port } => transitions
                .iter()
                .any(|transition| actions_use_port(transition.actions(), port)),
            CheckedCapabilityDescriptor::ProtocolBoundary { .. }
            | CheckedCapabilityDescriptor::ComponentExport { .. } => false,
        };
        if !used {
            return Err(Error::new(format!(
                "process {} declares unused {} {}",
                process.name,
                unused_authority_label(authority.descriptor()),
                authority.debug_name()
            )));
        }
    }
    Ok(())
}

fn actions_use_port(actions: &[CheckedAction], expected: CheckedPortId) -> bool {
    actions.iter().any(|action| match action {
        CheckedAction::Send { port, .. } | CheckedAction::SendOutcome { port, .. } => {
            port.is_some_and(|port| port == expected)
        }
        CheckedAction::IfElse {
            then_actions,
            else_actions,
            ..
        } => actions_use_port(then_actions, expected) || actions_use_port(else_actions, expected),
        CheckedAction::ForEach { body, .. } => actions_use_port(body, expected),
        CheckedAction::Emit { .. }
        | CheckedAction::Spawn { .. }
        | CheckedAction::SpawnOutcome { .. } => false,
    })
}

fn checked_capability_descriptor(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    entry_process: CheckedProcessId,
    ty: &TypeRef,
) -> Result<CheckedCapabilityDescriptor> {
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = ty
    else {
        return Err(Error::new(format!(
            "process {} authority type must be Cap<Spawn<ProcessName>> or Cap<PortConnect<PortName>>, got {ty}",
            process.name
        )));
    };
    if constructor.as_str() != CAP_TYPE || !const_args.is_empty() || args.len() != 1 {
        return Err(Error::new(format!(
            "process {} authority type must be Cap<Spawn<ProcessName>> or Cap<PortConnect<PortName>>, got {ty}",
            process.name
        )));
    }
    let TypeRef::Applied {
        constructor,
        args,
        const_args,
    } = &args[0]
    else {
        return Err(Error::new(format!(
            "process {} authority descriptor must be Spawn<ProcessName> or PortConnect<PortName>, got {}",
            process.name, args[0]
        )));
    };
    if !const_args.is_empty() || args.len() != 1 {
        return Err(Error::new(format!(
            "process {} authority descriptor must be Spawn<ProcessName> or PortConnect<PortName>, got {}",
            process.name, args[0]
        )));
    }
    match constructor.as_str() {
        SPAWN_TYPE => checked_spawn_capability_descriptor(
            module,
            semantic_index,
            process,
            entry_process,
            &args[0],
        ),
        PORT_CONNECT_TYPE => {
            checked_port_capability_descriptor(module, semantic_index, process, &args[0])
        }
        _ => Err(Error::new(format!(
            "process {} authority descriptor must be Spawn<ProcessName> or PortConnect<PortName>, got {}",
            process.name, args[0]
        ))),
    }
}

fn checked_spawn_capability_descriptor(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    entry_process: CheckedProcessId,
    target_ty: &TypeRef,
) -> Result<CheckedCapabilityDescriptor> {
    let TypeRef::Named(target_name) = target_ty else {
        return Err(Error::new(format!(
            "process {} spawn authority target must be a process name, got {target_ty}",
            process.name
        )));
    };
    let target = semantic_index.process_id(target_name)?;
    module.processes.get(target.index()).ok_or_else(|| {
        Error::new(format!(
            "spawn authority target {target_name} is not declared"
        ))
    })?;
    if target == semantic_index.process_id(&process.name)? {
        return Err(Error::new(format!(
            "process {} spawn authority targets itself, which is not supported",
            process.name
        )));
    }
    if target == entry_process {
        return Err(Error::new(format!(
            "process {} spawn authority targets entry process {target_name}, which is already started",
            process.name
        )));
    }
    Ok(CheckedCapabilityDescriptor::Spawn { target })
}

fn checked_port_capability_descriptor(
    module: &Module,
    semantic_index: &SemanticIndex,
    process: &Process,
    port_ty: &TypeRef,
) -> Result<CheckedCapabilityDescriptor> {
    let TypeRef::Named(port_name) = port_ty else {
        return Err(Error::new(format!(
            "process {} port authority target must be a port name, got {port_ty}",
            process.name
        )));
    };
    let port = semantic_index.port_id(port_name)?;
    module
        .ports
        .get(port.index())
        .ok_or_else(|| Error::new(format!("port authority target {port_name} is not declared")))?;
    Ok(CheckedCapabilityDescriptor::PortConnect { port })
}

fn authority_descriptor_label(descriptor: CheckedCapabilityDescriptor) -> &'static str {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { .. } => "spawn authority descriptor",
        CheckedCapabilityDescriptor::PortConnect { .. } => "port authority descriptor",
        CheckedCapabilityDescriptor::ProtocolBoundary { .. }
        | CheckedCapabilityDescriptor::ComponentExport { .. } => "authority descriptor",
    }
}

fn unused_authority_label(descriptor: CheckedCapabilityDescriptor) -> &'static str {
    match descriptor {
        CheckedCapabilityDescriptor::Spawn { .. } => "spawn authority",
        CheckedCapabilityDescriptor::PortConnect { .. } => "port authority",
        CheckedCapabilityDescriptor::ProtocolBoundary { .. }
        | CheckedCapabilityDescriptor::ComponentExport { .. } => "authority",
    }
}
