use super::support::*;
use crate::executable::{ExecutableActionPlan, ExecutableProgram, ExecutableSendTarget};
use crate::program::LoadedAction;
use crate::run::run_loaded_program_with_host;

#[test]
fn executable_plan_constructs_typed_action_tables_after_admission() {
    let artifact = valid_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let executable =
        ExecutableProgram::from_admitted(&program).expect("executable plan should construct");

    assert_eq!(executable.process_count(), 2);
    assert_eq!(executable.entry().process_id, ProcessId::new(0));
    assert_eq!(executable.entry().message_id, MessageId::new(0));

    let transition = executable
        .transition_for_dispatch(ProcessId::new(0), MessageId::new(0), StateId::new(0), None)
        .expect("entry transition should dispatch by typed ids");
    let actions = transition
        .actions()
        .all_actions(executable.actions())
        .map(|(_, action)| action)
        .collect::<Vec<_>>();

    match actions[0] {
        ExecutableActionPlan::Spawn {
            target,
            process_ref,
            spawn,
        } => {
            assert_eq!(*target, ProcessId::new(1));
            assert_eq!(process_ref.id, ProcessRefId::new(0));
            assert_eq!(process_ref.target_process, ProcessId::new(1));
            assert_eq!(spawn.id, SPAWN_SITE);
            assert_eq!(spawn.authority, SPAWN_AUTHORITY);
        }
        action => panic!("expected planned spawn action, got {action:?}"),
    }
    match actions[1] {
        ExecutableActionPlan::Send {
            target: ExecutableSendTarget::ProcessRef(process_ref),
            message,
            ..
        } => {
            assert_eq!(*message, MessageId::new(0));
            assert_eq!(process_ref.id, ProcessRefId::new(0));
            assert_eq!(process_ref.target_process, ProcessId::new(1));
        }
        action => panic!("expected planned send action, got {action:?}"),
    }
}

#[test]
fn executable_plan_order_is_deterministic_when_loaded_transition_order_changes() {
    let artifact = sequence_artifact();
    let program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let baseline = ExecutableProgram::from_admitted(&program)
        .expect("baseline executable plan should construct")
        .transition_signature();

    let mut reordered_artifact = sequence_artifact();
    reordered_artifact.processes[1].transitions.reverse();
    let reordered_program =
        LoadedProgram::from_artifact(&reordered_artifact).expect("reordered artifact should load");
    let reordered = ExecutableProgram::from_admitted(&reordered_program)
        .expect("reordered executable plan should construct")
        .transition_signature();

    assert_eq!(baseline, reordered);
}

#[test]
fn executable_plan_ignores_stale_loaded_transition_lookup() {
    let artifact = sequence_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions.swap(0, 1);
    let mut host = InMemoryRuntimeHost::default();

    let report = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("executable plan should rebuild dispatch from current loaded transitions");

    assert_eq!(
        report.emitted_outputs,
        vec![
            "worker handled First".to_string(),
            "worker handled Second".to_string()
        ]
    );
}

#[test]
fn executable_plan_dispatch_uses_ids_not_debug_labels() {
    let artifact = valid_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].debug_name = "WorkerLabel".to_string();
    program.processes[1].debug_name = "MainLabel".to_string();
    let mut host = InMemoryRuntimeHost::default();

    let report = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("debug labels must remain trace metadata only");

    assert_eq!(report.entry_process, "WorkerLabel");
    assert_eq!(
        report.emitted_outputs,
        vec!["worker handled Ping".to_string()]
    );
    assert!(
        report
            .delivered_messages
            .iter()
            .any(|delivery| delivery.process == "MainLabel" && delivery.message == "Ping")
    );
}

#[test]
fn executable_plan_rejects_invalid_loaded_references_before_artifact_loaded() {
    let artifact = valid_artifact();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[1].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(99),
        });
    let mut host = InMemoryRuntimeHost::default();

    let err = run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect_err("executable plan construction must fail closed");

    assert!(err.to_string().contains("output id 99 is not loaded"));
    assert!(host.events().is_empty());
    assert!(host.stdout().is_empty());
}
