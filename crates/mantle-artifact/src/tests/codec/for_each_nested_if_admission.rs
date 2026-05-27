use super::super::support::*;

#[test]
fn admission_accepts_one_nested_if_else_inside_for_each_loop_branch() {
    let mut artifact = valid_artifact();
    let bool_type = append_bool_type(&mut artifact);
    let list_type = append_list_type(&mut artifact, "BoolList", bool_type, 1);
    artifact.processes[0].authorities = Vec::new();
    artifact.processes[0].spawn_sites = Vec::new();
    let condition = ArtifactValueTemplate::LoopElement {
        ty: bool_type,
        element: LoopElementId::new(0),
    };
    artifact.processes[0].transitions[0].effects = vec![ArtifactEffect::Emit];
    artifact.processes[0].transitions[0].actions = vec![ArtifactAction::ForEach {
        element: ArtifactLoopElement {
            id: LoopElementId::new(0),
            ty: bool_type,
        },
        collection: ArtifactValueTemplate::Literal {
            ty: list_type,
            value: artifact_value("List[True]"),
        },
        max_items: 1,
        body: vec![ArtifactAction::IfElse {
            condition: condition.clone(),
            then_actions: vec![ArtifactAction::IfElse {
                condition,
                then_actions: vec![ArtifactAction::Emit {
                    output: OutputId::new(0),
                }],
                else_actions: vec![ArtifactAction::Emit {
                    output: OutputId::new(0),
                }],
            }],
            else_actions: vec![ArtifactAction::Emit {
                output: OutputId::new(0),
            }],
        }],
    }];

    artifact
        .validate()
        .expect("one nested if_else inside loop branch should admit");
    let encoded = artifact.encode();
    let decoded = MantleArtifact::decode(&encoded).expect("nested loop branch artifact decodes");

    assert_eq!(decoded, artifact);
    assert!(encoded.contains("process.0.transition.0.action.0.kind=for_each"));
    assert!(encoded.contains("process.0.transition.0.action.0.body_action.0.kind=if_else"));
    assert!(
        encoded
            .contains("process.0.transition.0.action.0.body_action.0.then_action.0.kind=if_else")
    );
}
