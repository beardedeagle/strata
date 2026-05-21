use super::support::*;

#[test]
fn runtime_accepts_loaded_nested_if_else_inside_for_each_loop_branch() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let list_type = push_list_type(&mut program, "BoolList", bool_type, 1);
    program
        .outputs
        .push("nested loop branch emitted".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::ForEach {
        element: crate::program::LoadedLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: LoadedValueTemplate::Literal {
            ty: list_type,
            value: RuntimeValue::List(vec![RuntimeValue::Atom("True".to_string())]),
        },
        max_items: 1,
        body: vec![LoadedActionFixture::nested_loop_branch(bool_type)],
    }];

    let mut host = InMemoryRuntimeHost::default();
    run_loaded_program_with_host(&program, &mut host, RunLimits::default())
        .expect("one nested runtime if inside loop branch should admit and run");
    assert_eq!(host.stdout(), ["nested loop branch emitted"]);
}

#[test]
fn runtime_rejects_loaded_if_else_nesting_above_limit_inside_for_each_loop_branch() {
    let mut artifact = artifact_with_unbound_worker_process_ref();
    let bool_type = TypeId::from_index(artifact.types.len()).expect("test type id should fit");
    artifact.types.push(ArtifactType::enum_value(
        "Bool",
        vec!["False".to_string(), "True".to_string()],
    ));
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    let list_type = push_list_type(&mut program, "BoolList", bool_type, 1);
    program
        .outputs
        .push("nested loop branch emitted".to_string());
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[ArtifactEffect::Emit]);
    program.processes[0].transitions[0].actions = vec![LoadedAction::ForEach {
        element: crate::program::LoadedLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: LoadedValueTemplate::Literal {
            ty: list_type,
            value: RuntimeValue::List(vec![RuntimeValue::Atom("True".to_string())]),
        },
        max_items: 1,
        body: vec![LoadedAction::IfElse {
            condition: loop_condition(bool_type),
            then_actions: vec![LoadedActionFixture::nested_loop_branch(bool_type)],
            else_actions: vec![LoadedAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Main transition 0 runtime if action nesting exceeds maximum depth",
    );
}

struct LoadedActionFixture;

impl LoadedActionFixture {
    fn nested_loop_branch(bool_type: TypeId) -> LoadedAction {
        LoadedAction::IfElse {
            condition: loop_condition(bool_type),
            then_actions: vec![LoadedAction::IfElse {
                condition: loop_condition(bool_type),
                then_actions: vec![LoadedAction::Emit {
                    output: OutputId::new(0),
                }],
                else_actions: vec![LoadedAction::Emit {
                    output: OutputId::new(0),
                }],
            }],
            else_actions: vec![LoadedAction::Emit {
                output: OutputId::new(0),
            }],
        }
    }
}

fn loop_condition(bool_type: TypeId) -> LoadedValueTemplate {
    LoadedValueTemplate::LoopElement {
        ty: bool_type,
        element: LoopElementId::new(0),
    }
}
