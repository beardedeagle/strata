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
            mailbox: VecDeque::new(),
        },
        StaticProcessInstance {
            pid: StaticProcessId::FIRST
                .checked_next()
                .expect("next static pid should exist"),
            process_id: checked_process_id(1),
            state: checked_state_id(0),
            status: StaticProcessStatus::Running,
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
