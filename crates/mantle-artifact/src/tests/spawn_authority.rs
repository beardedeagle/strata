use super::support::*;

#[test]
fn validate_rejects_duplicate_spawn_authority_name() {
    let mut artifact = valid_artifact();
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "spawn_worker".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(0),
        },
    });

    let err = artifact
        .validate()
        .expect_err("duplicate spawn authority name should fail");

    assert!(
        err.to_string()
            .contains("process Main duplicates authority spawn_worker"),
        "{err}"
    );
}

#[test]
fn validate_rejects_duplicate_spawn_authority_descriptor() {
    let mut artifact = valid_artifact();
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "spawn_worker_alias".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    });

    let err = artifact
        .validate()
        .expect_err("duplicate spawn authority descriptor should fail");

    assert!(
        err.to_string()
            .contains("process Main duplicates spawn authority descriptor"),
        "{err}"
    );
}

#[test]
fn validate_rejects_unused_spawn_authority_descriptor() {
    let mut artifact = valid_artifact();
    append_peer_process(&mut artifact);
    artifact.processes[0].authorities.push(ArtifactAuthority {
        debug_name: "spawn_peer".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(2),
        },
    });

    let err = artifact
        .validate()
        .expect_err("unused spawn authority descriptor should fail");

    assert!(
        err.to_string()
            .contains("process Main declares unused authority spawn_peer"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_authority_targeting_unknown_process() {
    let mut artifact = valid_artifact();
    artifact.processes[0].authorities[0].descriptor = ArtifactCapabilityDescriptor::Spawn {
        target: ProcessId::new(99),
    };

    let err = artifact
        .validate()
        .expect_err("unknown spawn authority target should fail");

    assert!(
        err.to_string()
            .contains("process Main authority spawn_worker targets undefined process id 99"),
        "{err}"
    );
}

fn append_peer_process(artifact: &mut MantleArtifact) {
    let mut peer = artifact.processes[1].clone();
    peer.debug_name = "Peer".to_string();
    peer.authorities = Vec::new();
    peer.spawn_sites = Vec::new();
    peer.process_refs = Vec::new();
    artifact.processes.push(peer);
}

#[test]
fn validate_rejects_spawn_authority_targeting_entry_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].authorities = vec![ArtifactAuthority {
        debug_name: "spawn_main".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(0),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("spawn authority targeting entry process should fail");

    assert!(
        err.to_string()
            .contains("process Worker authority spawn_main targets entry process id 0"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_authority_targeting_same_process() {
    let mut artifact = valid_artifact();
    artifact.processes[1].authorities = vec![ArtifactAuthority {
        debug_name: "spawn_self".to_string(),
        descriptor: ArtifactCapabilityDescriptor::Spawn {
            target: ProcessId::new(1),
        },
    }];

    let err = artifact
        .validate()
        .expect_err("spawn authority targeting same process should fail");

    assert!(
        err.to_string()
            .contains("process Worker authority spawn_self targets itself"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_site_referencing_unknown_authority() {
    let mut artifact = valid_artifact();
    artifact.processes[0].spawn_sites[0].authority = Some(AuthorityId::new(99));

    let err = artifact
        .validate()
        .expect_err("unknown spawn site authority should fail");

    assert!(
        err.to_string()
            .contains("process Main spawn site 0 references undefined authority id 99"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_site_target_mismatched_with_authority() {
    let mut artifact = valid_artifact();
    artifact.processes[0].spawn_sites[0].target = ProcessId::new(0);

    let err = artifact
        .validate()
        .expect_err("spawn site target must match authority descriptor");

    assert!(
        err.to_string().contains(
            "process Main spawn site 0 targets process id 0, but authority id 0 targets 1"
        ),
        "{err}"
    );
}

#[test]
fn validate_rejects_unused_spawn_site_even_when_authority_matches() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].effects = Vec::new();
    artifact.processes[0].transitions[0].actions = Vec::new();

    let err = artifact
        .validate()
        .expect_err("unused spawn site should fail even when its authority matches");

    assert!(
        err.to_string()
            .contains("process Main declares unused dynamic spawn site 0"),
        "{err}"
    );
}

#[test]
fn validate_rejects_spawn_action_referencing_unknown_spawn_site() {
    let mut artifact = valid_artifact();
    artifact.processes[0].transitions[0].actions[0] = ArtifactAction::Spawn {
        target: ProcessId::new(1),
        process_ref: ProcessRefId::new(0),
        spawn_site: SpawnSiteId::new(99),
    };

    let err = artifact
        .validate()
        .expect_err("unknown spawn action site should fail");

    assert!(
        err.to_string()
            .contains("process Main references undefined spawn site id 99"),
        "{err}"
    );
}
