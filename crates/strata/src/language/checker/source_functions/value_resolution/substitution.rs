use super::*;

pub(super) fn substitute_source_value_bindings(
    value: ValueExpr,
    bindings: &[SourceSubstitution],
) -> ValueExpr {
    match value {
        ValueExpr::StringLiteral(_) | ValueExpr::BytesLiteral(_) | ValueExpr::ScalarLiteral(_) => {
            value
        }
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|binding| (name == binding.name).then(|| binding.value.clone()))
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_source_value_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_source_value_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_source_value_bindings(field.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::List(list) => ValueExpr::List(ListValue {
            element_type: list.element_type,
            capacity: list.capacity,
            items: list
                .items
                .into_iter()
                .map(|item| substitute_source_value_bindings(item, bindings))
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
                    key: substitute_source_value_bindings(entry.key, bindings),
                    value: substitute_source_value_bindings(entry.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => ValueExpr::IfElse {
            condition: Box::new(substitute_source_value_bindings(*condition, bindings)),
            then_branch: Box::new(substitute_source_value_bindings(*then_branch, bindings)),
            else_branch: Box::new(substitute_source_value_bindings(*else_branch, bindings)),
        },
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => ValueExpr::Equality {
            operator,
            left: Box::new(substitute_source_value_bindings(*left, bindings)),
            right: Box::new(substitute_source_value_bindings(*right, bindings)),
        },
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => ValueExpr::ScalarArithmetic {
            operator,
            left: Box::new(substitute_source_value_bindings(*left, bindings)),
            right: Box::new(substitute_source_value_bindings(*right, bindings)),
        },
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => ValueExpr::ScalarOrdering {
            operator,
            left: Box::new(substitute_source_value_bindings(*left, bindings)),
            right: Box::new(substitute_source_value_bindings(*right, bindings)),
        },
        ValueExpr::BooleanNot { operand } => ValueExpr::BooleanNot {
            operand: Box::new(substitute_source_value_bindings(*operand, bindings)),
        },
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => ValueExpr::BooleanBinary {
            operator,
            left: Box::new(substitute_source_value_bindings(*left, bindings)),
            right: Box::new(substitute_source_value_bindings(*right, bindings)),
        },
        ValueExpr::Grouped { value } => ValueExpr::Grouped {
            value: Box::new(substitute_source_value_bindings(*value, bindings)),
        },
    }
}
