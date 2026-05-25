#[test]
fn property_generated_uniform_arm_prefix_shapes_lower_as_typed_actions() {
    for kind in ARM_PREFIX_KINDS {
        let source = arm_prefix_property_source(
            format!("process_return_match_arm_prefix_property_{}", kind.name()).as_str(),
            kind,
            kind,
            effects_for_kind(kind),
        );
        let checked = check_source(&source)
            .unwrap_or_else(|err| panic!("generated {kind:?} source should check: {err}"));
        let worker = checked_process(&checked, "Worker");
        let expected_effects = effects_for_kind(kind);
        let expected_actions = actions_for_kind(kind);

        assert_eq!(
            worker.transitions().len(),
            2,
            "generated {kind:?} source should create one transition per concrete payload"
        );
        for transition in worker.transitions() {
            assert_eq!(
                transition.effects(),
                expected_effects,
                "generated {kind:?} transition should retain exact declared effects"
            );
            assert_eq!(
                checked_action_kinds(transition.actions()),
                expected_actions,
                "generated {kind:?} transition should lower only uniform spawn plus selected arm actions"
            );
        }

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated {kind:?} source should lower: {err}"));
        let worker_artifact = artifact
            .processes
            .iter()
            .find(|process| process.debug_name == "Worker")
            .expect("Worker artifact process should exist");
        for transition in &worker_artifact.transitions {
            assert_eq!(
                transition.effects.to_vec(),
                expected_effects
                    .iter()
                    .copied()
                    .map(artifact_effect_for)
                    .collect::<Vec<_>>(),
                "generated {kind:?} artifact transition should retain exact typed effects"
            );
            assert_eq!(
                artifact_action_kinds(&transition.actions),
                expected_actions,
                "generated {kind:?} artifact transition should contain typed action variants"
            );
            assert_artifact_send_actions_use_ids(&transition.actions);
        }
    }
}

#[test]
fn property_divergent_arm_prefix_effect_sets_fail_closed() {
    for ready in ARM_PREFIX_KINDS {
        for done in ARM_PREFIX_KINDS {
            if ready == done {
                continue;
            }
            let declared_effects = union_effects(ready, done);
            let source = arm_prefix_property_source(
                format!(
                    "process_return_match_arm_prefix_reject_{}_{}",
                    ready.name(),
                    done.name()
                )
                .as_str(),
                ready,
                done,
                &declared_effects,
            );

            let err = match check_source(&source) {
                Ok(_) => {
                    panic!("divergent generated arms {ready:?}/{done:?} should fail closed")
                }
                Err(err) => err,
            };
            let err = err.to_string();
            assert!(
                err.contains("declares effect") && err.contains("does not use it"),
                "divergent generated arms {ready:?}/{done:?} failed for unexpected reason: {err}"
            );
        }
    }
}

#[test]
fn property_generated_selected_arm_action_block_shapes_lower_as_typed_actions() {
    for kind in ARM_ACTION_BLOCK_KINDS {
        let source = arm_action_block_property_source(
            format!(
                "process_return_match_arm_action_block_property_{}",
                kind.name()
            )
            .as_str(),
            kind,
        );
        let checked = check_source(&source).unwrap_or_else(|err| {
            panic!("generated selected-arm {kind:?} source should check: {err}")
        });
        let worker = checked_process(&checked, "Worker");
        let expected_actions = kind.top_level_actions();

        assert_eq!(
            worker.transitions().len(),
            2,
            "generated {kind:?} source should create one transition per concrete payload"
        );
        for transition in worker.transitions() {
            assert_eq!(
                transition.effects(),
                kind.effects(),
                "generated {kind:?} transition should retain exact declared effects"
            );
            assert_eq!(
                checked_action_kinds(transition.actions()),
                expected_actions,
                "generated {kind:?} transition should lower selected arm as typed action-block actions"
            );
        }

        let artifact = lower_to_artifact(&checked, &source)
            .unwrap_or_else(|err| panic!("generated selected-arm {kind:?} should lower: {err}"));
        let worker_artifact = artifact
            .processes
            .iter()
            .find(|process| process.debug_name == "Worker")
            .expect("Worker artifact process should exist");
        for transition in &worker_artifact.transitions {
            assert_eq!(
                artifact_action_kinds(&transition.actions),
                expected_actions,
                "generated {kind:?} artifact transition should preserve typed action variants"
            );
            assert_artifact_send_actions_use_ids(&transition.actions);
            assert_nested_artifact_send_actions_use_ids(&transition.actions);
        }
        let encoded = artifact.encode();
        assert!(
            !encoded.lines().any(|line| line.contains("job_phase")),
            "generated {kind:?} artifact must not dispatch through source loop binding job_phase"
        );
    }
}

#[test]
fn exhaustive_bounded_selected_arm_action_blocks_lower_as_typed_artifact_actions() {
    let mut sequence = Vec::new();
    for len in 0..=2 {
        visit_bounded_arm_statement_sequences(len, &mut sequence, &mut |sequence| {
            assert_bounded_arm_statement_sequence(sequence);
        });
    }
}

#[test]
fn exhaustive_bounded_invalid_selected_arm_action_blocks_fail_closed() {
    for case in BOUNDED_INVALID_ARM_CASES {
        let source = invalid_bounded_arm_action_block_source(case);
        let err = match check_source(&source) {
            Ok(_) => panic!("invalid bounded arm case {case:?} should fail"),
            Err(err) => err,
        };
        let message = err.to_string();

        assert!(
            message.contains(case.expected_diagnostic()),
            "invalid bounded arm case {case:?} failed for unexpected reason: {message}"
        );
    }
}

