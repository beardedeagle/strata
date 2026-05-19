use super::super::support::*;

#[test]
fn admission_rejects_nested_if_else_inside_runtime_if_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let condition = ArtifactValueTemplate::Literal {
        ty: bool_type,
        value: artifact_value("True"),
    };
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: condition.clone(),
        then_actions: vec![ArtifactAction::IfElse {
            condition,
            then_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
            else_actions: Vec::new(),
        }],
        else_actions: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("nested if_else inside top-level runtime if branch should fail admission");
    assert!(
        err.to_string()
            .contains("runtime if branch cannot contain nested runtime if actions"),
        "{err}"
    );
}

#[test]
fn admission_rejects_spawn_inside_runtime_if_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![ArtifactAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
        }],
        else_actions: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("spawn inside runtime if branch should fail admission");
    assert!(
        err.to_string()
            .contains("runtime if branch cannot bind process references"),
        "{err}"
    );
}

#[test]
fn admission_accepts_for_each_inside_runtime_if_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: list_type,
                value: artifact_value("List[True]"),
            },
            max_items: 1,
            body: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
        else_actions: Vec::new(),
    }];

    artifact
        .validate()
        .expect("for_each inside runtime if branch should pass admission");
}

#[test]
fn admission_rejects_nested_for_each_inside_runtime_if_branch_loop() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: list_type,
                value: artifact_value("List[True]"),
            },
            max_items: 1,
            body: vec![ArtifactAction::ForEach {
                element: ArtifactLoopElement {
                    id: LoopElementId::new(1),
                    ty: bool_type,
                },
                collection: ArtifactValueTemplate::Literal {
                    ty: list_type,
                    value: artifact_value("List[True]"),
                },
                max_items: 1,
                body: vec![ArtifactAction::Emit {
                    output: OutputId::new(0),
                }],
            }],
        }],
        else_actions: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("nested for_each inside branch-contained loop should fail admission");
    assert!(
        err.to_string()
            .contains("nested for loops are not supported"),
        "{err}"
    );
}

#[test]
fn admission_rejects_spawn_inside_runtime_if_branch_loop() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Spawn];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::IfElse {
        condition: ArtifactValueTemplate::Literal {
            ty: bool_type,
            value: artifact_value("True"),
        },
        then_actions: vec![ArtifactAction::ForEach {
            element: ArtifactLoopElement {
                id: LoopElementId::new(0),
                ty: bool_type,
            },
            collection: ArtifactValueTemplate::Literal {
                ty: list_type,
                value: artifact_value("List[True]"),
            },
            max_items: 1,
            body: vec![ArtifactAction::Spawn {
                target: ProcessId::new(1),
                process_ref: ProcessRefId::new(0),
            }],
        }],
        else_actions: Vec::new(),
    }];

    let err = artifact
        .validate()
        .expect_err("spawn inside branch-contained loop should fail admission");
    assert!(
        err.to_string()
            .contains("for loop body cannot bind process references"),
        "{err}"
    );
}
