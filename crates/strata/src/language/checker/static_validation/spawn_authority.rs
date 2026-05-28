use crate::language::checked::{
    CheckedCapabilityDescriptor, CheckedProcess, CheckedProcessId, CheckedSpawnKind,
    CheckedSpawnSiteId,
};
use crate::language::diagnostic::{Error, Result};

pub(super) fn validate_spawn_site(
    process: &CheckedProcess,
    spawn_site: CheckedSpawnSiteId,
    target: CheckedProcessId,
) -> Result<()> {
    let site = process
        .spawn_sites()
        .get(spawn_site.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} references undefined spawn site id {}",
                process.debug_name(),
                spawn_site.as_u32()
            ))
        })?;
    if site.target() != target {
        return Err(Error::new(format!(
            "process {} spawn site id {} targets process id {}, expected {}",
            process.debug_name(),
            spawn_site.as_u32(),
            site.target().as_u32(),
            target.as_u32()
        )));
    }
    if site.kind() != CheckedSpawnKind::DynamicLocal {
        return Err(Error::new(format!(
            "process {} spawn site id {} is not a dynamic local spawn site",
            process.debug_name(),
            spawn_site.as_u32()
        )));
    }
    let authority_id = site.authority().ok_or_else(|| {
        Error::new(format!(
            "process {} dynamic spawn site id {} does not reference an authority",
            process.debug_name(),
            spawn_site.as_u32()
        ))
    })?;
    let authority = process
        .authorities()
        .get(authority_id.index())
        .ok_or_else(|| {
            Error::new(format!(
                "process {} spawn site id {} references undefined authority id {}",
                process.debug_name(),
                spawn_site.as_u32(),
                authority_id.as_u32()
            ))
        })?;
    match authority.descriptor() {
        CheckedCapabilityDescriptor::Spawn {
            target: authority_target,
        } if authority_target == target => Ok(()),
        CheckedCapabilityDescriptor::Spawn {
            target: authority_target,
        } => Err(Error::new(format!(
            "process {} spawn site id {} authority targets process id {}, expected {}",
            process.debug_name(),
            spawn_site.as_u32(),
            authority_target.as_u32(),
            target.as_u32()
        ))),
    }
}
