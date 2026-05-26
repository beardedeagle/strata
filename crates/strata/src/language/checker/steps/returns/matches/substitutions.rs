use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StepReturnSubstitution<'a> {
    pub(super) name: &'a Identifier,
    pub(super) value: ValueExpr,
}

pub(super) fn substitute_step_return_bindings(
    value: ValueExpr,
    bindings: &[StepReturnSubstitution<'_>],
) -> ValueExpr {
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find(|binding| binding.name == &name)
            .map(|binding| binding.value.clone())
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::ScalarLiteral(_) => value,
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_step_return_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_step_return_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_step_return_bindings(field.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::List(list) => ValueExpr::List(ListValue {
            element_type: list.element_type,
            capacity: list.capacity,
            items: list
                .items
                .into_iter()
                .map(|item| substitute_step_return_bindings(item, bindings))
                .collect(),
        }),
        ValueExpr::Map(map) => ValueExpr::Map(MapValue {
            key_type: map.key_type,
            value_type: map.value_type,
            capacity: map.capacity,
            entries: map
                .entries
                .into_iter()
                .map(|entry| MapValueEntry {
                    key: substitute_step_return_bindings(entry.key, bindings),
                    value: substitute_step_return_bindings(entry.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => ValueExpr::Equality {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => ValueExpr::ScalarArithmetic {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => ValueExpr::ScalarOrdering {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::BooleanNot { operand } => ValueExpr::BooleanNot {
            operand: Box::new(substitute_step_return_bindings(*operand, bindings)),
        },
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => ValueExpr::BooleanBinary {
            operator,
            left: Box::new(substitute_step_return_bindings(*left, bindings)),
            right: Box::new(substitute_step_return_bindings(*right, bindings)),
        },
        ValueExpr::Grouped { value } => ValueExpr::Grouped {
            value: Box::new(substitute_step_return_bindings(*value, bindings)),
        },
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => ValueExpr::IfElse {
            condition: Box::new(substitute_step_return_bindings(*condition, bindings)),
            then_branch: Box::new(substitute_step_return_bindings(*then_branch, bindings)),
            else_branch: Box::new(substitute_step_return_bindings(*else_branch, bindings)),
        },
    }
}

pub(super) fn substitute_step_return_statements(
    statements: &[Statement],
    bindings: &[StepReturnSubstitution<'_>],
) -> Vec<Statement> {
    if statements.is_empty() {
        return Vec::new();
    }
    if bindings.is_empty() {
        return statements.to_vec();
    }
    statements
        .iter()
        .cloned()
        .map(|statement| substitute_step_return_statement(statement, bindings))
        .collect()
}

fn substitute_step_return_statement(
    statement: Statement,
    bindings: &[StepReturnSubstitution<'_>],
) -> Statement {
    match statement {
        Statement::Emit(_)
        | Statement::LetValue { .. }
        | Statement::LetProcessRef { .. }
        | Statement::LetSpawnOutcome { .. } => statement,
        Statement::ForEach {
            item,
            collection,
            body,
        } => {
            let body = substitute_step_return_for_each_body(&item, body, bindings);
            Statement::ForEach {
                item,
                collection: substitute_step_return_bindings(collection, bindings),
                body,
            }
        }
        Statement::IfElse {
            condition,
            then_body,
            else_body,
        } => Statement::IfElse {
            condition: substitute_step_return_bindings(condition, bindings),
            then_body: substitute_step_return_statement_vec(then_body, bindings),
            else_body: substitute_step_return_statement_vec(else_body, bindings),
        },
        Statement::Send {
            target,
            message,
            payload,
        } => Statement::Send {
            target,
            message,
            payload: payload.map(|payload| substitute_step_return_bindings(payload, bindings)),
        },
        Statement::LetSendOutcome {
            name,
            ty,
            target,
            message,
            payload,
        } => Statement::LetSendOutcome {
            name,
            ty,
            target,
            message,
            payload: payload.map(|payload| substitute_step_return_bindings(payload, bindings)),
        },
    }
}

fn substitute_step_return_for_each_body(
    item: &ForEachItem,
    body: Vec<Statement>,
    bindings: &[StepReturnSubstitution<'_>],
) -> Vec<Statement> {
    if bindings
        .iter()
        .all(|binding| !for_each_item_binds_name(item, binding.name))
    {
        return substitute_step_return_statement_vec(body, bindings);
    }
    let filtered = bindings
        .iter()
        .filter(|binding| !for_each_item_binds_name(item, binding.name))
        .cloned()
        .collect::<Vec<_>>();
    substitute_step_return_statement_vec(body, &filtered)
}

fn for_each_item_binds_name(item: &ForEachItem, name: &Identifier) -> bool {
    match item {
        ForEachItem::Binding(item) => item == name,
        ForEachItem::RecordPattern { fields, .. } => {
            fields.iter().any(|field| field.binding == *name)
        }
    }
}

fn substitute_step_return_statement_vec(
    statements: Vec<Statement>,
    bindings: &[StepReturnSubstitution<'_>],
) -> Vec<Statement> {
    statements
        .into_iter()
        .map(|statement| substitute_step_return_statement(statement, bindings))
        .collect()
}
