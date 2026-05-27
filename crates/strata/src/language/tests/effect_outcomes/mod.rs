use super::support::*;

mod branch_patterns;
mod static_rejections;

const EFFECT_OUTCOMES: &str = r#"
module effect_outcomes;

record MainState { outcome: Result<Unit,SendError<WorkerMsg>> }
enum MainMsg { Start, SpawnOnly }
enum Bool { False, True }
enum WorkerState { Idle }
enum WorkerMsg { Ping }

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    authority spawn_worker: Cap<Spawn<Worker>>;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState { outcome: Ok(Unit) };
    }

    fn step(state: MainState, Start) -> ProcResult<MainState> ! [spawn, send] ~ [] @det {
        let worker: ProcessRef<Worker> = spawn Worker;
        let sent: Result<Unit,SendError<WorkerMsg>> = send worker Ping;
        return Stop(MainState { outcome: sent });
    }

    fn step(state: MainState, SpawnOnly) -> ProcResult<MainState> ! [spawn] ~ [] @det {
        let spawned: Result<ProcessRef<Worker>,SpawnError<Unit>> = spawn Worker;
        return Stop(state);
    }
}

proc Worker mailbox bounded(1) {
    type State = WorkerState;
    type Msg = WorkerMsg;

    fn init() -> WorkerState ! [] ~ [] @det {
        return Idle;
    }

    fn step(state: WorkerState, Ping) -> ProcResult<WorkerState> ! [] ~ [] @det {
        return Stop(state);
    }
}
"#;

fn contains_effect_outcome_template(next_state: &NextState) -> bool {
    match next_state {
        NextState::Template(template) => template_contains_effect_outcome(template),
        NextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            template_contains_effect_outcome(condition)
                || contains_effect_outcome_template(then_state)
                || contains_effect_outcome_template(else_state)
        }
        NextState::Current | NextState::Value(_) => false,
    }
}

fn template_contains_effect_outcome(template: &ArtifactValueTemplate) -> bool {
    match template {
        ArtifactValueTemplate::EffectOutcome { .. } => true,
        ArtifactValueTemplate::EnumPayload { value, .. }
        | ArtifactValueTemplate::RecordField { record: value, .. }
        | ArtifactValueTemplate::ListElement { list: value, .. }
        | ArtifactValueTemplate::ListPrefixElement { list: value, .. }
        | ArtifactValueTemplate::ListRest { list: value, .. }
        | ArtifactValueTemplate::MapValue { map: value, .. }
        | ArtifactValueTemplate::MapRest { map: value, .. }
        | ArtifactValueTemplate::EnumVariant { payload: value, .. }
        | ArtifactValueTemplate::BooleanNot { operand: value, .. } => {
            template_contains_effect_outcome(value)
        }
        ArtifactValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            template_contains_effect_outcome(condition)
                || template_contains_effect_outcome(then_value)
                || template_contains_effect_outcome(else_value)
        }
        ArtifactValueTemplate::Record { fields, .. } => fields
            .iter()
            .any(|field| template_contains_effect_outcome(&field.value)),
        ArtifactValueTemplate::List { items, .. } => {
            items.iter().any(template_contains_effect_outcome)
        }
        ArtifactValueTemplate::Map { entries, .. } => entries.iter().any(|entry| {
            template_contains_effect_outcome(&entry.key)
                || template_contains_effect_outcome(&entry.value)
        }),
        ArtifactValueTemplate::Equality { left, right, .. }
        | ArtifactValueTemplate::ScalarArithmetic { left, right, .. }
        | ArtifactValueTemplate::ScalarOrdering { left, right, .. }
        | ArtifactValueTemplate::BooleanBinary { left, right, .. } => {
            template_contains_effect_outcome(left) || template_contains_effect_outcome(right)
        }
        ArtifactValueTemplate::Literal { .. }
        | ArtifactValueTemplate::ReceivedPayload { .. }
        | ArtifactValueTemplate::CurrentStatePayload { .. }
        | ArtifactValueTemplate::ProcessRef { .. }
        | ArtifactValueTemplate::LoopElement { .. } => false,
    }
}
