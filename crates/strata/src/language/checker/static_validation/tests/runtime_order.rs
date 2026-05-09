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
