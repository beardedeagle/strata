use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);

impl Identifier {
    pub(super) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLiteral(String);

impl OutputLiteral {
    pub(super) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: Identifier,
    pub records: Vec<Record>,
    pub enums: Vec<Enum>,
    pub processes: Vec<Process>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: Identifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Identifier,
    pub variants: Vec<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: Identifier,
    pub mailbox_bound: usize,
    pub state_type: TypeRef,
    pub msg_type: TypeRef,
    pub init: Function,
    pub step: Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Identifier,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub effects: Vec<Effect>,
    pub may: Vec<Identifier>,
    pub determinism: Determinism,
    pub body: Option<FunctionBody>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBody {
    pub statements: Vec<Statement>,
    pub returns: ReturnExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Emit(OutputLiteral),
    Spawn(Identifier),
    Send {
        target: Identifier,
        message: Identifier,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Identifier,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Named(Identifier),
    Applied {
        constructor: Identifier,
        args: Vec<TypeRef>,
    },
}

impl TypeRef {
    pub(super) fn named(name: Identifier) -> Self {
        Self::Named(name)
    }

    pub(super) fn as_named(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name.as_str()),
            Self::Applied { .. } => None,
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => f.write_str(name.as_str()),
            Self::Applied { constructor, args } => {
                write!(f, "{constructor}<")?;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(">")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Determinism {
    Det,
    Nondet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    Emit,
    Spawn,
    Send,
}

impl Effect {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "emit" => Some(Self::Emit),
            "spawn" => Some(Self::Spawn),
            "send" => Some(Self::Send),
            _ => None,
        }
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Emit => f.write_str("emit"),
            Self::Spawn => f.write_str("spawn"),
            Self::Send => f.write_str("send"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnExpr {
    Identifier(Identifier),
    Call { name: Identifier, arg: Identifier },
}
