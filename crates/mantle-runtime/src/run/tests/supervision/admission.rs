use crate::program::{
    LoadedSupervisorChild, LoadedSupervisorChildMode, LoadedSupervisorPlan,
    LoadedSupervisorRestartIntensity, LoadedSupervisorStrategy,
};

use super::*;

#[test]
fn loaded_admission_rejects_indirect_supervisor_cycle() {
    let artifact = nested_supervisor_artifact();
    let mut program =
        LoadedProgram::from_artifact(&artifact).expect("acyclic supervisor artifact should load");
    program.processes[2].spawn_sites = vec![LoadedSpawnSite {
        target: WORKER_PROCESS,
        authority: None,
        supervisor: Some(SupervisorId::new(0)),
        child: Some(SupervisorChildId::new(0)),
        kind: LoadedSpawnKind::LexicalSupervisorChild,
    }];
    program.processes[2].supervisor_plans = vec![LoadedSupervisorPlan {
        strategy: LoadedSupervisorStrategy::OneForOne,
        intensity: LoadedSupervisorRestartIntensity {
            max_restarts: 2,
            within_ms: 1000,
        },
        children: vec![LoadedSupervisorChild {
            debug_name: "worker".to_string(),
            target: WORKER_PROCESS,
            mode: LoadedSupervisorChildMode::Permanent,
            spawn_site: SPAWN_SITE,
        }],
    }];

    let err = loaded_admission_error_before_artifact_loaded(&program);

    assert!(
        err.contains("loaded local supervisor graph contains cycle Worker -> Helper -> Worker"),
        "{err}"
    );
}

#[test]
fn loaded_admission_rejects_lexical_supervisor_child_without_supervisor_id() {
    assert_loaded_supervisor_child_spawn_site_rejected(
        |site| site.supervisor = None,
        "lexical supervisor child spawn site 0 must carry supervisor and child ids",
    );
}

#[test]
fn loaded_admission_rejects_lexical_supervisor_child_without_child_id() {
    assert_loaded_supervisor_child_spawn_site_rejected(
        |site| site.child = None,
        "lexical supervisor child spawn site 0 must carry supervisor and child ids",
    );
}

#[test]
fn loaded_admission_rejects_supervisor_child_referencing_dynamic_spawn_site() {
    let artifact = supervisor_artifact(2, 1_000);
    let mut program = LoadedProgram::from_artifact(&artifact)
        .expect("lexical supervisor artifact should load before mutation");
    program.processes[0].authorities = vec![LoadedAuthority {
        debug_name: "spawn_worker".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: WORKER_PROCESS,
        },
    }];
    program.processes[0].spawn_sites[0] = LoadedSpawnSite {
        target: WORKER_PROCESS,
        authority: Some(SPAWN_AUTHORITY),
        supervisor: None,
        child: None,
        kind: LoadedSpawnKind::DynamicLocal,
    };

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "loaded supervisor child worker references non-lexical spawn site id 0",
    );
}

#[test]
fn loaded_admission_rejects_supervisor_child_with_wrong_supervisor_id() {
    assert_loaded_supervisor_child_spawn_site_rejected(
        |site| site.supervisor = Some(SupervisorId::new(1)),
        "loaded supervisor child worker spawn site references wrong supervisor id",
    );
}

#[test]
fn loaded_admission_rejects_supervisor_child_with_wrong_child_id() {
    assert_loaded_supervisor_child_spawn_site_rejected(
        |site| site.child = Some(SupervisorChildId::new(1)),
        "loaded supervisor child worker spawn site references wrong child id",
    );
}

fn assert_loaded_supervisor_child_spawn_site_rejected(
    mutate: impl FnOnce(&mut LoadedSpawnSite),
    expected: &str,
) {
    let artifact = supervisor_artifact(2, 1_000);
    let mut program = LoadedProgram::from_artifact(&artifact)
        .expect("lexical supervisor artifact should load before mutation");
    mutate(&mut program.processes[0].spawn_sites[0]);

    assert_loaded_admission_rejects_before_artifact_loaded(&program, expected);
}
