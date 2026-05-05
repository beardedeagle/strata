mod ast;
mod checked;
mod checker;
mod diagnostic;
mod lexer;
mod lowering;
mod parser;

#[cfg(test)]
mod tests;

pub use ast::{
    Determinism, Effect, Enum, Function, FunctionBlock, FunctionBody, Identifier, MessageMatch,
    MessageMatchArm, Module, OutputLiteral, Param, Process, Record, RecordField, RecordValue,
    RecordValueField, ReturnExpr, Statement, TypeRef, ValueExpr,
};
pub use checked::CheckedProgram;
pub use checker::check_module;
pub use diagnostic::{Error, Result};
pub use lowering::lower_to_artifact;
pub use parser::parse_source;

const STATIC_RUNTIME_DISPATCH_LIMIT: usize = 10_000;
const STATIC_RUNTIME_PROCESS_LIMIT: usize = 10_000;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_TOKEN_COUNT: usize = 128_000;
const MAX_TYPE_NESTING: usize = 32;
const MAX_VALUE_NESTING: usize = 32;
const PROC_RESULT_TYPE: &str = "ProcResult";
const PROCESS_REF_TYPE: &str = "ProcessRef";

pub fn check_source(source: &str) -> Result<CheckedProgram> {
    let module = parse_source(source)?;
    check_module(module)
}
