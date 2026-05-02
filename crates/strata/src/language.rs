mod ast;
mod checker;
mod lexer;
mod parser;

#[cfg(test)]
mod tests;

use mantle_artifact::Result;

pub use ast::{
    Determinism, Effect, Enum, Function, FunctionBody, Identifier, Module, OutputLiteral, Param,
    Process, Record, RecordField, RecordValue, RecordValueField, ReturnExpr, Statement, TypeRef,
    ValueExpr,
};
pub use checker::{check_module, CheckedProgram};
pub use parser::parse_source;

const STATIC_RUNTIME_DISPATCH_LIMIT: usize = 10_000;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_TOKEN_COUNT: usize = 128_000;
const MAX_TYPE_NESTING: usize = 32;
const MAX_VALUE_NESTING: usize = 32;
const PROC_RESULT_TYPE: &str = "ProcResult";

pub fn check_source(source: &str) -> Result<CheckedProgram> {
    let module = parse_source(source)?;
    let checked = check_module(module)?;
    checked.to_artifact(source)?;
    Ok(checked)
}
