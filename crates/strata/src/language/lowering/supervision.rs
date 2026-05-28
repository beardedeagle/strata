use mantle_artifact::{
    ArtifactSpawnKind, ArtifactSpawnSite, ArtifactSupervisorChild, ArtifactSupervisorChildMode,
    ArtifactSupervisorPlan, ArtifactSupervisorRestartIntensity, ArtifactSupervisorStrategy,
};

use super::{
    lower_authority_id, lower_process_id, lower_spawn_site_id, lower_supervisor_child_id,
    lower_supervisor_id,
};
use crate::language::checked::{
    CheckedProcess, CheckedSpawnKind, CheckedSpawnSite, CheckedSupervisorChildMode,
    CheckedSupervisorPlan, CheckedSupervisorStrategy,
};

pub(super) fn lower_spawn_site(spawn_site: &CheckedSpawnSite) -> ArtifactSpawnSite {
    ArtifactSpawnSite {
        target: lower_process_id(spawn_site.target()),
        authority: spawn_site.authority().map(lower_authority_id),
        supervisor: spawn_site.supervisor().map(lower_supervisor_id),
        child: spawn_site.child().map(lower_supervisor_child_id),
        kind: lower_spawn_kind(spawn_site.kind()),
    }
}

pub(super) fn lower_supervisor_plans(process: &CheckedProcess) -> Vec<ArtifactSupervisorPlan> {
    process
        .supervisor_plans()
        .iter()
        .map(lower_supervisor_plan)
        .collect()
}

fn lower_supervisor_plan(plan: &CheckedSupervisorPlan) -> ArtifactSupervisorPlan {
    ArtifactSupervisorPlan {
        strategy: lower_supervisor_strategy(plan.strategy()),
        intensity: ArtifactSupervisorRestartIntensity {
            max_restarts: plan.intensity().max_restarts(),
            within_ms: plan.intensity().within_ms(),
        },
        children: plan
            .children()
            .iter()
            .map(|child| ArtifactSupervisorChild {
                debug_name: child.debug_name().to_string(),
                target: lower_process_id(child.target()),
                mode: lower_supervisor_child_mode(child.mode()),
                spawn_site: lower_spawn_site_id(child.spawn_site()),
            })
            .collect(),
    }
}

fn lower_spawn_kind(kind: CheckedSpawnKind) -> ArtifactSpawnKind {
    match kind {
        CheckedSpawnKind::DynamicLocal => ArtifactSpawnKind::DynamicLocal,
        CheckedSpawnKind::LexicalSupervisorChild => ArtifactSpawnKind::LexicalSupervisorChild,
    }
}

fn lower_supervisor_strategy(strategy: CheckedSupervisorStrategy) -> ArtifactSupervisorStrategy {
    match strategy {
        CheckedSupervisorStrategy::OneForOne => ArtifactSupervisorStrategy::OneForOne,
    }
}

fn lower_supervisor_child_mode(mode: CheckedSupervisorChildMode) -> ArtifactSupervisorChildMode {
    match mode {
        CheckedSupervisorChildMode::Permanent => ArtifactSupervisorChildMode::Permanent,
        CheckedSupervisorChildMode::Transient => ArtifactSupervisorChildMode::Transient,
        CheckedSupervisorChildMode::Temporary => ArtifactSupervisorChildMode::Temporary,
    }
}
