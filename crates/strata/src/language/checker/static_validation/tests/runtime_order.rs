use super::support::*;

#[test]
fn static_process_refs_bind_sparsely_within_transition_scope() {
    let process = checked_process_with_declared_refs(2);
    let mut process_refs = BTreeMap::new();
    let process_ref = checked_process_ref_id(1);
    let pid = StaticProcessId::FIRST
        .checked_next()
        .expect("next static pid should exist");

    bind_static_process_ref(&process, &mut process_refs, process_ref, pid)
        .expect("declared process reference should bind");

    assert_eq!(process_refs.len(), 1);
    assert_eq!(
        resolve_static_process_ref(&process, &process_refs, process_ref)
            .expect("bound sparse process reference should resolve"),
        pid
    );
    let err = resolve_static_process_ref(&process, &process_refs, checked_process_ref_id(0))
        .expect_err("declared but unbound sparse process reference should fail");
    assert!(
        err.to_string()
            .contains("sends to unbound process reference id 0")
    );
}

#[test]
fn static_process_lookup_indexes_by_pid() {
    let instances = vec![
        StaticProcessInstance {
            pid: StaticProcessId::FIRST,
            process_id: checked_process_id(0),
            state: checked_state_id(0),
            status: StaticProcessStatus::Running,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Open,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        },
        StaticProcessInstance {
            pid: StaticProcessId::FIRST
                .checked_next()
                .expect("next static pid should exist"),
            process_id: checked_process_id(1),
            state: checked_state_id(0),
            status: StaticProcessStatus::Running,
            stop_reason: None,
            mailbox_state: StaticMailboxState::Open,
            supervisor_parent: None,
            mailbox: VecDeque::new(),
        },
    ];

    assert_eq!(
        static_process_index_for_pid(&instances, StaticProcessId::FIRST)
            .expect("first static pid should resolve"),
        0
    );
    assert_eq!(
        static_process_index_for_pid(&instances, instances[1].pid)
            .expect("second static pid should resolve"),
        1
    );
}

#[test]
fn static_process_lookup_rejects_unspawned_pid() {
    let instances = vec![StaticProcessInstance {
        pid: StaticProcessId::FIRST,
        process_id: checked_process_id(0),
        state: checked_state_id(0),
        status: StaticProcessStatus::Running,
        stop_reason: None,
        mailbox_state: StaticMailboxState::Open,
        supervisor_parent: None,
        mailbox: VecDeque::new(),
    }];
    let missing_pid = StaticProcessId::FIRST
        .checked_next()
        .expect("next static pid should exist");

    let err = static_process_index_for_pid(&instances, missing_pid)
        .expect_err("unspawned static pid should be rejected");

    assert!(
        err.to_string()
            .contains("static runtime process id 2 is not spawned")
    );
}

#[test]
fn static_process_capacity_rejects_instance_limit() {
    ensure_static_process_capacity(STATIC_RUNTIME_PROCESS_LIMIT - 1)
        .expect("capacity should allow the final process slot");

    let err = ensure_static_process_capacity(STATIC_RUNTIME_PROCESS_LIMIT)
        .expect_err("capacity should reject a new process beyond the limit");

    assert!(
        err.to_string().contains(
            "static runtime process instance limit exceeded at 10000 process instance(s)"
        )
    );
}

#[test]
fn static_spawn_outcome_capacity_counts_supervised_subtree() {
    let processes = vec![
        checked_process_with_supervised_targets(0, &[1]),
        checked_process_with_supervised_targets(1, &[2]),
        checked_process_with_supervised_targets(2, &[]),
    ];

    assert!(
        static_spawn_capacity_available_for_test(
            &processes,
            STATIC_RUNTIME_PROCESS_LIMIT - 3,
            checked_process_id(0),
        )
        .expect("supervised subtree size should fit")
    );
    assert!(
        !static_spawn_capacity_available_for_test(
            &processes,
            STATIC_RUNTIME_PROCESS_LIMIT - 2,
            checked_process_id(0),
        )
        .expect("supervised subtree size should not fit")
    );
    assert!(
        static_spawn_capacity_available_for_test(
            &processes,
            STATIC_RUNTIME_PROCESS_LIMIT - 1,
            checked_process_id(2),
        )
        .expect("leaf spawn should fit in one remaining slot")
    );
}

#[test]
fn static_spawn_outcome_capacity_exhaustion_binds_error_without_partial_subtree_spawn() {
    let processes = vec![
        checked_process_with_supervised_targets(0, &[]),
        checked_process_with_supervised_targets(1, &[2]),
        checked_process_with_supervised_targets(2, &[3]),
        checked_process_with_supervised_targets(3, &[]),
    ];
    let outcome_ty = spawn_outcome_type(1);
    let execution = static_spawn_outcome_execution_for_test(
        &processes,
        checked_process_id(0),
        STATIC_RUNTIME_PROCESS_LIMIT - 2,
        checked_process_id(1),
        outcome_ty.clone(),
    )
    .expect("exhausted spawn outcome should bind a typed error");

    assert_eq!(execution.instance_count, STATIC_RUNTIME_PROCESS_LIMIT - 2);
    assert_eq!(
        execution.next_pid.as_u32(),
        u32::try_from(STATIC_RUNTIME_PROCESS_LIMIT - 1).expect("static limit should fit u32")
    );
    assert_eq!(execution.outcome.ty(), &outcome_ty);
    assert_eq!(execution.outcome.label(), "Err(Exhausted(Unit))");
}

#[test]
fn static_send_outcome_running_closed_process_ref_returns_mailbox_closed() {
    let processes = vec![
        checked_process_with_declared_refs(1),
        checked_process_with_supervised_targets(1, &[]),
    ];
    let execution = static_process_ref_send_outcome_for_test(
        &processes,
        StaticProcessStatus::Running,
        None,
        StaticMailboxState::Closed,
    )
    .expect("closed running mailbox should bind a typed send failure");

    assert_eq!(execution.target_mailbox_len, 0);
    assert_eq!(execution.outcome.label(), "Err(MailboxClosed(Start))");
}

#[test]
fn static_send_outcome_normal_stop_returns_stopped() {
    let processes = vec![
        checked_process_with_declared_refs(1),
        checked_process_with_supervised_targets(1, &[]),
    ];
    let execution = static_process_ref_send_outcome_for_test(
        &processes,
        StaticProcessStatus::Stopped,
        Some(StaticStopReason::Normal),
        StaticMailboxState::Closed,
    )
    .expect("normal stop should bind a typed stopped send failure");

    assert_eq!(execution.target_mailbox_len, 0);
    assert_eq!(execution.outcome.label(), "Err(Stopped(Start))");
}

#[test]
fn static_supervisor_action_stop_returns_mailbox_closed() {
    let processes = vec![
        checked_process_with_supervised_targets(0, &[1]),
        checked_process_with_supervised_targets(1, &[]),
    ];
    let execution = static_supervisor_child_send_outcome_for_test(
        &processes,
        StaticProcessStatus::Stopped,
        Some(StaticStopReason::SupervisorAction),
        StaticMailboxState::Closed,
    )
    .expect("supervisor-driven stop should bind a typed closed-mailbox send failure");

    assert_eq!(execution.target_mailbox_len, 0);
    assert_eq!(execution.outcome.label(), "Err(MailboxClosed(Start))");
}

#[test]
fn static_supervised_restart_capacity_exhaustion_rejects_validation() {
    let processes = vec![
        checked_process_with_supervised_targets(0, &[1]),
        checked_process_with_supervised_targets(1, &[2]),
        checked_process_with_supervised_targets(2, &[]),
    ];

    let err =
        static_supervised_restart_exit_for_test(&processes, STATIC_RUNTIME_PROCESS_LIMIT - 1, 0)
            .expect_err("capacity-denied supervised restart should fail closed");

    assert!(
        err.to_string()
            .contains("static runtime supervisor restart capacity exceeded"),
        "{err}"
    );
}

#[test]
fn static_supervised_restart_intensity_exhaustion_rejects_validation() {
    let processes = vec![
        checked_process_with_supervised_targets(0, &[1]),
        checked_process_with_supervised_targets(1, &[]),
    ];

    let err = static_supervised_restart_exit_for_test(&processes, 2, 1)
        .expect_err("restart past static intensity budget should fail closed");

    assert!(
        err.to_string()
            .contains("static runtime supervisor restart intensity exceeded"),
        "{err}"
    );
}

#[test]
fn static_supervised_restart_throttle_rejects_unproven_second_restart() {
    let processes = vec![
        checked_process_with_supervised_targets_and_intensity(0, &[1], 2),
        checked_process_with_supervised_targets(1, &[]),
    ];

    let err = static_supervised_restart_exit_for_test(&processes, 2, 1)
        .expect_err("second restart without static time proof should fail closed");

    assert!(
        err.to_string()
            .contains("static runtime supervisor restart throttled"),
        "{err}"
    );
}

#[test]
fn static_runtime_resolves_next_state_before_actions() {
    let main_state = value_type("MainState");
    let main = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Main"),
        state_type: main_state.clone(),
        state_values: checked_state_values_for_type(main_state.clone(), &["MainState"]),
        message_type: value_type("MainMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Start".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                Some(main_state.clone()),
            )
            .expect("valid checked message case"),
        ],
        process_refs: vec![CheckedProcessRef::new(
            ident("worker"),
            checked_process_id(1),
        )],
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Template(CheckedValueTemplate::ReceivedPayload {
                ty: main_state,
            }),
            effects: Vec::new(),
            actions: vec![CheckedAction::Send {
                target: CheckedSendTarget::ProcessRef(checked_process_ref_id(0)),
                port: None,
                message: checked_message_id(0),
                payload: None,
            }],
        })],
    });
    let worker = CheckedProcess::new(CheckedProcessParts {
        debug_name: ident("Worker"),
        state_type: value_type("WorkerState"),
        state_values: checked_state_values("WorkerState", &["WorkerState"]),
        message_type: value_type("WorkerMsg"),
        message_cases: vec![
            CheckedMessageCase::new(
                "Ping".to_string(),
                CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                None,
            )
            .expect("valid checked message case"),
        ],
        process_refs: Vec::new(),
        mailbox_bound: 1,
        init_state: checked_state_id(0),
        transitions: vec![CheckedTransition::new(CheckedTransitionParts {
            current_state: None,
            message: checked_message_id(0),
            step_result: CheckedStepResult::Stop,
            next_state: CheckedNextState::Current,
            effects: Vec::new(),
            actions: Vec::new(),
        })],
    });

    let err = validate_static_runtime_order(
        &[main, worker],
        checked_process_id(0),
        checked_message_id(0),
    )
    .expect_err("next-state preflight should fail before actions execute");

    assert!(
        err.to_string()
            .contains("received payload template requires a payload-bearing message"),
        "{err}"
    );
}

fn checked_process_with_supervised_targets(
    index: usize,
    child_targets: &[usize],
) -> CheckedProcess {
    checked_process_with_supervised_targets_and_intensity(index, child_targets, 1)
}

fn checked_process_with_supervised_targets_and_intensity(
    index: usize,
    child_targets: &[usize],
    max_restarts: u32,
) -> CheckedProcess {
    let debug_name = format!("Process{index}");
    let state_name = format!("State{index}");
    let message_name = format!("Message{index}");
    let state_type = value_type(&state_name);
    let supervisor_plans = if child_targets.is_empty() {
        Vec::new()
    } else {
        vec![
            CheckedSupervisorPlan::new(
                CheckedSupervisorStrategy::OneForOne,
                CheckedSupervisorRestartIntensity::new(max_restarts, 1)
                    .expect("test restart intensity should be valid"),
                child_targets
                    .iter()
                    .enumerate()
                    .map(|(child_index, target)| {
                        CheckedSupervisorChild::new(
                            ident(&format!("child_{child_index}")),
                            checked_process_id(*target),
                            CheckedSupervisorChildMode::Permanent,
                            checked_spawn_site_id(child_index),
                        )
                    })
                    .collect(),
            )
            .expect("test supervisor plan should be valid"),
        ]
    };

    CheckedProcess::with_authority(
        CheckedProcessParts {
            debug_name: ident(&debug_name),
            state_type: state_type.clone(),
            state_values: checked_state_values_for_type(state_type, &[&state_name]),
            message_type: value_type(&message_name),
            message_cases: vec![
                CheckedMessageCase::new(
                    "Start".to_string(),
                    CheckedMessageVariantId::from_index(0).expect("valid message variant id"),
                    None,
                )
                .expect("valid checked message case"),
            ],
            process_refs: Vec::new(),
            mailbox_bound: 1,
            init_state: checked_state_id(0),
            transitions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
        supervisor_plans,
    )
}

fn spawn_outcome_type(target: usize) -> CheckedTypeRef {
    let unit_ty = value_type("Unit");
    let process_ref_ty = CheckedTypeRef::test_process_ref(
        &format!("ProcessRef<Process{target}>"),
        checked_process_id(target),
    );
    let spawn_error_ty = CheckedTypeRef::new(
        value_type("SpawnError<Unit>").id(),
        "SpawnError<Unit>".to_string(),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum {
                variants: vec![
                    CheckedEnumVariant {
                        name: ident("Denied"),
                        payload_type: Some(unit_ty.id()),
                    },
                    CheckedEnumVariant {
                        name: ident("Exhausted"),
                        payload_type: Some(unit_ty.id()),
                    },
                    CheckedEnumVariant {
                        name: ident("BackendUnavailable"),
                        payload_type: Some(unit_ty.id()),
                    },
                ],
            },
        },
    );
    CheckedTypeRef::new(
        value_type("SpawnOutcome").id(),
        format!("Result<ProcessRef<Process{target}>,SpawnError<Unit>>"),
        CheckedTypeKind::Value {
            shape: CheckedValueShape::Enum {
                variants: vec![
                    CheckedEnumVariant {
                        name: ident("Ok"),
                        payload_type: Some(process_ref_ty.id()),
                    },
                    CheckedEnumVariant {
                        name: ident("Err"),
                        payload_type: Some(spawn_error_ty.id()),
                    },
                ],
            },
        },
    )
}
