use super::*;

impl ArtifactValueTemplate {
    pub(in crate::artifact) fn depends_on_received_payload(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => true,
            Self::CurrentStatePayload { .. } => false,
            Self::EnumPayload { value, .. } => value.depends_on_received_payload(),
            Self::RecordField { record, .. } => record.depends_on_received_payload(),
            Self::ListElement { list, .. }
            | Self::ListPrefixElement { list, .. }
            | Self::ListRest { list, .. } => list.depends_on_received_payload(),
            Self::MapValue { map, .. } => map.depends_on_received_payload(),
            Self::MapRest { map, .. } => map.depends_on_received_payload(),
            Self::ProcessRef { .. } => false,
            Self::LoopElement { .. } => false,
            Self::EnumVariant { payload, .. } => payload.depends_on_received_payload(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_received_payload()),
            Self::List { items, .. } => items.iter().any(Self::depends_on_received_payload),
            Self::Map { entries, .. } => entries.iter().any(|entry| {
                entry.key.depends_on_received_payload() || entry.value.depends_on_received_payload()
            }),
            Self::IfElse {
                condition,
                then_value,
                else_value,
                ..
            } => {
                condition.depends_on_received_payload()
                    || then_value.depends_on_received_payload()
                    || else_value.depends_on_received_payload()
            }
            Self::Equality { left, right, .. }
            | Self::ScalarArithmetic { left, right, .. }
            | Self::ScalarOrdering { left, right, .. } => {
                left.depends_on_received_payload() || right.depends_on_received_payload()
            }
            Self::BooleanNot { operand, .. } => operand.depends_on_received_payload(),
            Self::BooleanBinary { left, right, .. } => {
                left.depends_on_received_payload() || right.depends_on_received_payload()
            }
        }
    }

    pub(in crate::artifact) fn depends_on_loop_element(&self) -> bool {
        match self {
            Self::Literal { .. } => false,
            Self::ReceivedPayload { .. } => false,
            Self::CurrentStatePayload { .. } => false,
            Self::EnumPayload { value, .. } => value.depends_on_loop_element(),
            Self::RecordField { record, .. } => record.depends_on_loop_element(),
            Self::ListElement { list, .. }
            | Self::ListPrefixElement { list, .. }
            | Self::ListRest { list, .. } => list.depends_on_loop_element(),
            Self::MapValue { map, .. } => map.depends_on_loop_element(),
            Self::MapRest { map, .. } => map.depends_on_loop_element(),
            Self::ProcessRef { .. } => false,
            Self::LoopElement { .. } => true,
            Self::EnumVariant { payload, .. } => payload.depends_on_loop_element(),
            Self::Record { fields, .. } => fields
                .iter()
                .any(|field| field.value.depends_on_loop_element()),
            Self::List { items, .. } => items.iter().any(Self::depends_on_loop_element),
            Self::Map { entries, .. } => entries.iter().any(|entry| {
                entry.key.depends_on_loop_element() || entry.value.depends_on_loop_element()
            }),
            Self::IfElse {
                condition,
                then_value,
                else_value,
                ..
            } => {
                condition.depends_on_loop_element()
                    || then_value.depends_on_loop_element()
                    || else_value.depends_on_loop_element()
            }
            Self::Equality { left, right, .. }
            | Self::ScalarArithmetic { left, right, .. }
            | Self::ScalarOrdering { left, right, .. } => {
                left.depends_on_loop_element() || right.depends_on_loop_element()
            }
            Self::BooleanNot { operand, .. } => operand.depends_on_loop_element(),
            Self::BooleanBinary { left, right, .. } => {
                left.depends_on_loop_element() || right.depends_on_loop_element()
            }
        }
    }
}
