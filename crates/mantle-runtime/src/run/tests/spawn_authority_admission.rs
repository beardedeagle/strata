use super::support::*;

#[test]
fn runtime_rejects_loaded_authority_targeting_entry_process_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].authorities = vec![LoadedAuthority {
        debug_name: "spawn_main".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: ProcessId::new(0),
        },
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker loaded authority spawn_main targets entry process id 0",
    );
}

#[test]
fn runtime_rejects_loaded_authority_targeting_same_process_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].authorities = vec![LoadedAuthority {
        debug_name: "spawn_self".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker loaded authority spawn_self targets itself",
    );
}

#[test]
fn runtime_rejects_unused_loaded_authority_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    append_loaded_peer_process(&mut program);
    program.processes[0].authorities.push(LoadedAuthority {
        debug_name: "spawn_peer".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: ProcessId::new(2),
        },
    });

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main declares unused loaded authority spawn_peer",
    );
}

#[test]
fn runtime_rejects_loaded_spawn_site_referencing_unknown_authority_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].spawn_sites = vec![LoadedSpawnSite {
        target: ProcessId::new(1),
        authority: AuthorityId::new(99),
        kind: LoadedSpawnKind::DynamicLocal,
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main loaded spawn site 0 references undefined authority id 99",
    );
}

#[test]
fn runtime_rejects_loaded_spawn_site_target_mismatched_with_authority_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].authorities = vec![LoadedAuthority {
        debug_name: "spawn_worker".to_string(),
        descriptor: LoadedCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    }];
    program.processes[0].spawn_sites = vec![LoadedSpawnSite {
        target: ProcessId::new(0),
        authority: SPAWN_AUTHORITY,
        kind: LoadedSpawnKind::DynamicLocal,
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main loaded spawn site 0 targets process id 0, but authority id 0 targets 1",
    );
}

#[test]
fn runtime_rejects_unused_loaded_spawn_site_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    grant_loaded_main_spawn_authority(&mut program);
    program.processes[0].transitions[0].effect_authority =
        LoadedEffectAuthority::from_artifact(&[]);
    program.processes[0].transitions[0].actions = Vec::new();

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main declares unused loaded spawn site 0",
    );
}

#[test]
fn runtime_rejects_loaded_spawn_action_referencing_unknown_spawn_site_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Spawn]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::Spawn {
        target: ProcessId::new(1),
        process_ref: ProcessRefId::new(0),
        spawn_site: SpawnSiteId::new(99),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main references unloaded spawn site id 99",
    );
}

fn append_loaded_peer_process(program: &mut LoadedProgram) {
    let mut peer = program.processes[1].clone();
    peer.debug_name = "Peer".to_string();
    peer.authorities = Vec::new();
    peer.spawn_sites = Vec::new();
    peer.process_refs = Vec::new();
    program.processes.push(peer);
}
