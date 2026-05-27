use super::support::*;

#[test]
fn runtime_rejects_loaded_dynamic_for_each_non_list_collection_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    grant_loaded_main_spawn_authority(&mut program);
    program.processes[1].message_variants[0].payload_type = Some(JOB);
    align_loaded_process_message_type(&mut program, 1);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Spawn,
            ArtifactEffect::Send,
        ]);
    program.processes[0].transitions[0].actions = vec![
        LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_SITE,
        },
        LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(loaded_template(ArtifactValueTemplate::Literal {
                ty: JOB,
                value: artifact_value("Job{phase:Ready}"),
            })),
        },
    ];
    program.processes[1].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[]);
    program.processes[1].transitions[0].actions = vec![LoadedAction::ForEach {
        element: crate::program::LoadedLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: LoadedValueTemplate::ReceivedPayload { ty: JOB },
        max_items: 1,
        body: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker transition 0 for collection type id 4 must be a list type",
    );
}

#[test]
fn runtime_rejects_loaded_dynamic_for_each_collection_element_mismatch_before_artifact_loaded() {
    let artifact = artifact_with_unbound_worker_process_ref();
    let mut program = LoadedProgram::from_artifact(&artifact).expect("artifact should load");
    grant_loaded_main_spawn_authority(&mut program);
    let box_list = push_list_type(&mut program, "BoxList", BOX, 1);
    program.processes[1].message_variants[0].payload_type = Some(box_list);
    align_loaded_process_message_type(&mut program, 1);
    program.processes[0].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[
            ArtifactEffect::Spawn,
            ArtifactEffect::Send,
        ]);
    program.processes[0].transitions[0].actions = vec![
        LoadedAction::Spawn {
            target: ProcessId::new(1),
            process_ref: ProcessRefId::new(0),
            spawn_site: SPAWN_SITE,
        },
        LoadedAction::Send {
            target: LoadedSendTarget::ProcessRef(ProcessRefId::new(0)),
            message: MessageId::new(0),
            payload: Some(LoadedValueTemplate::List {
                ty: box_list,
                items: vec![LoadedValueTemplate::Literal {
                    ty: BOX,
                    value: RuntimeValue::Atom("Box".to_string()),
                }],
            }),
        },
    ];
    program.processes[1].transitions[0].effect_authority =
        crate::program::LoadedEffectAuthority::from_artifact(&[]);
    program.processes[1].transitions[0].actions = vec![LoadedAction::ForEach {
        element: crate::program::LoadedLoopElement {
            id: LoopElementId::new(0),
            ty: JOB,
        },
        collection: LoadedValueTemplate::ReceivedPayload { ty: box_list },
        max_items: 1,
        body: Vec::new(),
    }];

    assert_loaded_admission_rejects_before_artifact_loaded(
        &program,
        "process Worker transition 0 for collection element type id 5, expected 4",
    );
}
