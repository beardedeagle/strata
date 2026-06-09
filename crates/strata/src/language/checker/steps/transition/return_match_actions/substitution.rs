use crate::language::ast::{ListValue, MapValue, MapValueEntry, RecordValue, RecordValueField};

use super::*;

pub(super) fn substitute_static_arm_bindings(
    value: ValueExpr,
    bindings: &[StaticArmSubstitution<'_>],
) -> ValueExpr {
    if bindings.is_empty() {
        return value;
    }
    match value {
        ValueExpr::Identifier(name) => bindings
            .iter()
            .find_map(|binding| (binding.name == &name).then(|| binding.value.clone()))
            .unwrap_or(ValueExpr::Identifier(name)),
        ValueExpr::StringLiteral(_) | ValueExpr::BytesLiteral(_) | ValueExpr::ScalarLiteral(_) => {
            value
        }
        ValueExpr::Call { name, arg } => ValueExpr::Call {
            name,
            arg: Box::new(substitute_static_arm_bindings(*arg, bindings)),
        },
        ValueExpr::EnumVariant { name, payload } => ValueExpr::EnumVariant {
            name,
            payload: Box::new(substitute_static_arm_bindings(*payload, bindings)),
        },
        ValueExpr::Record(record) => ValueExpr::Record(RecordValue {
            name: record.name,
            fields: record
                .fields
                .into_iter()
                .map(|field| RecordValueField {
                    name: field.name,
                    value: substitute_static_arm_bindings(field.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::List(list) => ValueExpr::List(ListValue {
            element_type: list.element_type,
            capacity: list.capacity,
            items: list
                .items
                .into_iter()
                .map(|item| substitute_static_arm_bindings(item, bindings))
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
                    key: substitute_static_arm_bindings(entry.key, bindings),
                    value: substitute_static_arm_bindings(entry.value, bindings),
                })
                .collect(),
        }),
        ValueExpr::IfElse {
            condition,
            then_branch,
            else_branch,
        } => ValueExpr::IfElse {
            condition: Box::new(substitute_static_arm_bindings(*condition, bindings)),
            then_branch: Box::new(substitute_static_arm_bindings(*then_branch, bindings)),
            else_branch: Box::new(substitute_static_arm_bindings(*else_branch, bindings)),
        },
        ValueExpr::Equality {
            operator,
            left,
            right,
        } => ValueExpr::Equality {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::ScalarArithmetic {
            operator,
            left,
            right,
        } => ValueExpr::ScalarArithmetic {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::ScalarOrdering {
            operator,
            left,
            right,
        } => ValueExpr::ScalarOrdering {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::BooleanNot { operand } => ValueExpr::BooleanNot {
            operand: Box::new(substitute_static_arm_bindings(*operand, bindings)),
        },
        ValueExpr::BooleanBinary {
            operator,
            left,
            right,
        } => ValueExpr::BooleanBinary {
            operator,
            left: Box::new(substitute_static_arm_bindings(*left, bindings)),
            right: Box::new(substitute_static_arm_bindings(*right, bindings)),
        },
        ValueExpr::Grouped { value } => ValueExpr::Grouped {
            value: Box::new(substitute_static_arm_bindings(*value, bindings)),
        },
    }
}
