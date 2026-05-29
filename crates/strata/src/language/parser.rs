use super::ast::{
    AuthorityDeclaration, CollectionPatternBinding, Component, ConstructorPayloadPattern,
    Determinism, Effect, Enum, EnumVariant, ForEachItem, Function, FunctionBlock, FunctionBody,
    FunctionParam, Identifier, Import, ListPattern, ListValue, MapPattern, MapPatternCompleteness,
    MapPatternEntry, MapValue, MapValueEntry, Match, MatchArm, Module, OutputLiteral, Param,
    Pattern, Port, Process, Protocol, Record, RecordField, RecordPatternField, RecordValue,
    RecordValueField, ReturnExpr, Statement, SupervisorChildDeclaration, SupervisorChildMode,
    SupervisorDeclaration, SupervisorStrategy, TypeRef, ValueBooleanOperator,
    ValueEqualityOperator, ValueExpr, ValueScalarArithmeticOperator, ValueScalarOrderingOperator,
};
use super::diagnostic::{Error, Result};
use super::lexer::{Lexer, Token, TokenKind};
use super::{LIST_TYPE, MAP_TYPE, MAX_SOURCE_BYTES, MAX_TYPE_NESTING, MAX_VALUE_NESTING};

pub fn parse_source(source: &str) -> Result<Module> {
    Parser::new(source)?.parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

struct CollectionTypeArgs {
    types: Vec<TypeRef>,
    capacity: usize,
}

struct ListPatternItems {
    elements: Vec<CollectionPatternBinding>,
    rest: Option<Identifier>,
}

mod declarations;
mod patterns;
mod statements;
mod tokens;
mod types;
mod values;

impl Parser {
    fn new(source: &str) -> Result<Self> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(Error::new(format!(
                "source exceeds maximum size of {MAX_SOURCE_BYTES} bytes"
            )));
        }
        Ok(Self {
            tokens: Lexer::new(source).tokenize()?,
            index: 0,
        })
    }
}
