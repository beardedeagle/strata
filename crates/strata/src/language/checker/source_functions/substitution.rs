use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::language::checker::source_functions) struct SourceSubstitution {
    pub(in crate::language::checker::source_functions) name: Identifier,
    pub(in crate::language::checker::source_functions) value: ValueExpr,
}

impl SourceSubstitution {
    pub(in crate::language::checker::source_functions) fn new(
        name: Identifier,
        value: ValueExpr,
    ) -> Self {
        Self { name, value }
    }
}
