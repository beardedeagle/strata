use super::ast::{
    CollectionPatternBinding, ConstructorPayloadPattern, Determinism, Effect, Enum, EnumVariant,
    Function, FunctionBlock, FunctionBody, FunctionParam, Identifier, ListPattern, ListValue,
    MapPattern, MapPatternEntry, MapValue, MapValueEntry, Match, MatchArm, Module, OutputLiteral,
    Param, Pattern, Process, Record, RecordField, RecordPatternField, RecordValue,
    RecordValueField, ReturnExpr, Statement, TypeRef, ValueExpr,
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
        let mut functions = Vec::new();
        let mut processes = Vec::new();

        while !self.at_eof() {
            if self.peek_keyword("record") {
                records.push(self.parse_record()?);
            } else if self.peek_keyword("enum") {
                enums.push(self.parse_enum()?);
            } else if self.peek_keyword("fn") {
                functions.push(self.parse_function()?);
            } else if self.peek_keyword("proc") {
                processes.push(self.parse_process()?);
            } else if self.peek_keyword("security") {
                return Err(self.error_here(
                    "security declarations are not supported in this buildable source slice",
                ));
            } else {
                return Err(self.error_here("expected record, enum, function, or proc declaration"));
            }
        }

        Ok(Module {
            name,
            records,
            enums,
            functions,
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
            let name = self.expect_identifier()?;
            let payload_type = if self.consume_symbol('(') {
                let ty = self.parse_type()?;
                self.expect_symbol(')')?;
                Some(ty)
            } else {
                None
            };
            variants.push(EnumVariant { name, payload_type });
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
        let mut functions = Vec::new();
        let mut steps = Vec::new();

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
                        steps.push(function);
                    }
                    _ => {
                        functions.push(function);
                    }
                }
            } else {
                return Err(self.error_here("expected process type alias or function"));
            }
        }
        if steps.is_empty() {
            return Err(Error::new(format!("process {name} must declare step")));
        }

        Ok(Process {
            name: name.clone(),
            mailbox_bound,
            state_type: state_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type State")))?,
            msg_type: msg_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type Msg")))?,
            init: init.ok_or_else(|| Error::new(format!("process {name} must declare init")))?,
            functions,
            steps,
        })
    }

    fn parse_function(&mut self) -> Result<Function> {
        self.expect_keyword("fn")?;
        let name = self.expect_identifier()?;
        self.expect_symbol('(')?;
        let mut params = Vec::new();
        if !self.consume_symbol(')') {
            loop {
                let param_name = self.expect_ident()?;
                if self.consume_symbol(':') {
                    let ty = self.parse_type()?;
                    params.push(FunctionParam::Binding(Param {
                        name: Identifier::new(param_name)?,
                        ty,
                    }));
                } else {
                    params.push(FunctionParam::Pattern(self.parse_pattern(param_name)?));
                }
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
            let body = if self.peek_keyword("match") {
                let match_body = self.parse_match_body()?;
                if !self.peek_symbol('}') {
                    return Err(self.error_here(
                        "match body must be the whole function body in this source slice",
                    ));
                }
                FunctionBody::Match(match_body)
            } else {
                FunctionBody::Block(self.parse_function_block()?)
            };
            self.expect_symbol('}')?;
            Some(body)
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

    fn parse_pattern(&mut self, value: String) -> Result<Pattern> {
        if value == "_" {
            if self.peek_symbol('(') {
                return Err(self.error_here("wildcard patterns cannot bind payloads"));
            }
            if self.peek_symbol('{') {
                return Err(self.error_here("wildcard patterns cannot destructure fields"));
            }
            return Ok(Pattern::Wildcard);
        }

        let name = Identifier::new(value)?;
        if name.as_str() == LIST_TYPE {
            let type_args = self.parse_optional_collection_type_args(&name, 1)?;
            if self.consume_symbol('[') {
                let (element_type, capacity) = match type_args {
                    Some(mut args) => (Some(args.types.remove(0)), Some(args.capacity)),
                    None => (None, None),
                };
                return Ok(Pattern::List(ListPattern {
                    element_type,
                    capacity,
                    elements: self.parse_list_pattern_elements()?,
                }));
            }
            if type_args.is_some() {
                return Err(self.error_here("list patterns must use List<T,N>[...]"));
            }
        }
        if name.as_str() == MAP_TYPE {
            let type_args = self.parse_optional_collection_type_args(&name, 2)?;
            if self.consume_symbol('[') {
                let (key_type, value_type, capacity) = match type_args {
                    Some(mut args) => {
                        let value_type = args.types.remove(1);
                        let key_type = args.types.remove(0);
                        (Some(key_type), Some(value_type), Some(args.capacity))
                    }
                    None => (None, None, None),
                };
                return Ok(Pattern::Map(MapPattern {
                    key_type,
                    value_type,
                    capacity,
                    entries: self.parse_map_pattern_entries()?,
                }));
            }
            if type_args.is_some() {
                return Err(self.error_here("map patterns must use Map<K,V,N>[...]"));
            }
        }
        if self.consume_symbol('{') {
            let fields = self.parse_record_pattern_fields(&name)?;
            return Ok(Pattern::Record { name, fields });
        }
        let payload = if self.consume_symbol('(') {
            let value = self.expect_ident()?;
            let payload = if self.consume_symbol(':') {
                let ty = self.parse_type()?;
                ConstructorPayloadPattern::Binding(Param {
                    name: Identifier::new(value)?,
                    ty,
                })
            } else if self.starts_payload_destructuring_pattern(&value) {
                ConstructorPayloadPattern::Destructure(Box::new(self.parse_pattern(value)?))
            } else {
                return Err(self.error_previous(format!(
                    "constructor payload pattern {name}({value}) must bind the payload as {value}: Type or destructure a record, list, map, or wildcard"
                )));
            };
            self.expect_symbol(')')?;
            Some(payload)
        } else {
            None
        };
        Ok(Pattern::Constructor { name, payload })
    }

    fn starts_payload_destructuring_pattern(&self, value: &str) -> bool {
        value == "_"
            || self.peek_symbol('{')
            || ((value == LIST_TYPE || value == MAP_TYPE)
                && (self.peek_symbol('[') || self.peek_symbol('<')))
    }

    fn parse_optional_collection_type_args(
        &mut self,
        constructor: &Identifier,
        expected_type_count: usize,
    ) -> Result<Option<CollectionTypeArgs>> {
        if !self.consume_symbol('<') {
            return Ok(None);
        }
        let mut type_args = Vec::new();
        let mut capacity = None;
        if self.consume_symbol('>') {
            return Err(self.error_previous(format!(
                "type {constructor} must declare {expected_type_count} type arguments and one numeric capacity"
            )));
        }
        loop {
            if self.peek_number() {
                if capacity.is_some() {
                    return Err(self.error_here(format!(
                        "type {constructor} must declare exactly one numeric capacity"
                    )));
                }
                capacity = Some(self.parse_capacity_arg(constructor)?);
            } else if capacity.is_some() {
                return Err(self.error_here(format!(
                    "type {constructor} must declare type arguments before its numeric capacity"
                )));
            } else {
                type_args.push(self.parse_type_with_depth(1)?);
            }
            if self.consume_symbol(',') {
                if self.consume_symbol('>') {
                    break;
                }
                continue;
            }
            self.expect_symbol('>')?;
            break;
        }
        if type_args.len() != expected_type_count || capacity.is_none() {
            return Err(self.error_previous(format!(
                "type {constructor} must declare {expected_type_count} type arguments and one numeric capacity"
            )));
        }
        Ok(Some(CollectionTypeArgs {
            types: type_args,
            capacity: capacity.expect("capacity checked above"),
        }))
    }

    fn parse_list_pattern_elements(&mut self) -> Result<Vec<CollectionPatternBinding>> {
        let mut elements = Vec::new();
        if self.consume_symbol(']') {
            return Ok(elements);
        }
        loop {
            elements.push(self.parse_collection_pattern_binding()?);
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(elements)
    }

    fn parse_map_pattern_entries(&mut self) -> Result<Vec<MapPatternEntry>> {
        let mut entries = Vec::new();
        if self.consume_symbol(']') {
            return Ok(entries);
        }
        loop {
            let key = self.parse_value_expr()?;
            self.expect_fat_arrow()?;
            let binding = self.parse_collection_pattern_binding()?;
            entries.push(MapPatternEntry { key, binding });
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(entries)
    }

    fn parse_collection_pattern_binding(&mut self) -> Result<CollectionPatternBinding> {
        if self.peek_keyword("mut") || self.peek_keyword("var") {
            return Err(self.error_here(
                "collection pattern bindings are immutable; mutable bindings are not supported",
            ));
        }
        let binding = self.expect_ident()?;
        if binding == "_" {
            Ok(CollectionPatternBinding::Wildcard)
        } else {
            Ok(CollectionPatternBinding::Binding(Identifier::new(binding)?))
        }
    }

    fn parse_record_pattern_fields(
        &mut self,
        record_name: &Identifier,
    ) -> Result<Vec<RecordPatternField>> {
        let mut fields = Vec::new();
        if self.peek_symbol('}') {
            return Err(self.error_here(format!(
                "record pattern {record_name} must bind at least one field"
            )));
        }
        loop {
            if self.peek_keyword("mut") || self.peek_keyword("var") {
                return Err(self.error_here(
                    "record pattern bindings are immutable; mutable bindings are not supported",
                ));
            }
            let field = self.expect_identifier()?;
            if self.consume_symbol('=') {
                return Err(self.error_previous(
                    "record pattern fields use ':'; assignment syntax is not supported",
                ));
            }
            let binding = if self.consume_symbol(':') {
                self.expect_identifier()?
            } else {
                field.clone()
            };
            fields.push(RecordPatternField { field, binding });
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

    fn parse_match_body(&mut self) -> Result<Match> {
        self.expect_keyword("match")?;
        let scrutinee = self.expect_identifier()?;
        self.expect_symbol('{')?;
        let mut arms = Vec::new();
        if self.peek_symbol('}') {
            return Err(self.error_here("match body must declare at least one arm"));
        }
        while !self.peek_symbol('}') {
            arms.push(self.parse_match_arm()?);
            if self.consume_symbol(',') {
                return Err(self.error_previous(
                    "match arms are block-delimited and must not use comma separators",
                ));
            }
        }
        self.expect_symbol('}')?;
        Ok(Match { scrutinee, arms })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm> {
        let pattern_name = self.expect_ident()?;
        let pattern = self.parse_pattern(pattern_name)?;
        self.expect_fat_arrow()?;
        self.expect_symbol('{')?;
        let body = self.parse_function_block()?;
        self.expect_symbol('}')?;
        Ok(MatchArm { pattern, body })
    }

    fn parse_function_block(&mut self) -> Result<FunctionBlock> {
        let mut statements = Vec::new();
        while !self.peek_keyword("return") {
            statements.push(self.parse_function_statement()?);
        }
        self.expect_keyword("return")?;
        let returns = self.parse_return_expr()?;
        self.expect_symbol(';')?;
        Ok(FunctionBlock {
            statements,
            returns,
        })
    }

    fn parse_function_statement(&mut self) -> Result<Statement> {
        if self.peek_keyword("match") {
            return Err(
                self.error_here("match body must be the whole function body in this source slice")
            );
        }
        if self.peek_keyword("emit") {
            self.expect_keyword("emit")?;
            let text = self.expect_string_literal()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Emit(OutputLiteral::new(text)?));
        }
        if self.peek_keyword("let") {
            self.expect_keyword("let")?;
            let name = self.expect_identifier()?;
            self.expect_symbol(':')?;
            let ty = self.parse_type()?;
            self.expect_symbol('=')?;
            self.expect_keyword("spawn")?;
            let target = self.expect_identifier()?;
            self.expect_symbol(';')?;
            return Ok(Statement::LetProcessRef { name, ty, target });
        }
        if self.peek_keyword("send") {
            self.expect_keyword("send")?;
            let target = self.expect_identifier()?;
            let message = self.expect_identifier()?;
            let payload = if self.consume_symbol('(') {
                let value = self.parse_value_expr()?;
                self.expect_symbol(')')?;
                Some(value)
            } else {
                None
            };
            self.expect_symbol(';')?;
            return Ok(Statement::Send {
                target,
                message,
                payload,
            });
        }
        Err(self.error_here("expected emit, let, send, or return statement"))
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
        let mut const_args = Vec::new();
        let mut seen_const_arg = false;
        if self.consume_symbol('>') {
            return Err(self.error_previous(format!(
                "type {name} must declare at least one type argument"
            )));
        }
        loop {
            if self.peek_number() {
                seen_const_arg = true;
                const_args.push(self.parse_capacity_arg(&name)?);
            } else {
                if seen_const_arg {
                    return Err(self.error_here(format!(
                        "type {name} must declare type arguments before numeric arguments"
                    )));
                }
                args.push(self.parse_type_with_depth(depth + 1)?);
            }
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
            const_args,
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
        if self.peek_keyword("match") {
            return Ok(ReturnExpr::Match(self.parse_match_body()?));
        }
        let value = self.parse_value_expr()?;
        if let ValueExpr::Call { name, arg } = value {
            return Ok(ReturnExpr::Call { name, arg: *arg });
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
        if name.as_str() == LIST_TYPE {
            let type_args = self.parse_optional_collection_type_args(&name, 1)?;
            if self.consume_symbol('[') {
                let (element_type, capacity) = match type_args {
                    Some(mut args) => (Some(args.types.remove(0)), Some(args.capacity)),
                    None => (None, None),
                };
                return Ok(ValueExpr::List(ListValue {
                    element_type,
                    capacity,
                    items: self.parse_list_value_items(depth)?,
                }));
            }
            if type_args.is_some() {
                return Err(self.error_here("list values must use List<T,N>[...]"));
            }
        }
        if name.as_str() == MAP_TYPE {
            let type_args = self.parse_optional_collection_type_args(&name, 2)?;
            if self.consume_symbol('[') {
                let (key_type, value_type, capacity) = match type_args {
                    Some(mut args) => {
                        let value_type = args.types.remove(1);
                        let key_type = args.types.remove(0);
                        (Some(key_type), Some(value_type), Some(args.capacity))
                    }
                    None => (None, None, None),
                };
                return Ok(ValueExpr::Map(MapValue {
                    key_type,
                    value_type,
                    capacity,
                    entries: self.parse_map_value_entries(depth)?,
                }));
            }
            if type_args.is_some() {
                return Err(self.error_here("map values must use Map<K,V,N>[...]"));
            }
        }
        if self.consume_symbol('(') {
            let arg = self.parse_value_expr_with_depth(depth + 1)?;
            self.expect_symbol(')')?;
            return Ok(ValueExpr::Call {
                name,
                arg: Box::new(arg),
            });
        }
        if !self.consume_symbol('{') {
            return Ok(ValueExpr::Identifier(name));
        }
        let fields = self.parse_record_value_fields(&name, depth)?;
        Ok(ValueExpr::Record(RecordValue { name, fields }))
    }

    fn parse_list_value_items(&mut self, depth: usize) -> Result<Vec<ValueExpr>> {
        let mut items = Vec::new();
        if self.consume_symbol(']') {
            return Ok(items);
        }
        loop {
            items.push(self.parse_value_expr_with_depth(depth + 1)?);
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(items)
    }

    fn parse_map_value_entries(&mut self, depth: usize) -> Result<Vec<MapValueEntry>> {
        let mut entries = Vec::new();
        if self.consume_symbol(']') {
            return Ok(entries);
        }
        loop {
            let key = self.parse_value_expr_with_depth(depth + 1)?;
            self.expect_fat_arrow()?;
            let value = self.parse_value_expr_with_depth(depth + 1)?;
            entries.push(MapValueEntry { key, value });
            if self.consume_symbol(',') {
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(entries)
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

    fn peek_number(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Number(_))
    }

    fn parse_capacity_arg(&mut self, constructor: &Identifier) -> Result<usize> {
        let capacity = self.expect_number()?;
        capacity.parse::<usize>().map_err(|_| {
            self.error_previous(format!(
                "type {constructor} numeric capacity must fit in usize"
            ))
        })
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

    fn expect_fat_arrow(&mut self) -> Result<()> {
        if matches!(self.peek_kind(), TokenKind::FatArrow) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here("expected =>"))
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
