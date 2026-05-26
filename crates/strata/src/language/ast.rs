use std::fmt;

use mantle_artifact::{ArtifactScalarValue, MAX_FIELD_VALUE_BYTES, MAX_IDENTIFIER_BYTES};

use super::diagnostic::{Error, Result};

mod scalar;

pub use scalar::{ValueScalarArithmeticOperator, ValueScalarOrderingOperator};

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
    matches!(
        value,
        "_" | "as"
            | "bounded"
            | "else"
            | "emit"
            | "enum"
            | "fn"
            | "for"
            | "if"
            | "in"
            | "let"
            | "mailbox"
            | "match"
            | "module"
            | "mut"
            | "proc"
            | "record"
            | "return"
            | "security"
            | "send"
            | "spawn"
            | "type"
            | "var"
    )
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
    pub functions: Vec<Function>,
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
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub payload_type: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: Identifier,
    pub mailbox_bound: usize,
    pub state_type: TypeRef,
    pub msg_type: TypeRef,
    pub init: Function,
    pub functions: Vec<Function>,
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
    pub body: Option<FunctionBody>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionParam {
    Binding(Param),
    Pattern(Pattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Constructor {
        name: Identifier,
        payload: Option<ConstructorPayloadPattern>,
    },
    Record {
        name: Identifier,
        fields: Vec<RecordPatternField>,
    },
    List(ListPattern),
    Map(MapPattern),
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorPayloadPattern {
    Binding(Param),
    Destructure(Box<Pattern>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPatternField {
    pub field: Identifier,
    pub binding: Identifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPattern {
    pub element_type: Option<TypeRef>,
    pub capacity: Option<usize>,
    pub elements: Vec<CollectionPatternBinding>,
    pub rest: Option<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPattern {
    pub key_type: Option<TypeRef>,
    pub value_type: Option<TypeRef>,
    pub capacity: Option<usize>,
    pub completeness: MapPatternCompleteness,
    pub rest: Option<Identifier>,
    pub entries: Vec<MapPatternEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MapPatternCompleteness {
    Exact,
    Subset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPatternEntry {
    pub key: ValueExpr,
    pub binding: CollectionPatternBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionPatternBinding {
    Binding(Identifier),
    Pattern(Box<Pattern>),
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    Block(Box<FunctionBlock>),
    Match(Match),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBlock {
    pub statements: Vec<Statement>,
    pub returns: ReturnExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub scrutinee: Identifier,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: FunctionBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Emit(OutputLiteral),
    LetValue {
        name: Identifier,
        ty: TypeRef,
        value: ValueExpr,
    },
    LetProcessRef {
        name: Identifier,
        ty: TypeRef,
        target: Identifier,
    },
    LetSpawnOutcome {
        name: Identifier,
        ty: TypeRef,
        target: Identifier,
    },
    Send {
        target: Identifier,
        message: Identifier,
        payload: Option<ValueExpr>,
    },
    LetSendOutcome {
        name: Identifier,
        ty: TypeRef,
        target: Identifier,
        message: Identifier,
        payload: Option<ValueExpr>,
    },
    IfElse {
        condition: ValueExpr,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    ForEach {
        item: ForEachItem,
        collection: ValueExpr,
        body: Vec<Statement>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForEachItem {
    Binding(Identifier),
    RecordPattern {
        name: Identifier,
        fields: Vec<RecordPatternField>,
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
        const_args: Vec<usize>,
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
            Self::Applied {
                constructor,
                args,
                const_args,
            } => {
                write!(f, "{constructor}<")?;
                let mut needs_comma = false;
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{arg}")?;
                    needs_comma = true;
                }
                for value in const_args {
                    if needs_comma {
                        f.write_str(",")?;
                    }
                    write!(f, "{value}")?;
                    needs_comma = true;
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
    Call {
        name: Identifier,
        arg: ValueExpr,
    },
    Match(Match),
    IfElse {
        condition: ValueExpr,
        then_branch: Box<FunctionBlock>,
        else_branch: Box<FunctionBlock>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    Identifier(Identifier),
    ScalarLiteral(ArtifactScalarValue),
    Call {
        name: Identifier,
        arg: Box<ValueExpr>,
    },
    EnumVariant {
        name: Identifier,
        payload: Box<ValueExpr>,
    },
    Record(RecordValue),
    List(ListValue),
    Map(MapValue),
    IfElse {
        condition: Box<ValueExpr>,
        then_branch: Box<ValueExpr>,
        else_branch: Box<ValueExpr>,
    },
    Equality {
        operator: ValueEqualityOperator,
        left: Box<ValueExpr>,
        right: Box<ValueExpr>,
    },
    ScalarArithmetic {
        operator: ValueScalarArithmeticOperator,
        left: Box<ValueExpr>,
        right: Box<ValueExpr>,
    },
    ScalarOrdering {
        operator: ValueScalarOrderingOperator,
        left: Box<ValueExpr>,
        right: Box<ValueExpr>,
    },
    BooleanNot {
        operand: Box<ValueExpr>,
    },
    BooleanBinary {
        operator: ValueBooleanOperator,
        left: Box<ValueExpr>,
        right: Box<ValueExpr>,
    },
    Grouped {
        value: Box<ValueExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueEqualityOperator {
    Equal,
    NotEqual,
}

impl ValueEqualityOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueBooleanOperator {
    And,
    Or,
}

impl ValueBooleanOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "&&",
            Self::Or => "||",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListValue {
    pub element_type: Option<TypeRef>,
    pub capacity: Option<usize>,
    pub items: Vec<ValueExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapValue {
    pub key_type: Option<TypeRef>,
    pub value_type: Option<TypeRef>,
    pub capacity: Option<usize>,
    pub entries: Vec<MapValueEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapValueEntry {
    pub key: ValueExpr,
    pub value: ValueExpr,
}

impl fmt::Display for ValueExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_min_precedence(f, 0)
    }
}

impl ValueExpr {
    fn fmt_with_min_precedence(
        &self,
        f: &mut fmt::Formatter<'_>,
        min_precedence: u8,
    ) -> fmt::Result {
        let precedence = self.display_precedence();
        let needs_parens = precedence < min_precedence;
        if needs_parens {
            f.write_str("(")?;
        }
        match self {
            Self::Identifier(name) => write!(f, "{name}"),
            Self::ScalarLiteral(value) => write!(f, "{}", value.label()),
            Self::Call { name, arg } => write!(f, "{name}({arg})"),
            Self::EnumVariant { name, payload } => write!(f, "{name}({payload})"),
            Self::Record(value) => write!(f, "{value}"),
            Self::List(value) => write!(f, "{value}"),
            Self::Map(value) => write!(f, "{value}"),
            Self::IfElse {
                condition,
                then_branch,
                else_branch,
            } => write!(f, "if({condition}){{{then_branch}}}else{{{else_branch}}}"),
            Self::Equality {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::ScalarArithmetic {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::ScalarOrdering {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::BooleanNot { operand } => {
                f.write_str("!")?;
                operand.fmt_with_min_precedence(f, precedence)
            }
            Self::BooleanBinary {
                operator,
                left,
                right,
            } => {
                left.fmt_with_min_precedence(f, precedence)?;
                write!(f, " {} ", operator.as_str())?;
                right.fmt_with_min_precedence(f, precedence + 1)
            }
            Self::Grouped { value } => {
                f.write_str("(")?;
                value.fmt_with_min_precedence(f, 0)?;
                f.write_str(")")
            }
        }?;
        if needs_parens {
            f.write_str(")")?;
        }
        Ok(())
    }

    fn display_precedence(&self) -> u8 {
        match self {
            Self::BooleanBinary {
                operator: ValueBooleanOperator::Or,
                ..
            } => 1,
            Self::BooleanBinary {
                operator: ValueBooleanOperator::And,
                ..
            } => 2,
            Self::Equality { .. } => 3,
            Self::ScalarOrdering { .. } => 4,
            Self::ScalarArithmetic {
                operator:
                    ValueScalarArithmeticOperator::Add | ValueScalarArithmeticOperator::Subtract,
                ..
            } => 5,
            Self::ScalarArithmetic {
                operator:
                    ValueScalarArithmeticOperator::Multiply
                    | ValueScalarArithmeticOperator::Divide
                    | ValueScalarArithmeticOperator::Modulo,
                ..
            } => 6,
            Self::BooleanNot { .. } => 7,
            Self::Identifier(_)
            | Self::ScalarLiteral(_)
            | Self::Call { .. }
            | Self::EnumVariant { .. }
            | Self::Record(_)
            | Self::List(_)
            | Self::Map(_)
            | Self::IfElse { .. }
            | Self::Grouped { .. } => 8,
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

impl fmt::Display for ListValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("List")?;
        if let (Some(element_type), Some(capacity)) = (&self.element_type, self.capacity) {
            write!(f, "<{element_type},{capacity}>")?;
        }
        f.write_str("[")?;
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{item}")?;
        }
        f.write_str("]")
    }
}

impl fmt::Display for MapValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Map")?;
        if let (Some(key_type), Some(value_type), Some(capacity)) =
            (&self.key_type, &self.value_type, self.capacity)
        {
            write!(f, "<{key_type},{value_type},{capacity}>")?;
        }
        f.write_str("[")?;
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}=>{}", entry.key, entry.value)?;
        }
        f.write_str("]")
    }
}
