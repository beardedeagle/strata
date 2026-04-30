use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub records: Vec<Record>,
    pub enums: Vec<Enum>,
    pub processes: Vec<Process>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub mailbox_bound: usize,
    pub state_type: String,
    pub msg_type: String,
    pub init: Function,
    pub step: Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
    pub effects: Vec<String>,
    pub may: Vec<String>,
    pub determinism: Determinism,
    pub body: Option<FunctionBody>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBody {
    pub emits: Vec<String>,
    pub returns: ReturnExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Determinism {
    Det,
    Nondet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnExpr {
    TypeValue(String),
    Variable(String),
    Call { name: String, arg: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub module: Module,
    pub entry_process: String,
    pub message_variant: String,
    pub init_state: String,
    pub step_result: StepResult,
    pub emitted_outputs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Stop,
}

pub fn parse_source(source: &str) -> Result<Module> {
    Parser::new(source)?.parse_module()
}

pub fn check_source(source: &str) -> Result<CheckedProgram> {
    let module = parse_source(source)?;
    check_module(module)
}

pub fn check_module(module: Module) -> Result<CheckedProgram> {
    if module.records.is_empty() {
        return Err(Error::new("expected at least one record declaration"));
    }
    if module.enums.is_empty() {
        return Err(Error::new("expected at least one enum declaration"));
    }
    if module.processes.is_empty() {
        return Err(Error::new("expected at least one process declaration"));
    }

    let process = module
        .processes
        .iter()
        .find(|candidate| candidate.name == "Main")
        .or_else(|| module.processes.first())
        .ok_or_else(|| Error::new("expected an entry process"))?;

    require_record(&module, &process.state_type)?;
    let msg_enum = require_enum(&module, &process.msg_type)?;
    let message_variant = msg_enum.variants.first().cloned().ok_or_else(|| {
        Error::new(format!(
            "enum {} must declare at least one variant",
            msg_enum.name
        ))
    })?;

    if process.mailbox_bound == 0 {
        return Err(Error::new(format!(
            "process {} mailbox bound must be greater than zero",
            process.name
        )));
    }

    check_init(process)?;
    let (init_state, step_result, emitted_outputs) = check_step(process)?;

    let entry_process = process.name.clone();

    Ok(CheckedProgram {
        module,
        entry_process,
        message_variant,
        init_state,
        step_result,
        emitted_outputs,
    })
}

fn require_record<'a>(module: &'a Module, name: &str) -> Result<&'a Record> {
    module
        .records
        .iter()
        .find(|record| record.name == name)
        .ok_or_else(|| Error::new(format!("type {name} is not declared as a record")))
}

fn require_enum<'a>(module: &'a Module, name: &str) -> Result<&'a Enum> {
    module
        .enums
        .iter()
        .find(|item| item.name == name)
        .ok_or_else(|| Error::new(format!("type {name} is not declared as an enum")))
}

fn check_init(process: &Process) -> Result<()> {
    let init = &process.init;
    if !init.params.is_empty() {
        return Err(Error::new(
            "initial hello slice requires init() with no parameters",
        ));
    }
    if init.return_type != process.state_type {
        return Err(Error::new(format!(
            "init returns {}, expected {}",
            init.return_type, process.state_type
        )));
    }
    if !init.effects.is_empty() || !init.may.is_empty() {
        return Err(Error::new(
            "initial hello slice requires init effects and may-behaviors to be empty",
        ));
    }
    if init.determinism != Determinism::Det {
        return Err(Error::new(
            "initial hello slice requires deterministic init",
        ));
    }
    let Some(body) = &init.body else {
        return Err(Error::new("init must have a body for buildable source"));
    };
    if !body.emits.is_empty() {
        return Err(Error::new(
            "init may not emit output in the initial hello slice",
        ));
    }
    match &body.returns {
        ReturnExpr::TypeValue(name) if name == &process.state_type => Ok(()),
        _ => Err(Error::new(format!(
            "init body must return {}",
            process.state_type
        ))),
    }
}

fn check_step(process: &Process) -> Result<(String, StepResult, Vec<String>)> {
    let step = &process.step;
    if step.params.len() != 2 {
        return Err(Error::new("step must declare state and msg parameters"));
    }
    let state_param = &step.params[0];
    let msg_param = &step.params[1];
    if state_param.name != "state" || state_param.ty != process.state_type {
        return Err(Error::new(format!(
            "step first parameter must be state: {}",
            process.state_type
        )));
    }
    if msg_param.name != "msg" || msg_param.ty != process.msg_type {
        return Err(Error::new(format!(
            "step second parameter must be msg: {}",
            process.msg_type
        )));
    }

    let expected_return = format!("ProcResult<{}>", process.state_type);
    if step.return_type != expected_return {
        return Err(Error::new(format!(
            "step returns {}, expected {}",
            step.return_type, expected_return
        )));
    }
    if !step.may.is_empty() {
        return Err(Error::new(
            "initial hello slice requires step may-behaviors to be empty",
        ));
    }
    if step.determinism != Determinism::Det {
        return Err(Error::new(
            "initial hello slice requires deterministic step",
        ));
    }

    let Some(body) = &step.body else {
        return Err(Error::new("step must have a body for buildable source"));
    };
    if body.emits.is_empty() {
        if !step.effects.is_empty() {
            return Err(Error::new(
                "step without emit statements must declare no effects in the initial hello slice",
            ));
        }
    } else if step.effects.as_slice() != ["emit"] {
        return Err(Error::new(
            "step with emit statements must declare exactly ! [emit] in the initial hello slice",
        ));
    }
    for output in &body.emits {
        validate_emit_text(output)?;
    }

    let step_result = match &body.returns {
        ReturnExpr::Call { name, arg } if arg == "state" && name == "Stop" => StepResult::Stop,
        ReturnExpr::Call { name, arg } if arg == "state" && name == "Continue" => {
            StepResult::Continue
        }
        _ => {
            return Err(Error::new(
                "step body must return Stop(state) or Continue(state)",
            ))
        }
    };

    Ok((process.state_type.clone(), step_result, body.emits.clone()))
}

fn validate_emit_text(output: &str) -> Result<()> {
    if output.is_empty() {
        return Err(Error::new("emit output must not be empty"));
    }
    if output.chars().any(char::is_control) {
        return Err(Error::new(
            "emit output must not contain control characters in the initial hello slice",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Number(String),
    StringLiteral(String),
    Symbol(char),
    Arrow,
    AtIdent(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    offset: usize,
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        while let Some((offset, ch)) = self.peek_char() {
            if ch.is_whitespace() {
                self.bump_char();
                continue;
            }
            if ch == '/' && self.peek_next_char() == Some('/') {
                self.bump_char();
                self.bump_char();
                while let Some((_, next)) = self.peek_char() {
                    self.bump_char();
                    if next == '\n' {
                        break;
                    }
                }
                continue;
            }
            if ch == '-' && self.peek_next_char() == Some('>') {
                self.bump_char();
                self.bump_char();
                tokens.push(Token {
                    kind: TokenKind::Arrow,
                    offset,
                });
                continue;
            }
            if ch == '@' {
                self.bump_char();
                match self.peek_char() {
                    Some((_, next)) if is_ident_start(next) => {}
                    _ => {
                        return Err(Error::new(format!(
                            "expected identifier after '@' at byte {offset}"
                        )));
                    }
                }
                let ident = self.read_ident()?;
                tokens.push(Token {
                    kind: TokenKind::AtIdent(ident),
                    offset,
                });
                continue;
            }
            if ch == '"' {
                let literal = self.read_string_literal(offset)?;
                tokens.push(Token {
                    kind: TokenKind::StringLiteral(literal),
                    offset,
                });
                continue;
            }
            if is_ident_start(ch) {
                let ident = self.read_ident()?;
                tokens.push(Token {
                    kind: TokenKind::Ident(ident),
                    offset,
                });
                continue;
            }
            if ch.is_ascii_digit() {
                let number = self.read_number();
                tokens.push(Token {
                    kind: TokenKind::Number(number),
                    offset,
                });
                continue;
            }
            if "{}()[];:,=<>!~".contains(ch) {
                self.bump_char();
                tokens.push(Token {
                    kind: TokenKind::Symbol(ch),
                    offset,
                });
                continue;
            }
            return Err(Error::new(format!(
                "unsupported character {ch:?} at byte {offset}"
            )));
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            offset: self.source.len(),
        });
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<(usize, char)> {
        self.source[self.offset..]
            .char_indices()
            .next()
            .map(|(local, ch)| (self.offset + local, ch))
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.offset..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.source[self.offset..].chars().next()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn read_ident(&mut self) -> Result<String> {
        let mut ident = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if is_ident_continue(ch) {
                ident.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        if ident.is_empty() {
            Err(Error::new(format!(
                "expected identifier at byte {}",
                self.offset
            )))
        } else {
            Ok(ident)
        }
    }

    fn read_number(&mut self) -> String {
        let mut number = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if ch.is_ascii_digit() {
                number.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        number
    }

    fn read_string_literal(&mut self, start: usize) -> Result<String> {
        self.bump_char();
        let mut literal = String::new();
        while let Some((offset, ch)) = self.peek_char() {
            match ch {
                '"' => {
                    self.bump_char();
                    return Ok(literal);
                }
                '\n' | '\r' => {
                    return Err(Error::new(format!(
                        "unterminated string literal at byte {start}"
                    )));
                }
                '\\' => {
                    return Err(Error::new(format!(
                        "string escapes are not supported in the initial hello slice at byte {offset}"
                    )));
                }
                _ => {
                    literal.push(ch);
                    self.bump_char();
                }
            }
        }
        Err(Error::new(format!(
            "unterminated string literal at byte {start}"
        )))
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self> {
        Ok(Self {
            tokens: Lexer::new(source).tokenize()?,
            index: 0,
        })
    }

    fn parse_module(mut self) -> Result<Module> {
        self.expect_keyword("module")?;
        let name = self.expect_ident()?;
        self.expect_symbol(';')?;

        let mut records = Vec::new();
        let mut enums = Vec::new();
        let mut processes = Vec::new();

        while !self.at_eof() {
            if self.peek_keyword("record") {
                records.push(self.parse_record()?);
            } else if self.peek_keyword("enum") {
                enums.push(self.parse_enum()?);
            } else if self.peek_keyword("proc") {
                processes.push(self.parse_process()?);
            } else if self.peek_keyword("security") {
                self.skip_statement()?;
            } else {
                return Err(self.error_here("expected record, enum, or proc declaration"));
            }
        }

        Ok(Module {
            name,
            records,
            enums,
            processes,
        })
    }

    fn parse_record(&mut self) -> Result<Record> {
        self.expect_keyword("record")?;
        let name = self.expect_ident()?;
        if self.consume_symbol(';') {
            return Ok(Record { name });
        }
        if self.consume_symbol('{') {
            self.skip_balanced_body('{', '}')?;
            self.expect_symbol(';')?;
            return Ok(Record { name });
        }
        Err(self.error_here("expected ';' or record field body"))
    }

    fn parse_enum(&mut self) -> Result<Enum> {
        self.expect_keyword("enum")?;
        let name = self.expect_ident()?;
        self.expect_symbol('{')?;
        let mut variants = Vec::new();
        while !self.consume_symbol('}') {
            variants.push(self.expect_ident()?);
            let _ = self.consume_symbol(',');
        }
        self.expect_symbol(';')?;
        Ok(Enum { name, variants })
    }

    fn parse_process(&mut self) -> Result<Process> {
        self.expect_keyword("proc")?;
        let name = self.expect_ident()?;
        self.expect_keyword("mailbox")?;
        self.expect_keyword("bounded")?;
        self.expect_symbol('(')?;
        let mailbox_bound = self
            .expect_number()?
            .parse::<usize>()
            .map_err(|_| Error::new(format!("process {name} mailbox bound must fit in usize")))?;
        self.expect_symbol(')')?;
        self.expect_symbol('{')?;

        let mut state_type = None;
        let mut msg_type = None;
        let mut init = None;
        let mut step = None;

        while !self.consume_symbol('}') {
            if self.peek_keyword("type") {
                self.expect_keyword("type")?;
                let alias = self.expect_ident()?;
                self.expect_symbol('=')?;
                let ty = self.parse_type()?;
                self.expect_symbol(';')?;
                match alias.as_str() {
                    "State" => state_type = Some(ty),
                    "Msg" => msg_type = Some(ty),
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported process type alias {alias}; expected State or Msg"
                        )));
                    }
                }
            } else if self.peek_keyword("fn") {
                let function = self.parse_function()?;
                match function.name.as_str() {
                    "init" => init = Some(function),
                    "step" => step = Some(function),
                    other => {
                        return Err(Error::new(format!(
                            "unsupported process function {other}; expected init or step"
                        )));
                    }
                }
            } else {
                return Err(self.error_here("expected process type alias or function"));
            }
        }

        Ok(Process {
            name: name.clone(),
            mailbox_bound,
            state_type: state_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type State")))?,
            msg_type: msg_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type Msg")))?,
            init: init.ok_or_else(|| Error::new(format!("process {name} must declare init")))?,
            step: step.ok_or_else(|| Error::new(format!("process {name} must declare step")))?,
        })
    }

    fn parse_function(&mut self) -> Result<Function> {
        self.expect_keyword("fn")?;
        let name = self.expect_ident()?;
        self.expect_symbol('(')?;
        let mut params = Vec::new();
        while !self.consume_symbol(')') {
            let param_name = self.expect_ident()?;
            self.expect_symbol(':')?;
            let ty = self.parse_type()?;
            params.push(Param {
                name: param_name,
                ty,
            });
            let _ = self.consume_symbol(',');
        }
        self.expect_arrow()?;
        let return_type = self.parse_type()?;
        self.expect_symbol('!')?;
        let effects = self.parse_list()?;
        self.expect_symbol('~')?;
        let may = self.parse_list()?;
        let determinism = match self.expect_at_ident()?.as_str() {
            "det" => Determinism::Det,
            "nondet" => Determinism::Nondet,
            other => {
                return Err(Error::new(format!(
                    "unsupported determinism @{other}; expected @det or @nondet"
                )));
            }
        };

        let body = if self.consume_symbol(';') {
            None
        } else {
            self.expect_symbol('{')?;
            let mut emits = Vec::new();
            while self.peek_keyword("emit") {
                self.expect_keyword("emit")?;
                emits.push(self.expect_string_literal()?);
                self.expect_symbol(';')?;
            }
            self.expect_keyword("return")?;
            let returns = self.parse_return_expr()?;
            self.expect_symbol(';')?;
            self.expect_symbol('}')?;
            Some(FunctionBody { emits, returns })
        };

        Ok(Function {
            name,
            params,
            return_type,
            effects,
            may,
            determinism,
            body,
        })
    }

    fn parse_type(&mut self) -> Result<String> {
        let name = self.expect_ident()?;
        if !self.consume_symbol('<') {
            return Ok(name);
        }
        let mut args = Vec::new();
        while !self.consume_symbol('>') {
            args.push(self.parse_type()?);
            let _ = self.consume_symbol(',');
        }
        Ok(format!("{name}<{}>", args.join(",")))
    }

    fn parse_list(&mut self) -> Result<Vec<String>> {
        self.expect_symbol('[')?;
        let mut values = Vec::new();
        while !self.consume_symbol(']') {
            values.push(self.expect_ident()?);
            let _ = self.consume_symbol(',');
        }
        Ok(values)
    }

    fn parse_return_expr(&mut self) -> Result<ReturnExpr> {
        let name = self.expect_ident()?;
        if self.consume_symbol('(') {
            let arg = self.expect_ident()?;
            self.expect_symbol(')')?;
            return Ok(ReturnExpr::Call { name, arg });
        }
        if self.consume_symbol('{') {
            self.skip_balanced_body('{', '}')?;
            return Ok(ReturnExpr::TypeValue(name));
        }
        if name
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
        {
            Ok(ReturnExpr::TypeValue(name))
        } else {
            Ok(ReturnExpr::Variable(name))
        }
    }

    fn skip_statement(&mut self) -> Result<()> {
        while !self.at_eof() {
            if self.consume_symbol(';') {
                return Ok(());
            }
            self.index += 1;
        }
        Err(self.error_here("expected ';'"))
    }

    fn skip_balanced_body(&mut self, open: char, close: char) -> Result<()> {
        let mut depth = 1usize;
        while !self.at_eof() {
            if self.consume_symbol(open) {
                depth += 1;
                continue;
            }
            if self.consume_symbol(close) {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
                continue;
            }
            self.index += 1;
        }
        Err(self.error_here("unterminated balanced body"))
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        match self.next_kind() {
            TokenKind::Ident(value) if value == keyword => Ok(()),
            _ => Err(self.error_previous(format!("expected keyword {keyword}"))),
        }
    }

    fn peek_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(value) if value == keyword)
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::Ident(value) => Ok(value),
            _ => Err(self.error_previous("expected identifier")),
        }
    }

    fn expect_number(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::Number(value) => Ok(value),
            _ => Err(self.error_previous("expected number")),
        }
    }

    fn expect_string_literal(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::StringLiteral(value) => Ok(value),
            _ => Err(self.error_previous("expected string literal")),
        }
    }

    fn expect_at_ident(&mut self) -> Result<String> {
        match self.next_kind() {
            TokenKind::AtIdent(value) => Ok(value),
            _ => Err(self.error_previous("expected @identifier")),
        }
    }

    fn expect_arrow(&mut self) -> Result<()> {
        match self.next_kind() {
            TokenKind::Arrow => Ok(()),
            _ => Err(self.error_previous("expected ->")),
        }
    }

    fn expect_symbol(&mut self, symbol: char) -> Result<()> {
        if self.consume_symbol(symbol) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected symbol {symbol:?}")))
        }
    }

    fn consume_symbol(&mut self, symbol: char) -> bool {
        if matches!(self.peek_kind(), TokenKind::Symbol(value) if *value == symbol) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next_kind(&mut self) -> TokenKind {
        let kind = self.peek_kind().clone();
        if !matches!(kind, TokenKind::Eof) {
            self.index += 1;
        }
        kind
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error_here(&self, message: impl Into<String>) -> Error {
        Error::new(format!(
            "{} at byte {}",
            message.into(),
            self.tokens[self.index].offset
        ))
    }

    fn error_previous(&self, message: impl Into<String>) -> Error {
        let offset = self
            .tokens
            .get(self.index.saturating_sub(1))
            .map(|token| token.offset)
            .unwrap_or(0);
        Error::new(format!("{} at byte {offset}", message.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"
module hello;

record MainState;
enum MainMsg { Start };

proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;

    fn init() -> MainState ! [] ~ [] @det {
        return MainState;
    }

    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [emit] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

    #[test]
    fn parses_and_checks_hello() {
        let checked = check_source(HELLO).expect("hello should check");

        assert_eq!(checked.module.name, "hello");
        assert_eq!(checked.entry_process, "Main");
        assert_eq!(checked.message_variant, "Start");
        assert_eq!(checked.step_result, StepResult::Stop);
        assert_eq!(checked.emitted_outputs, ["hello from Strata"]);
    }

    #[test]
    fn rejects_declaration_only_entry_points() {
        let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det;
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det;
}
"#;

        let err = check_source(source).expect_err("declaration-only source should be rejected");
        assert!(err.to_string().contains("init must have a body"));
    }

    #[test]
    fn rejects_emit_without_declared_effect() {
        let source = r#"
module hello;
record MainState;
enum MainMsg { Start };
proc Main mailbox bounded(1) {
    type State = MainState;
    type Msg = MainMsg;
    fn init() -> MainState ! [] ~ [] @det { return MainState; }
    fn step(state: MainState, msg: MainMsg) -> ProcResult<MainState> ! [] ~ [] @det {
        emit "hello from Strata";
        return Stop(state);
    }
}
"#;

        let err = check_source(source).expect_err("undeclared emit should be rejected");
        assert!(err.to_string().contains("must declare exactly ! [emit]"));
    }

    #[test]
    fn rejects_invalid_annotation_identifier_start() {
        let source = HELLO.replacen("@det", "@1", 1);

        let err = parse_source(&source).expect_err("invalid annotation should fail lexing");

        assert!(err.to_string().contains("expected identifier after '@'"));
    }
}
