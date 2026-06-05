use super::super::support::*;
use super::*;
use crate::authority_effect_binding::RuntimeAuthorityEffectBinding;
use crate::event::{RuntimeAuthorityResult, RuntimeEvent};
use crate::run::run_loaded_program_with_bindings;
use mantle_artifact::{
    ArtifactAction, ArtifactAuthority, ArtifactPort, ArtifactProtocol, EffectOutcomeId, PortId,
    ProtocolId,
};

#[test]
fn runtime_boundary_send_outcome_traces_accepted_boundary_on_message_acceptance() {
    let mut artifact = send_outcome_artifact();
    attach_worker_boundary_to_send_outcome(&mut artifact);
    let mut host = InMemoryRuntimeHost::default();

    run_artifact_with_host(&artifact, &mut host, RunLimits::default())
        .expect("accepted boundary send outcome should run");

    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::BoundarySendChecked {
            port_id,
            target_process_id,
            message_id,
            boundary_result: RuntimeAuthorityResult::Accepted,
            ..
        } if *port_id == PortId::new(0)
            && *target_process_id == WORKER_PROCESS
            && *message_id == PING_MESSAGE
    )));
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::MessageAccepted {
            process_id,
            message_id,
            ..
        } if *process_id == WORKER_PROCESS && *message_id == PING_MESSAGE
    )));
}

#[test]
fn runtime_boundary_send_outcome_denied_policy_fails_closed_without_binding_outcome() {
    let mut artifact = send_outcome_artifact();
    attach_worker_boundary_to_send_outcome(&mut artifact);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let binding = RuntimeAuthorityEffectBinding::decode_for_test(
        &denied_port_send_outcome_binding_json(),
        &artifact,
    )
    .expect("denied port authority/effect binding should admit");
    let mut host = InMemoryRuntimeHost::default();

    let err = run_loaded_program_with_bindings(
        &program,
        &mut host,
        RunLimits::default(),
        None,
        binding.into_policy(),
    )
    .expect_err("denied boundary send outcome should fail closed before source binding");

    assert!(
        err.to_string().contains("boundary send authority denied"),
        "unexpected diagnostic: {err}"
    );
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::MessageAccepted {
                process_id: WORKER_PROCESS,
                message_id: PING_MESSAGE,
                ..
            }
        )),
        "denied boundary send outcome must not accept the target message"
    );
    assert!(host.events().iter().any(|event| matches!(
        event,
        RuntimeEvent::BoundarySendChecked {
            port_id,
            authority_policy_decision_id: Some(1),
            boundary_result: RuntimeAuthorityResult::Denied,
            target_process_id,
            message_id,
            ..
        } if *port_id == PortId::new(0)
            && *target_process_id == WORKER_PROCESS
            && *message_id == PING_MESSAGE
    )));
    assert!(
        !host.events().iter().any(|event| matches!(
            event,
            RuntimeEvent::EffectOutcomeBound {
                action: RuntimeEffectOutcomeAction::Send,
                ..
            }
        )),
        "denied boundary send outcome must not bind a source-visible send result"
    );
}

#[test]
fn runtime_boundary_send_outcome_returns_full_without_accepted_boundary_trace() {
    let mut artifact = send_outcome_artifact();
    attach_worker_boundary_to_send_outcome(&mut artifact);
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program
        .validate_admission()
        .expect("boundary send outcome should admit");
    let mut host = InMemoryRuntimeHost::default();

    let executable = ExecutableProgram::from_admitted(&program)
        .expect("executable plan should admit loaded program");
    let mut run = RuntimeRun::new(&program, &executable, &mut host, RunLimits::default());
    let main_pid = run
        .spawn_process(MAIN_PROCESS, None)
        .expect("main should spawn");
    let worker_pid = run
        .spawn_process(WORKER_PROCESS, Some(main_pid))
        .expect("worker should spawn");
    let worker_index = run
        .process_index_for_pid(worker_pid)
        .expect("worker pid should resolve");
    run.processes[worker_index]
        .mailbox
        .push_back(RuntimeMessageEnvelope::new(PING_MESSAGE, None));

    let mut process_refs = LocalProcessRefs::new(1);
    process_refs
        .bind(ProcessRefId::new(0), worker_pid)
        .expect("worker process ref should bind");
    let step = main_step(main_pid);
    let action = LoadedAction::SendOutcome {
        outcome: EffectOutcomeId::new(0),
        outcome_ty: SEND_RESULT,
        target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
        port: Some(PortId::new(0)),
        message: PING_MESSAGE,
        payload: None,
    };
    let mut effect_outcomes = Vec::new();

    let handled = run
        .execute_prestate_action(&mut process_refs, &step, &action, &mut effect_outcomes)
        .expect("full boundary send outcome should bind a typed result");

    assert!(handled);
    assert_eq!(effect_outcomes[0].payload.label(), "Err(Full(Ping))");
    assert_eq!(run.processes[worker_index].mailbox.len(), 1);
    assert!(run.delivered_messages.is_empty());
    assert!(
        !host
            .events()
            .iter()
            .any(|event| matches!(event, RuntimeEvent::BoundarySendChecked { .. }))
    );
}

fn attach_worker_boundary_to_send_outcome(artifact: &mut MantleArtifact) {
    artifact.protocols = vec![ArtifactProtocol {
        debug_name: "WorkerProtocol".to_string(),
        message_type: WORKER_MSG,
        required_authority: ArtifactCapabilityDescriptor::ProtocolBoundary {
            protocol: ProtocolId::new(0),
        },
    }];
    artifact.ports = vec![ArtifactPort {
        debug_name: "WorkerPort".to_string(),
        protocol: ProtocolId::new(0),
        target_process: WORKER_PROCESS,
        required_authority: ArtifactCapabilityDescriptor::PortConnect {
            port: PortId::new(0),
        },
    }];
    artifact.processes[MAIN_PROCESS.index()]
        .authorities
        .push(ArtifactAuthority {
            debug_name: "connect_worker".to_string(),
            descriptor: ArtifactCapabilityDescriptor::PortConnect {
                port: PortId::new(0),
            },
        });
    let Some(ArtifactAction::SendOutcome { port, .. }) = artifact.processes[MAIN_PROCESS.index()]
        .transitions[0]
        .actions
        .get_mut(1)
    else {
        panic!("test artifact should have a send outcome action");
    };
    *port = Some(PortId::new(0));
}

fn denied_port_send_outcome_binding_json() -> String {
    r#"{"schema_id":"mantle.runtime_authority_effect_binding","schema_version_major":1,"schema_version_minor":0,"artifact_kind":"runtime_authority_effect_binding","deployment_id":0,"source_language":"test_frontend","source_module":"unbound_worker_process_ref","source_fingerprint":"0000000000000000","source_fingerprint_algorithm":"fnv1a64-diagnostic","mantle_artifact_format":"mantle-target-artifact","mantle_artifact_schema_version":"6","mantle_artifact_module":"unbound_worker_process_ref","mantle_artifact_source_hash_fnv1a64":"0000000000000000","authority_effect_schema_id":"test_frontend.checked_authority_effects","authority_effect_schema_version_major":1,"authority_effect_schema_version_minor":0,"authority_policy_schema_id":"test_frontend.authority_policy_decisions","authority_policy_schema_version_major":1,"authority_policy_schema_version_minor":0,"processes":[{"process_id":0,"authorities":[{"authority_id":0,"descriptor":{"kind":"spawn","target_process_id":1}},{"authority_id":1,"descriptor":{"kind":"port_connect","port_id":0}}],"spawn_sites":[{"spawn_site_id":0,"kind":"dynamic_local","target_process_id":1,"authority_id":0,"supervisor_id":null,"supervisor_child_id":null}],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[{"effect_id":0,"effect":"spawn"},{"effect_id":1,"effect":"send"}]}]},{"process_id":1,"authorities":[],"spawn_sites":[],"transition_effects":[{"transition_id":0,"message_id":0,"current_state_id":null,"effects":[]}]}],"component_authority_surfaces":[],"policy_decisions":[{"decision_id":0,"process_id":0,"authority_id":0,"descriptor":{"kind":"spawn","target_process_id":1},"decision":"admit"},{"decision_id":1,"process_id":0,"authority_id":1,"descriptor":{"kind":"port_connect","port_id":0},"decision":"deny"}],"admission_result":"admitted","extensions":{}}"#.to_string()
}
