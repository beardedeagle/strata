use super::{
    EncodedArtifactShape, KeyLen, add_field_bytes, add_field_u32, add_field_u64, add_field_usize,
};
use crate::{ArtifactProcess, Result};

pub(super) fn add_spawn_site_bytes(
    total: &mut EncodedArtifactShape,
    prefix: KeyLen,
    process: &ArtifactProcess,
) -> Result<()> {
    add_field_usize(
        total,
        prefix.child("spawn_site_count"),
        process.spawn_sites.len(),
    )?;
    for (spawn_site_index, spawn_site) in process.spawn_sites.iter().enumerate() {
        let spawn_site_prefix = prefix.indexed_child("spawn_site", spawn_site_index);
        add_field_u32(
            total,
            spawn_site_prefix.child("target_process"),
            spawn_site.target.as_u32(),
        )?;
        add_field_bytes(
            total,
            spawn_site_prefix.child("kind"),
            spawn_site.kind.as_str(),
        )?;
        if let Some(authority) = spawn_site.authority {
            add_field_u32(
                total,
                spawn_site_prefix.child("authority"),
                authority.as_u32(),
            )?;
        }
        if let Some(supervisor) = spawn_site.supervisor {
            add_field_u32(
                total,
                spawn_site_prefix.child("supervisor"),
                supervisor.as_u32(),
            )?;
        }
        if let Some(child) = spawn_site.child {
            add_field_u32(
                total,
                spawn_site_prefix.child("supervisor_child"),
                child.as_u32(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn add_supervisor_plan_bytes(
    total: &mut EncodedArtifactShape,
    prefix: KeyLen,
    process: &ArtifactProcess,
) -> Result<()> {
    add_field_usize(
        total,
        prefix.child("supervisor_count"),
        process.supervisor_plans.len(),
    )?;
    for (supervisor_index, supervisor) in process.supervisor_plans.iter().enumerate() {
        let supervisor_prefix = prefix.indexed_child("supervisor", supervisor_index);
        add_field_bytes(
            total,
            supervisor_prefix.child("strategy"),
            supervisor.strategy.as_str(),
        )?;
        add_field_u32(
            total,
            supervisor_prefix.child("max_restarts"),
            supervisor.intensity.max_restarts,
        )?;
        add_field_u64(
            total,
            supervisor_prefix.child("within_ms"),
            supervisor.intensity.within_ms,
        )?;
        add_field_usize(
            total,
            supervisor_prefix.child("child_count"),
            supervisor.children.len(),
        )?;
        for (child_index, child) in supervisor.children.iter().enumerate() {
            let child_prefix = supervisor_prefix.indexed_child("child", child_index);
            add_field_bytes(total, child_prefix.child("debug_name"), &child.debug_name)?;
            add_field_u32(
                total,
                child_prefix.child("target_process"),
                child.target.as_u32(),
            )?;
            add_field_bytes(total, child_prefix.child("mode"), child.mode.as_str())?;
            add_field_u32(
                total,
                child_prefix.child("spawn_site"),
                child.spawn_site.as_u32(),
            )?;
        }
    }
    Ok(())
}
