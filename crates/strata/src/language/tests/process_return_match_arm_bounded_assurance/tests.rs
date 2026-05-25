#[test]
fn bounded_assurance_selected_arm_action_blocks_match_checked_ir_and_artifacts() {
    let mut sequence = Vec::with_capacity(MAX_MODEL_SEQUENCE_LEN);
    let mut case_index = 0usize;
    for len in 0..=MAX_MODEL_SEQUENCE_LEN {
        visit_model_sequences(len, &mut sequence, &mut |sequence| {
            assert_valid_model_sequence(case_index, sequence, ACTION_BLOCK_TERMINAL_PROFILE);
            case_index = case_index
                .checked_add(1)
                .expect("bounded model case count should not overflow");
        });
    }

    assert_eq!(case_index, expected_model_case_count());
}

#[test]
fn bounded_assurance_terminal_variants_lower_checked_ir_and_artifacts() {
    for (index, terminal) in TERMINAL_CASES.into_iter().enumerate() {
        let module_name = format!("process_return_match_arm_assurance_terminal_{index}");
        let profile = TerminalProfile {
            ready: terminal,
            done: ModelTerminal::StopSawDone,
        };
        let source = source_with_arm_bodies(
            &module_name,
            "[spawn]",
            "",
            "",
            terminal.source(),
            ModelTerminal::StopSawDone.source(),
            false,
        );
        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should check: {err}"));
        let worker = checked_process(&checked, "Worker");
        assert_eq!(
            worker.transitions().len(),
            1,
            "terminal case {terminal:?} should select the reachable Ready arm"
        );
        assert_checked_terminal(worker, &worker.transitions()[0], profile);

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should lower: {err}"));
        artifact
            .validate()
            .unwrap_or_else(|err| panic!("terminal case {terminal:?} should validate: {err}"));
        let worker_artifact = artifact_process(&artifact, "Worker");
        assert_eq!(
            worker_artifact.transitions.len(),
            1,
            "terminal case {terminal:?} should emit one typed artifact transition"
        );
        assert_artifact_terminal(worker_artifact, &worker_artifact.transitions[0], profile);
    }
}

#[test]
fn bounded_assurance_invalid_source_models_fail_closed() {
    for case in INVALID_SOURCE_CASES {
        let source = invalid_source_case(case);
        let err = match check_source(&source) {
            Ok(_) => panic!("invalid bounded source case {} should fail", case.name),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(
            message.contains(case.expected),
            "invalid bounded source case {} failed for unexpected reason: {message}",
            case.name
        );
    }
}

#[test]
fn bounded_assurance_artifact_bypass_mutations_fail_admission() {
    let source = valid_source_for_sequence(
        "process_return_match_arm_assurance_artifact_bypass",
        &[
            ModelStatement::IfWithForNestedIf,
            ModelStatement::ForEach,
            ModelStatement::ForWithIf,
        ],
        ACTION_BLOCK_TERMINAL_PROFILE,
    );
    let checked = check_source(&source).expect("artifact-bypass seed source should check");
    let artifact =
        lower_to_artifact(&checked, &source).expect("artifact-bypass seed source should lower");
    artifact
        .validate()
        .expect("artifact-bypass seed artifact should validate before mutation");

    assert_artifact_mutation_rejected(
        artifact.clone(),
        insert_nested_for_each_into_first_worker_loop,
        "nested for loops are not supported",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        deepen_first_worker_runtime_if,
        "runtime if action nesting exceeds maximum depth",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        insert_spawn_inside_first_worker_runtime_if,
        "runtime if branch cannot bind process references",
    );
    assert_artifact_mutation_rejected(
        artifact.clone(),
        empty_first_worker_runtime_if_branches,
        "runtime if action branches cannot both be empty",
    );
    assert_artifact_mutation_rejected(
        artifact,
        remove_send_effect_from_worker_transition,
        "uses effect send but does not declare it",
    );
}

