use super::ast::{
    Determinism, Effect, Enum, Function, FunctionBody, Identifier, Module, OutputLiteral, Param,
    Process, Record, RecordField, RecordValue, RecordValueField, ReturnExpr, Statement, TypeRef,
    ValueExpr,
};
use super::diagnostic::{Error, Result};
use super::lexer::{Lexer, Token, TokenKind};
use super::{MAX_SOURCE_BYTES, MAX_TYPE_NESTING, MAX_VALUE_NESTING};

pub fn parse_source(source: &str) -> Result<Module> {
    Parser::new(source)?.parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

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

    fn parse_module(mut self) -> Result<Module> {
        self.expect_keyword("module")?;
        let name = self.expect_identifier()?;
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
                return Err(self.error_here(
                    "security declarations are not supported in this buildable source slice",
                ));
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
        let name = self.expect_identifier()?;
        if self.consume_symbol(';') {
            return Ok(Record {
                name,
                fields: Vec::new(),
            });
        }
        if self.consume_symbol('{') {
            let fields = self.parse_record_fields(&name)?;
            self.reject_braced_type_semicolon("record")?;
            return Ok(Record { name, fields });
        }
        Err(self.error_here("expected ';' or record field body"))
    }

    fn parse_record_fields(&mut self, record_name: &Identifier) -> Result<Vec<RecordField>> {
        let mut fields = Vec::new();
        if self.peek_symbol('}') {
            return Err(self.error_here(format!(
                "fieldless records use `record {record_name};`; braced records must declare at least one field"
            )));
        }
        loop {
            if self.peek_keyword("mut") || self.peek_keyword("var") {
                return Err(self.error_here(
                    "record fields are immutable; mutable field declarations are not supported",
                ));
            }
            let name = self.expect_identifier()?;
            self.expect_symbol(':')?;
            let ty = self.parse_type()?;
            fields.push(RecordField { name, ty });
            if self.consume_symbol(',') {
                if self.consume_symbol('}') {
                    break;
                }
                continue;
            }
            self.expect_symbol('}')?;
            break;
        }
        Ok(fields)
    }

    fn parse_enum(&mut self) -> Result<Enum> {
        self.expect_keyword("enum")?;
        let name = self.expect_identifier()?;
        self.expect_symbol('{')?;
        let mut variants = Vec::new();
        if self.consume_symbol('}') {
            self.reject_braced_type_semicolon("enum")?;
            return Ok(Enum { name, variants });
        }
        loop {
            variants.push(self.expect_identifier()?);
            if self.consume_symbol(',') {
                if self.consume_symbol('}') {
                    break;
                }
                continue;
            }
            self.expect_symbol('}')?;
            break;
        }
        self.reject_braced_type_semicolon("enum")?;
        Ok(Enum { name, variants })
    }

    fn parse_process(&mut self) -> Result<Process> {
        self.expect_keyword("proc")?;
        let name = self.expect_identifier()?;
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
                    "State" => {
                        if state_type.is_some() {
                            return Err(Error::new(format!(
                                "process {name} declares duplicate type State"
                            )));
                        }
                        state_type = Some(ty);
                    }
                    "Msg" => {
                        if msg_type.is_some() {
                            return Err(Error::new(format!(
                                "process {name} declares duplicate type Msg"
                            )));
                        }
                        msg_type = Some(ty);
                    }
                    _ => {
                        return Err(Error::new(format!(
                            "unsupported process type alias {alias}; expected State or Msg"
                        )));
                    }
                }
            } else if self.peek_keyword("fn") {
                let function = self.parse_function()?;
                match function.name.as_str() {
                    "init" => {
                        if init.is_some() {
                            return Err(Error::new(format!(
                                "process {name} declares duplicate init function"
                            )));
                        }
                        init = Some(function);
                    }
                    "step" => {
                        if step.is_some() {
                            return Err(Error::new(format!(
                                "process {name} declares duplicate step function"
                            )));
                        }
                        step = Some(function);
                    }
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
        let name = self.expect_identifier()?;
        self.expect_symbol('(')?;
        let mut params = Vec::new();
        if !self.consume_symbol(')') {
            loop {
                let param_name = self.expect_identifier()?;
                self.expect_symbol(':')?;
                let ty = self.parse_type()?;
                params.push(Param {
                    name: param_name,
                    ty,
                });
                if self.consume_symbol(',') {
                    if self.consume_symbol(')') {
                        break;
                    }
                    continue;
                }
                self.expect_symbol(')')?;
                break;
            }
        }
        self.expect_arrow()?;
        let return_type = self.parse_type()?;
        self.expect_symbol('!')?;
        let effects = self.parse_effect_list()?;
        self.expect_symbol('~')?;
        let may = self.parse_identifier_list()?;
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
            let mut statements = Vec::new();
            while !self.peek_keyword("return") {
                statements.push(self.parse_function_statement()?);
            }
            self.expect_keyword("return")?;
            let returns = self.parse_return_expr()?;
            self.expect_symbol(';')?;
            self.expect_symbol('}')?;
            Some(FunctionBody {
                statements,
                returns,
            })
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

    fn parse_function_statement(&mut self) -> Result<Statement> {
        if self.peek_keyword("emit") {
            self.expect_keyword("emit")?;
            let text = self.expect_string_literal()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Emit(OutputLiteral::new(text)?));
        }
        if self.peek_keyword("spawn") {
            self.expect_keyword("spawn")?;
            let target = self.expect_identifier()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Spawn(target));
        }
        if self.peek_keyword("send") {
            self.expect_keyword("send")?;
            let target = self.expect_identifier()?;
            let message = self.expect_identifier()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Send { target, message });
        }
        Err(self.error_here("expected emit, spawn, send, or return statement"))
    }

    fn parse_type(&mut self) -> Result<TypeRef> {
        self.parse_type_with_depth(0)
    }

    fn parse_type_with_depth(&mut self, depth: usize) -> Result<TypeRef> {
        if depth > MAX_TYPE_NESTING {
            return Err(Error::new(format!(
                "type nesting exceeds maximum depth of {MAX_TYPE_NESTING}"
            )));
        }
        let name = self.expect_identifier()?;
        if !self.consume_symbol('<') {
            return Ok(TypeRef::named(name));
        }
        let mut args = Vec::new();
        if self.consume_symbol('>') {
            return Err(self.error_previous(format!(
                "type {name} must declare at least one type argument"
            )));
        }
        loop {
            args.push(self.parse_type_with_depth(depth + 1)?);
            if self.consume_symbol(',') {
                if self.consume_symbol('>') {
                    break;
                }
                continue;
            }
            self.expect_symbol('>')?;
            break;
        }
        Ok(TypeRef::Applied {
            constructor: name,
            args,
        })
    }

    fn parse_effect_list(&mut self) -> Result<Vec<Effect>> {
        self.expect_symbol('[')?;
        let mut values = Vec::new();
        if self.consume_symbol(']') {
            return Ok(values);
        }
        loop {
            let ident = self.expect_ident()?;
            let effect = Effect::parse(&ident)
                .ok_or_else(|| Error::new(format!("unsupported effect {ident}")))?;
            values.push(effect);
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(values)
    }

    fn parse_identifier_list(&mut self) -> Result<Vec<Identifier>> {
        self.expect_symbol('[')?;
        let mut values = Vec::new();
        if self.consume_symbol(']') {
            return Ok(values);
        }
        loop {
            values.push(self.expect_identifier()?);
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(values)
    }

    fn parse_return_expr(&mut self) -> Result<ReturnExpr> {
        let value = self.parse_value_expr()?;
        if let ValueExpr::Identifier(name) = value {
            if !self.consume_symbol('(') {
                return Ok(ReturnExpr::Value(ValueExpr::Identifier(name)));
            }
            let arg = self.parse_value_expr()?;
            self.expect_symbol(')')?;
            return Ok(ReturnExpr::Call { name, arg });
        }
        Ok(ReturnExpr::Value(value))
    }

    fn parse_value_expr(&mut self) -> Result<ValueExpr> {
        self.parse_value_expr_with_depth(0)
    }

    fn parse_value_expr_with_depth(&mut self, depth: usize) -> Result<ValueExpr> {
        if depth > MAX_VALUE_NESTING {
            return Err(self.error_here(format!(
                "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
            )));
        }
        let name = self.expect_identifier()?;
        if !self.consume_symbol('{') {
            return Ok(ValueExpr::Identifier(name));
        }
        let fields = self.parse_record_value_fields(&name, depth)?;
        Ok(ValueExpr::Record(RecordValue { name, fields }))
    }

    fn parse_record_value_fields(
        &mut self,
        record_name: &Identifier,
        depth: usize,
    ) -> Result<Vec<RecordValueField>> {
        let mut fields = Vec::new();
        if self.peek_symbol('}') {
            return Err(self.error_here(format!(
                "fieldless record values use `{record_name}`; braced record values must declare at least one field"
            )));
        }
        loop {
            if self.peek_keyword("mut") || self.peek_keyword("var") {
                return Err(self.error_here(
                    "record values are immutable; mutable field bindings are not supported",
                ));
            }
            let name = self.expect_identifier()?;
            if self.consume_symbol('=') {
                return Err(self.error_previous(
                    "record value fields use ':'; assignment syntax is not supported",
                ));
            }
            self.expect_symbol(':')?;
            let value = self.parse_value_expr_with_depth(depth + 1)?;
            fields.push(RecordValueField { name, value });
            if self.consume_symbol(',') {
                if self.consume_symbol('}') {
                    break;
                }
                continue;
            }
            self.expect_symbol('}')?;
            break;
        }
        Ok(fields)
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        if self.peek_keyword(keyword) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here(format!("expected keyword {keyword}")))
        }
    }

    fn peek_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(value) if value == keyword)
    }

    fn expect_ident(&mut self) -> Result<String> {
        if let TokenKind::Ident(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected identifier"))
        }
    }

    fn expect_identifier(&mut self) -> Result<Identifier> {
        Identifier::new(self.expect_ident()?)
    }

    fn expect_number(&mut self) -> Result<String> {
        if let TokenKind::Number(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected number"))
        }
    }

    fn expect_string_literal(&mut self) -> Result<String> {
        if let TokenKind::StringLiteral(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected string literal"))
        }
    }

    fn expect_at_ident(&mut self) -> Result<String> {
        if let TokenKind::AtIdent(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected @identifier"))
        }
    }

    fn expect_arrow(&mut self) -> Result<()> {
        if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here("expected ->"))
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

    fn reject_braced_type_semicolon(&mut self, declaration_kind: &str) -> Result<()> {
        if self.peek_symbol(';') {
            return Err(self.error_here(format!(
                "braced {declaration_kind} declarations are terminated by '}}', not ';'"
            )));
        }
        Ok(())
    }

    fn peek_symbol(&self, symbol: char) -> bool {
        matches!(self.peek_kind(), TokenKind::Symbol(value) if *value == symbol)
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.index += 1;
        }
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
