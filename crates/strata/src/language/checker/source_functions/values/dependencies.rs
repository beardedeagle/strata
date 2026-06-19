use super::*;

pub(super) fn source_value_uses_any_binding(
    value: &ValueExpr,
    bindings: &[SourceValueBinding<'_>],
) -> bool {
    bindings
        .iter()
        .any(|binding| source_value_uses_binding(value, binding.name))
}

pub(super) fn source_value_requires_resolution(value: &ValueExpr) -> bool {
    match value {
        ValueExpr::Identifier(_)
        | ValueExpr::StringLiteral(_)
        | ValueExpr::BytesLiteral(_)
        | ValueExpr::ScalarLiteral(_) => false,
        ValueExpr::Call { .. } => true,
        ValueExpr::EnumVariant { payload, .. } => source_value_requires_resolution(payload),
        ValueExpr::Record(record) => record
            .fields
            .iter()
            .any(|field| source_value_requires_resolution(&field.value)),
        ValueExpr::List(list) => list.items.iter().any(source_value_requires_resolution),
        ValueExpr::Map(map) => map.entries.iter().any(|entry| {
            source_value_requires_resolution(&entry.key)
                || source_value_requires_resolution(&entry.value)
        }),
        ValueExpr::IfElse { .. }
        | ValueExpr::Equality { .. }
        | ValueExpr::ScalarArithmetic { .. }
        | ValueExpr::ScalarOrdering { .. }
        | ValueExpr::BooleanNot { .. }
        | ValueExpr::BooleanBinary { .. }
        | ValueExpr::Grouped { .. } => true,
    }
}
