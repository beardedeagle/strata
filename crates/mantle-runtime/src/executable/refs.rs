use mantle_artifact::{
    AuthorityId, Error, ProcessId, ProcessRefId, Result, SpawnSiteId, SupervisorChildId,
    SupervisorId, TypeId,
};

use crate::program::{LoadedProcess, LoadedSendTarget, LoadedSpawnKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableProcessRef {
    pub(crate) id: ProcessRefId,
    pub(crate) target_process: ProcessId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExecutableSpawnSite {
    pub(crate) id: SpawnSiteId,
    pub(crate) authority: AuthorityId,
    pub(crate) kind: LoadedSpawnKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutableSendTarget {
    ProcessRef(ExecutableProcessRef),
    SupervisorChild {
        supervisor: SupervisorId,
        child: SupervisorChildId,
        target_process: ProcessId,
    },
    ReceivedPayload {
        ty: TypeId,
        target_process: ProcessId,
    },
}

impl ExecutableSendTarget {
    pub(super) fn from_loaded(process: &LoadedProcess, target: &LoadedSendTarget) -> Result<Self> {
        match target {
            LoadedSendTarget::ProcessRef(process_ref) => Ok(Self::ProcessRef(
                executable_process_ref(process, *process_ref)?,
            )),
            LoadedSendTarget::SupervisorChild {
                supervisor,
                child,
                target_process,
            } => Ok(Self::SupervisorChild {
                supervisor: *supervisor,
                child: *child,
                target_process: *target_process,
            }),
            LoadedSendTarget::ReceivedPayload { ty, target_process } => Ok(Self::ReceivedPayload {
                ty: *ty,
                target_process: *target_process,
            }),
        }
    }
}

pub(super) fn executable_process_ref(
    process: &LoadedProcess,
    process_ref: ProcessRefId,
) -> Result<ExecutableProcessRef> {
    let target_process = process
        .process_refs
        .get(process_ref.index())
        .map(|process_ref| process_ref.target)
        .ok_or_else(|| {
            Error::new(format!(
                "process {} executable action references unloaded process reference id {}",
                process.debug_name,
                process_ref.as_u32()
            ))
        })?;
    Ok(ExecutableProcessRef {
        id: process_ref,
        target_process,
    })
}

pub(super) fn executable_spawn_site(
    process: &LoadedProcess,
    spawn_site: SpawnSiteId,
    target: ProcessId,
) -> Result<ExecutableSpawnSite> {
    let site = process.validate_spawn_site(spawn_site, target)?;
    let authority = site.authority.ok_or_else(|| {
        Error::new(format!(
            "process {} executable dynamic spawn site id {} has no authority id",
            process.debug_name,
            spawn_site.as_u32()
        ))
    })?;
    Ok(ExecutableSpawnSite {
        id: spawn_site,
        authority,
        kind: site.kind,
    })
}
