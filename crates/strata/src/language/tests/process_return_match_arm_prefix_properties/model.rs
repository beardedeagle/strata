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
    SpawnOutcome,
    Emit,
    Send,
    SendOutcome,
    IfElse,
    ForEach,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArmActionBlockKind {
    MultipleIf,
    MultipleFor,
    IfWithFor,
    IfWithForNestedLoopIf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundedArmStatementKind {
    IfElse,
    ForEach,
    IfWithFor,
    ForWithIf,
}

const ARM_PREFIX_KINDS: [ArmPrefixKind; 4] = [
    ArmPrefixKind::None,
    ArmPrefixKind::Emit,
    ArmPrefixKind::Send,
    ArmPrefixKind::EmitThenSend,
];

const ARM_ACTION_BLOCK_KINDS: [ArmActionBlockKind; 4] = [
    ArmActionBlockKind::MultipleIf,
    ArmActionBlockKind::MultipleFor,
    ArmActionBlockKind::IfWithFor,
    ArmActionBlockKind::IfWithForNestedLoopIf,
];

const BOUNDED_ARM_STATEMENT_KINDS: [BoundedArmStatementKind; 4] = [
    BoundedArmStatementKind::IfElse,
    BoundedArmStatementKind::ForEach,
    BoundedArmStatementKind::IfWithFor,
    BoundedArmStatementKind::ForWithIf,
];
