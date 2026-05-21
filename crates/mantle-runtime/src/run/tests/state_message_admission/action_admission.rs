use super::super::support::*;

#[test]
fn runtime_accepts_loaded_one_nested_if_else_inside_runtime_if_branch_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.outputs.push("nested branch emitted".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    let condition = LoadedValueTemplate::Literal {
        ty: bool_type,
        value: RuntimeValue::Atom("True".to_string()),
    };
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: condition.clone(),
        then_actions: vec![LoadedAction::IfElse {
            condition,
            then_actions: vec![LoadedAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: Vec::new(),
        }],
        else_actions: Vec::new(),
    }];

    program
        .validate_admission()
        .expect("one nested loaded if_else action should pass admission");
}

#[test]
fn runtime_rejects_loaded_if_else_action_nesting_above_limit_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.outputs.push("nested branch emitted".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    let condition = LoadedValueTemplate::Literal {
        ty: bool_type,
        value: RuntimeValue::Atom("True".to_string()),
    };
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: condition.clone(),
        then_actions: vec![LoadedAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![LoadedAction::IfElse {
                condition,
                then_actions: vec![LoadedAction::Emit {
                    output: OutputId::new(0),
                }],
                else_actions: Vec::new(),
            }],
            else_actions: Vec::new(),
        }],
        else_actions: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 runtime if action nesting exceeds maximum depth of 2",
    );
}

#[test]
fn runtime_rejects_loaded_both_empty_if_else_action_branches_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: LoadedValueTemplate::Literal {
            ty: bool_type,
            value: RuntimeValue::Atom("True".to_string()),
        },
        then_actions: Vec::new(),
        else_actions: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 runtime if action branches cannot both be empty",
    );
}

#[test]
fn runtime_rejects_loaded_spawn_inside_runtime_if_branch_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Spawn]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: LoadedValueTemplate::Literal {
            ty: bool_type,
            value: RuntimeValue::Atom("True".to_string()),
        },
        then_actions: vec![LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }],
        else_actions: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 runtime if branch cannot bind process references",
    );
}

#[test]
fn runtime_accepts_loaded_for_each_inside_runtime_if_branch() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let list_type = push_list_type(&mut program, "BoolList", bool_type, 1);
    program.outputs.push("branch loop emitted".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: LoadedValueTemplate::Literal {
            ty: bool_type,
            value: RuntimeValue::Atom("True".to_string()),
        },
        then_actions: vec![LoadedAction::ForEach {
            element: crate::program::LoadedLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: LoadedValueTemplate::Literal {
                ty: list_type,
                value: RuntimeValue::List(vec![RuntimeValue::Atom("True".to_string())]),
            },
            max_items: 1,
            body: vec![LoadedAction::Emit {
                output: OutputId::new(0),
            }],
        }],
        else_actions: Vec::new(),
    }];

    let mut host = InMemoryRuntimeHost::default();
    run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("for_each inside runtime if branch should be admitted and run");
    assert_eq!(host.stdout(), ["branch loop emitted"]);
}

#[test]
fn runtime_rejects_loaded_spawn_inside_runtime_if_branch_loop_before_artifact_loaded() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let list_type = push_list_type(&mut program, "BoolList", bool_type, 1);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Spawn]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::IfElse {
        condition: LoadedValueTemplate::Literal {
            ty: bool_type,
            value: RuntimeValue::Atom("True".to_string()),
        },
        then_actions: vec![LoadedAction::ForEach {
            element: crate::program::LoadedLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: LoadedValueTemplate::Literal {
                ty: list_type,
                value: RuntimeValue::List(vec![RuntimeValue::Atom("True".to_string())]),
            },
            max_items: 1,
            body: vec![LoadedAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }],
        }],
        else_actions: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 for loop body cannot bind process references",
    );
}

#[test]
fn runtime_rejects_loaded_unknown_emit_output_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0]
        .actions
        .push(LoadedAction::Emit {
            output: OutputId::new(0),
        });

    assert_loaded_admission_rejects_before_artifact_loaded(&program, "output id 0 is not loaded");
}
