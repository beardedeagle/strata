use std::collections::{BTreeMap, BTreeSet};

use mantle_artifact::MAX_AUTHORITIES_PER_PROCESS;

use super::super::ast::{Identifier, Module, Process, TypeRef};
use super::super::checked::{
    CheckedAuthority, CheckedAuthorityId, CheckedCapabilityDescriptor, CheckedProcessId,
    CheckedSpawnKind, CheckedSpawnSite, CheckedSpawnSiteId,
};
use super::super::diagnostic::{Error, Result};
use super::super::{CAP_TYPE, SPAWN_TYPE};
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
    pub(in crate::language::checker) fn push(
        &mut self,
        target: CheckedProcessId,
        authority: CheckedAuthorityId,
    ) -> Result<CheckedSpawnSiteId> {
        let id = CheckedSpawnSiteId::from_index(self.sites.len())?;
        self.sites.push(CheckedSpawnSite::new(
            target,
            authority,
            CheckedSpawnKind::DynamicLocal,
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
                "process {} declares duplicate spawn authority descriptor",
                process.name
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
) -> Result<()> {
    for (authority_index, authority) in authorities.iter().enumerate() {
        if !spawn_sites
            .iter()
            .any(|site| site.authority().index() == authority_index)
        {
            return Err(Error::new(format!(
                "process {} declares unused spawn authority {}",
                process.name,
                authority.debug_name()
            )));
        }
    }
    Ok(())
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
            "process {} authority type must be Cap<Spawn<ProcessName>>, got {ty}",
            process.name
        )));
    };
    if constructor.as_str() != CAP_TYPE || !const_args.is_empty() || args.len() != 1 {
        return Err(Error::new(format!(
            "process {} authority type must be Cap<Spawn<ProcessName>>, got {ty}",
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
            "process {} authority descriptor must be Spawn<ProcessName>, got {}",
            process.name, args[0]
        )));
    };
    if constructor.as_str() != SPAWN_TYPE || !const_args.is_empty() || args.len() != 1 {
        return Err(Error::new(format!(
            "process {} authority descriptor must be Spawn<ProcessName>, got {}",
            process.name, args[0]
        )));
    }
    let TypeRef::Named(target_name) = &args[0] else {
        return Err(Error::new(format!(
            "process {} spawn authority target must be a process name, got {}",
            process.name, args[0]
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
