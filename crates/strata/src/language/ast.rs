use std::fmt;

use mantle_artifact::{MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES};

use super::diagnostic::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);

impl Identifier {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
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

impl TryFrom<&str> for Identifier {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for Identifier {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLiteral(String);

impl OutputLiteral {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_output_literal(&value)?;
        Ok(Self(value))
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

impl TryFrom<&str> for OutputLiteral {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for OutputLiteral {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

fn validate_identifier(value: &str) -> Result<()> {
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(Error::new(format!(
            "identifier exceeds maximum length of {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if is_reserved_identifier(value) {
        return Err(Error::new(format!(
            "identifier {value:?} is reserved for Strata syntax"
        )));
    }
    if is_identifier(value) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "identifier must start with an ASCII letter or '_' and contain only ASCII letters, digits, or '_', got {value:?}"
        )))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_reserved_identifier(value: &str) -> bool {
    matches!(value, "as" | "let" | "mut" | "var")
}

fn validate_output_literal(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::new("output literal must not be empty"));
    }
    if value.len() > MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "output literal exceeds maximum length of {MAX_FIELD_VALUE_BYTES} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::new(
            "output literal must not contain control characters",
        ));
    }
    Ok(())
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
    pub fields: Vec<RecordField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordField {
    pub name: Identifier,
    pub ty: TypeRef,
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
    pub steps: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Identifier,
    pub params: Vec<FunctionParam>,
    pub return_type: TypeRef,
    pub effects: Vec<Effect>,
    pub may: Vec<Identifier>,
    pub determinism: Determinism,
    pub body: Option<FunctionBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionParam {
    Binding(Param),
    Pattern(SignaturePattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignaturePattern {
    Variant(Identifier),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBlock {
    pub statements: Vec<Statement>,
    pub returns: ReturnExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Emit(OutputLiteral),
    LetProcessRef {
        name: Identifier,
        ty: TypeRef,
        target: Identifier,
    },
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
    Value(ValueExpr),
    Call { name: Identifier, arg: ValueExpr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    Identifier(Identifier),
    Record(RecordValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordValue {
    pub name: Identifier,
    pub fields: Vec<RecordValueField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordValueField {
    pub name: Identifier,
    pub value: ValueExpr,
}

impl fmt::Display for ValueExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(name) => write!(f, "{name}"),
            Self::Record(value) => write!(f, "{value}"),
        }
    }
}

impl fmt::Display for RecordValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{{", self.name)?;
        for (index, field) in self.fields.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}:{}", field.name, field.value)?;
        }
        f.write_str("}")
    }
}
