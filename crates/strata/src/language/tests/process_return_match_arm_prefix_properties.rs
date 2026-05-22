use super::support::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmPrefixKind {
    None,
    Emit,
    Send,
    EmitThenSend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    Spawn,
    Emit,
    Send,
}

const ARM_PREFIX_KINDS: [ArmPrefixKind; 4] = [
    ArmPrefixKind::None,
    ArmPrefixKind::Emit,
    ArmPrefixKind::Send,
    ArmPrefixKind::EmitThenSend,
];

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

fn arm_prefix_property_source(
    module_name: &str,
    ready: ArmPrefixKind,
    done: ArmPrefixKind,
    effects: &[Effect],
) -> String {
    format!(
        r#"
module {module_name};

record MainState;
record SinkState;
enum MainMsg {{ Start }}
enum Phase {{ Ready, Done }}
enum Route {{ Assign(Phase) }}
enum WorkerState {{ Idle, SawReady, Done }}
enum WorkerMsg {{ Envelope(Route) }}
enum SinkMsg {{ Ack }}

proc Main mailbox bounded(1) {{
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {{
        return MainState;
    }}

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {{
        let worker: ProcessRef<Worker> = spawn Worker;
        send worker Envelope(Assign(Ready));
        send worker Envelope(Assign(Done));
        return Stop(state);
    }}
}}

proc Worker mailbox bounded(2) {{
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {{
        return Idle;
    }}

    fn step(state: WorkerState, Envelope(Assign(phase: Phase))) -> ProcResult<WorkerState> ! {effects_source} ~ [] @det {{
        let sink: ProcessRef<Sink> = spawn Sink;
        return match phase {{
            Ready => {{
{ready_statements}                return Continue(SawReady);
            }}
            Done => {{
{done_statements}                return Stop(Done);
            }}
        }};
    }}
}}

proc Sink mailbox bounded(2) {{
    type State = SinkState;
    type Msg = SinkMsg;

    fn init() -> SinkState ! [] ~ [] @det {{
        return SinkState;
    }}

    fn step(state: SinkState, Ack) -> ProcResult<SinkState> ! [] ~ [] @det {{
        return Stop(state);
    }}
}}
"#,
        effects_source = effects_source(effects),
        ready_statements = ready.statements("ready"),
        done_statements = done.statements("done"),
    )
}

fn checked_process<'a>(checked: &'a CheckedProgram, name: &str) -> &'a CheckedProcess {
    checked
        .processes()
        .iter()
        .find(|process| process.debug_name().as_str() == name)
        .unwrap_or_else(|| panic!("checked process {name} should exist"))
}

fn checked_action_kinds(actions: &[CheckedAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            CheckedAction::Spawn { .. } => ActionKind::Spawn,
            CheckedAction::Emit { .. } => ActionKind::Emit,
            CheckedAction::Send { .. } => ActionKind::Send,
            CheckedAction::IfElse { .. } | CheckedAction::ForEach { .. } => {
                panic!("return-match arm prefix property generated unsupported action: {action:?}")
            }
        })
        .collect()
}

fn artifact_action_kinds(actions: &[ArtifactAction]) -> Vec<ActionKind> {
    actions
        .iter()
        .map(|action| match action {
            ArtifactAction::Spawn { .. } => ActionKind::Spawn,
            ArtifactAction::Emit { .. } => ActionKind::Emit,
            ArtifactAction::Send { .. } => ActionKind::Send,
            ArtifactAction::IfElse { .. } | ArtifactAction::ForEach { .. } => {
                panic!("return-match arm prefix property lowered unsupported action: {action:?}")
            }
        })
        .collect()
}

fn assert_artifact_send_actions_use_ids(actions: &[ArtifactAction]) {
    for action in actions {
        let ArtifactAction::Send {
            target, message, ..
        } = action
        else {
            continue;
        };
        assert_eq!(
            *target,
            ArtifactSendTarget::ProcessRef(ProcessRefId::new(0)),
            "arm-local send should lower through a process-ref id, not a source target name"
        );
        assert_eq!(
            *message,
            MessageId::new(0),
            "arm-local send should lower through a message id, not a source message name"
        );
    }
}

fn effects_for_kind(kind: ArmPrefixKind) -> &'static [Effect] {
    match kind {
        ArmPrefixKind::None => &[Effect::Spawn],
        ArmPrefixKind::Emit => &[Effect::Emit, Effect::Spawn],
        ArmPrefixKind::Send => &[Effect::Spawn, Effect::Send],
        ArmPrefixKind::EmitThenSend => &[Effect::Emit, Effect::Spawn, Effect::Send],
    }
}

fn actions_for_kind(kind: ArmPrefixKind) -> Vec<ActionKind> {
    let mut actions = vec![ActionKind::Spawn];
    match kind {
        ArmPrefixKind::None => {}
        ArmPrefixKind::Emit => actions.push(ActionKind::Emit),
        ArmPrefixKind::Send => actions.push(ActionKind::Send),
        ArmPrefixKind::EmitThenSend => {
            actions.push(ActionKind::Emit);
            actions.push(ActionKind::Send);
        }
    }
    actions
}

fn union_effects(left: ArmPrefixKind, right: ArmPrefixKind) -> Vec<Effect> {
    let mut effects = vec![Effect::Spawn];
    if [left, right].iter().any(|kind| kind.uses_emit()) {
        effects.insert(0, Effect::Emit);
    }
    if [left, right].iter().any(|kind| kind.uses_send()) {
        effects.push(Effect::Send);
    }
    effects
}

fn effects_source(effects: &[Effect]) -> String {
    let effects = effects
        .iter()
        .map(|effect| match effect {
            Effect::Emit => "emit",
            Effect::Spawn => "spawn",
            Effect::Send => "send",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{effects}]")
}

fn artifact_effect_for(effect: Effect) -> ArtifactEffect {
    match effect {
        Effect::Emit => ArtifactEffect::Emit,
        Effect::Spawn => ArtifactEffect::Spawn,
        Effect::Send => ArtifactEffect::Send,
    }
}

impl ArmPrefixKind {
    fn name(self) -> &'static str {
        match self {
            ArmPrefixKind::None => "none",
            ArmPrefixKind::Emit => "emit",
            ArmPrefixKind::Send => "send",
            ArmPrefixKind::EmitThenSend => "emit_send",
        }
    }

    fn statements(self, label: &str) -> String {
        match self {
            ArmPrefixKind::None => String::new(),
            ArmPrefixKind::Emit => format!("                emit \"{label} arm prefix\";\n"),
            ArmPrefixKind::Send => "                send sink Ack;\n".to_string(),
            ArmPrefixKind::EmitThenSend => {
                format!(
                    "                emit \"{label} arm prefix\";\n                send sink Ack;\n"
                )
            }
        }
    }

    fn uses_emit(self) -> bool {
        matches!(self, ArmPrefixKind::Emit | ArmPrefixKind::EmitThenSend)
    }

    fn uses_send(self) -> bool {
        matches!(self, ArmPrefixKind::Send | ArmPrefixKind::EmitThenSend)
    }
}
