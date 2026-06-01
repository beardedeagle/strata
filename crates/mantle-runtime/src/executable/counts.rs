use crate::program::{LoadedAction, LoadedNextState, LoadedProgram, LoadedValueTemplate};

pub(super) fn count_loaded_actions(loaded: &LoadedProgram) -> usize {
    loaded
        .processes
        .iter()
        .flat_map(|process| &process.transitions)
        .map(|transition| count_loaded_action_block(&transition.actions))
        .sum()
}

pub(super) fn count_loaded_action_block(actions: &[LoadedAction]) -> usize {
    actions
        .iter()
        .map(|action| {
            1 + match action {
                LoadedAction::IfElse {
                    then_actions,
                    else_actions,
                    ..
                } => {
                    count_loaded_action_block(then_actions)
                        + count_loaded_action_block(else_actions)
                }
                LoadedAction::ForEach { body, .. } => count_loaded_action_block(body),
                LoadedAction::Emit { .. }
                | LoadedAction::Spawn { .. }
                | LoadedAction::SpawnOutcome { .. }
                | LoadedAction::Send { .. }
                | LoadedAction::SendOutcome { .. } => 0,
            }
        })
        .sum()
}

pub(super) fn count_loaded_program_templates(program: &LoadedProgram) -> usize {
    program
        .processes
        .iter()
        .flat_map(|process| &process.transitions)
        .map(|transition| {
            count_loaded_next_state_templates(&transition.next_state)
                + transition
                    .actions
                    .iter()
                    .map(count_loaded_action_templates)
                    .sum::<usize>()
        })
        .sum()
}

fn count_loaded_next_state_templates(next_state: &LoadedNextState) -> usize {
    match next_state {
        LoadedNextState::Current | LoadedNextState::Value(_) => 0,
        LoadedNextState::Template(template) => count_loaded_value_templates(template),
        LoadedNextState::IfElse {
            condition,
            then_state,
            else_state,
        } => {
            count_loaded_value_templates(condition)
                + count_loaded_next_state_templates(then_state)
                + count_loaded_next_state_templates(else_state)
        }
    }
}

fn count_loaded_action_templates(action: &LoadedAction) -> usize {
    match action {
        LoadedAction::Send { payload, .. } | LoadedAction::SendOutcome { payload, .. } => {
            payload.as_ref().map_or(0, count_loaded_value_templates)
        }
        LoadedAction::IfElse {
            condition,
            then_actions,
            else_actions,
        } => {
            count_loaded_value_templates(condition)
                + then_actions
                    .iter()
                    .map(count_loaded_action_templates)
                    .sum::<usize>()
                + else_actions
                    .iter()
                    .map(count_loaded_action_templates)
                    .sum::<usize>()
        }
        LoadedAction::ForEach {
            collection, body, ..
        } => {
            count_loaded_value_templates(collection)
                + body
                    .iter()
                    .map(count_loaded_action_templates)
                    .sum::<usize>()
        }
        LoadedAction::Emit { .. }
        | LoadedAction::Spawn { .. }
        | LoadedAction::SpawnOutcome { .. } => 0,
    }
}

fn count_loaded_value_templates(template: &LoadedValueTemplate) -> usize {
    1 + match template {
        LoadedValueTemplate::EnumPayload { value, .. }
        | LoadedValueTemplate::RecordField { record: value, .. }
        | LoadedValueTemplate::ListElement { list: value, .. }
        | LoadedValueTemplate::ListPrefixElement { list: value, .. }
        | LoadedValueTemplate::ListRest { list: value, .. }
        | LoadedValueTemplate::MapValue { map: value, .. }
        | LoadedValueTemplate::MapRest { map: value, .. }
        | LoadedValueTemplate::EnumVariant { payload: value, .. }
        | LoadedValueTemplate::BooleanNot { operand: value, .. } => {
            count_loaded_value_templates(value)
        }
        LoadedValueTemplate::Record { fields, .. } => fields
            .iter()
            .map(|field| count_loaded_value_templates(&field.value))
            .sum(),
        LoadedValueTemplate::List { items, .. } => {
            items.iter().map(count_loaded_value_templates).sum()
        }
        LoadedValueTemplate::Map { entries, .. } => entries
            .iter()
            .map(|entry| {
                count_loaded_value_templates(&entry.key)
                    + count_loaded_value_templates(&entry.value)
            })
            .sum(),
        LoadedValueTemplate::IfElse {
            condition,
            then_value,
            else_value,
            ..
        } => {
            count_loaded_value_templates(condition)
                + count_loaded_value_templates(then_value)
                + count_loaded_value_templates(else_value)
        }
        LoadedValueTemplate::Equality { left, right, .. }
        | LoadedValueTemplate::ScalarArithmetic { left, right, .. }
        | LoadedValueTemplate::ScalarOrdering { left, right, .. }
        | LoadedValueTemplate::BooleanBinary { left, right, .. } => {
            count_loaded_value_templates(left) + count_loaded_value_templates(right)
        }
        LoadedValueTemplate::Literal { .. }
        | LoadedValueTemplate::ReceivedPayload { .. }
        | LoadedValueTemplate::CurrentStatePayload { .. }
        | LoadedValueTemplate::ProcessRef { .. }
        | LoadedValueTemplate::LoopElement { .. }
        | LoadedValueTemplate::EffectOutcome { .. } => 0,
    }
}
