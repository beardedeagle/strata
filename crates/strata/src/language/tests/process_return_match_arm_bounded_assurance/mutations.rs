fn assert_artifact_mutation_rejected(
    mut artifact: MantleArtifact,
    mutate: fn(&mut MantleArtifact),
    expected: &str,
) {
    mutate(&mut artifact);
    let err = match artifact.validate() {
        Ok(_) => panic!("mutated artifact should reject with {expected}"),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(
        message.contains(expected),
        "mutated artifact rejected with unexpected diagnostic: {message}"
    );
}

fn first_worker_transition_mut(artifact: &mut MantleArtifact) -> &mut ArtifactTransition {
    artifact_process_mut(artifact, "Worker")
        .transitions
        .first_mut()
        .expect("Worker artifact process should have transitions")
}

fn insert_nested_for_each_into_first_worker_loop(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    let nested = transition
        .actions
        .iter()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("seed artifact should contain a top-level for_each action")
        .clone();
    let ArtifactAction::ForEach { body, .. } = transition
        .actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::ForEach { .. }))
        .expect("seed artifact should contain a mutable top-level for_each action")
    else {
        unreachable!("find predicate already matched for_each");
    };
    body.push(nested);
}

fn deepen_first_worker_runtime_if(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    assert!(
        deepen_first_nested_if(&mut transition.actions),
        "seed artifact should contain nested runtime if actions"
    );
}

fn insert_spawn_inside_first_worker_runtime_if(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    let spawn = transition
        .actions
        .iter()
        .find(|action| matches!(action, ArtifactAction::Spawn { .. }))
        .expect("seed artifact should contain a spawn action")
        .clone();
    assert!(
        push_into_first_if_branch(&mut transition.actions, spawn),
        "seed artifact should contain a runtime if action"
    );
}

fn empty_first_worker_runtime_if_branches(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    assert!(
        empty_first_if(&mut transition.actions),
        "seed artifact should contain a runtime if action"
    );
}

fn remove_send_effect_from_worker_transition(artifact: &mut MantleArtifact) {
    let transition = first_worker_transition_mut(artifact);
    transition
        .effects
        .retain(|effect| *effect != ArtifactEffect::Send);
}

fn deepen_first_nested_if(actions: &mut [ArtifactAction]) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                if insert_extra_if_inside_first_if(then_actions) {
                    return true;
                }
                if insert_extra_if_inside_first_if(else_actions) {
                    return true;
                }
                if deepen_first_nested_if(then_actions) {
                    return true;
                }
                if deepen_first_nested_if(else_actions) {
                    return true;
                }
            }
            ArtifactAction::ForEach { body, .. } => {
                if deepen_first_nested_if(body) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::SpawnOutcome { .. }
            | ArtifactAction::SendOutcome { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}

fn insert_extra_if_inside_first_if(actions: &mut [ArtifactAction]) -> bool {
    let Some(action) = actions
        .iter_mut()
        .find(|action| matches!(action, ArtifactAction::IfElse { .. }))
    else {
        return false;
    };
    let ArtifactAction::IfElse {
        condition,
        then_actions,
        ..
    } = action
    else {
        unreachable!("find predicate already matched if_else");
    };
    let nested_then = std::mem::take(then_actions);
    then_actions.push(ArtifactAction::IfElse {
        condition: condition.clone(),
        then_actions: nested_then,
        else_actions: Vec::new(),
    });
    true
}

fn push_into_first_if_branch(actions: &mut [ArtifactAction], inserted: ArtifactAction) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse { then_actions, .. } => {
                then_actions.push(inserted);
                return true;
            }
            ArtifactAction::ForEach { body, .. } => {
                if push_into_first_if_branch(body, inserted.clone()) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::SpawnOutcome { .. }
            | ArtifactAction::SendOutcome { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}

fn empty_first_if(actions: &mut [ArtifactAction]) -> bool {
    for action in actions {
        match action {
            ArtifactAction::IfElse {
                then_actions,
                else_actions,
                ..
            } => {
                then_actions.clear();
                else_actions.clear();
                return true;
            }
            ArtifactAction::ForEach { body, .. } => {
                if empty_first_if(body) {
                    return true;
                }
            }
            ArtifactAction::Emit { .. }
            | ArtifactAction::Spawn { .. }
            | ArtifactAction::SpawnOutcome { .. }
            | ArtifactAction::SendOutcome { .. }
            | ArtifactAction::Send { .. } => {}
        }
    }
    false
}
