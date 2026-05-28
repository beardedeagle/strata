use super::*;

impl Parser {
    pub(super) fn parse_module(mut self) -> Result<Module> {
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
                continue;
            }
            if self.peek_keyword("enum") {
                enums.push(self.parse_enum()?);
                continue;
            }
            if self.peek_keyword("fn") {
                functions.push(self.parse_function()?);
                continue;
            }
            if self.peek_keyword("proc") {
                processes.push(self.parse_process()?);
                continue;
            }
            if self.peek_keyword("security") {
                return Err(self.error_here(
                    "security declarations are not supported in the buildable language surface",
                ));
            }

            return Err(self.error_here("expected record, enum, function, or proc declaration"));
        }

        Ok(Module {
            name,
            records,
            enums,
            functions,
            processes,
        })
    }

    pub(super) fn parse_record(&mut self) -> Result<Record> {
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

    pub(super) fn parse_record_fields(
        &mut self,
        record_name: &Identifier,
    ) -> Result<Vec<RecordField>> {
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

    pub(super) fn parse_enum(&mut self) -> Result<Enum> {
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

    pub(super) fn parse_process(&mut self) -> Result<Process> {
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
        let mut authorities = Vec::new();
        let mut supervisors = Vec::new();
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
            } else if self.peek_keyword("authority") {
                self.expect_keyword("authority")?;
                let authority_name = self.expect_identifier()?;
                self.expect_symbol(':')?;
                let ty = self.parse_type()?;
                self.expect_symbol(';')?;
                authorities.push(AuthorityDeclaration {
                    name: authority_name,
                    ty,
                });
            } else if self.peek_keyword("supervise") {
                supervisors.push(self.parse_supervisor()?);
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
                return Err(self.error_here(
                    "expected process authority, supervisor, type alias, or function",
                ));
            }
        }
        if steps.is_empty() {
            return Err(Error::new(format!("process {name} must declare step")));
        }

        Ok(Process {
            name: name.clone(),
            mailbox_bound,
            authorities,
            supervisors,
            state_type: state_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type State")))?,
            msg_type: msg_type
                .ok_or_else(|| Error::new(format!("process {name} must declare type Msg")))?,
            init: init.ok_or_else(|| Error::new(format!("process {name} must declare init")))?,
            functions,
            steps,
        })
    }

    fn parse_supervisor(&mut self) -> Result<SupervisorDeclaration> {
        self.expect_keyword("supervise")?;
        self.expect_keyword("local")?;
        self.expect_keyword("one_for_one")?;
        self.expect_symbol('(')?;
        self.expect_keyword("max_restarts")?;
        self.expect_symbol(':')?;
        let max_restarts = self.parse_supervisor_u32_literal("max_restarts")?;
        self.expect_symbol(',')?;
        self.expect_keyword("within_ms")?;
        self.expect_symbol(':')?;
        let within_ms = self.parse_supervisor_u64_literal("within_ms")?;
        self.expect_symbol(')')?;
        self.expect_symbol('{')?;

        let mut children = Vec::new();
        if self.peek_symbol('}') {
            return Err(self.error_here("local supervisor must declare at least one child"));
        }
        while !self.consume_symbol('}') {
            children.push(self.parse_supervisor_child()?);
        }

        Ok(SupervisorDeclaration {
            strategy: SupervisorStrategy::OneForOne,
            max_restarts,
            within_ms,
            children,
        })
    }

    fn parse_supervisor_child(&mut self) -> Result<SupervisorChildDeclaration> {
        self.expect_keyword("child")?;
        let name = self.expect_identifier()?;
        self.expect_symbol(':')?;
        let process = self.expect_identifier()?;
        self.expect_symbol('=')?;
        self.expect_keyword("spawn")?;
        let spawn_target = self.expect_identifier()?;
        self.expect_keyword("as")?;
        let mode = self.parse_supervisor_child_mode()?;
        self.expect_symbol(';')?;
        Ok(SupervisorChildDeclaration {
            name,
            process,
            spawn_target,
            mode,
        })
    }

    fn parse_supervisor_child_mode(&mut self) -> Result<SupervisorChildMode> {
        if self.peek_keyword("permanent") {
            self.expect_keyword("permanent")?;
            return Ok(SupervisorChildMode::Permanent);
        }
        if self.peek_keyword("transient") {
            self.expect_keyword("transient")?;
            return Ok(SupervisorChildMode::Transient);
        }
        if self.peek_keyword("temporary") {
            self.expect_keyword("temporary")?;
            return Ok(SupervisorChildMode::Temporary);
        }
        Err(self.error_here("expected child mode permanent, transient, or temporary"))
    }

    fn parse_supervisor_u32_literal(&mut self, field: &str) -> Result<u32> {
        let value = self.parse_supervisor_unsigned_literal(field, "_u32")?;
        u32::try_from(value).map_err(|_| self.error_previous(format!("{field} must fit in U32")))
    }

    fn parse_supervisor_u64_literal(&mut self, field: &str) -> Result<u64> {
        self.parse_supervisor_unsigned_literal(field, "_u64")
    }

    fn parse_supervisor_unsigned_literal(
        &mut self,
        field: &str,
        expected_suffix: &str,
    ) -> Result<u64> {
        let number_offset = self.tokens[self.index].offset;
        let digits = self.expect_number()?;
        let suffix_offset = self.tokens[self.index].offset;
        let suffix = self.expect_ident().map_err(|_| {
            self.error_here(format!(
                "{field} requires explicit {expected_suffix} suffix"
            ))
        })?;
        if suffix_offset != number_offset + digits.len() {
            return Err(
                self.error_previous(format!("{field} suffix must be contiguous with digits"))
            );
        }
        if suffix != expected_suffix {
            return Err(self.error_previous(format!(
                "{field} requires {expected_suffix} suffix, got {suffix:?}"
            )));
        }
        digits
            .parse::<u64>()
            .map_err(|_| self.error_previous(format!("{field} must be a base-10 integer")))
    }

    pub(super) fn parse_function(&mut self) -> Result<Function> {
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
                    return Err(self.error_here("match body must be the whole function body"));
                }
                FunctionBody::Match(match_body)
            } else {
                FunctionBody::Block(Box::new(self.parse_function_block()?))
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
}
