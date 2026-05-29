use super::*;
use crate::language::RESULT_TYPE;

impl Parser {
    const MAX_DIRECT_STATEMENT_IF_DEPTH: usize =
        mantle_artifact::MAX_DIRECT_RUNTIME_IF_ACTION_DEPTH;

    pub(super) fn parse_match_body(&mut self) -> Result<Match> {
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

    pub(super) fn parse_match_arm(&mut self) -> Result<MatchArm> {
        let pattern_name = self.expect_ident()?;
        let pattern = self.parse_pattern(pattern_name)?;
        self.expect_fat_arrow()?;
        self.expect_symbol('{')?;
        let body = self.parse_function_block()?;
        self.expect_symbol('}')?;
        Ok(MatchArm { pattern, body })
    }

    pub(super) fn parse_function_block(&mut self) -> Result<FunctionBlock> {
        let mut statements = Vec::new();
        while !(self.peek_keyword("return")
            || (self.peek_keyword("if") && self.if_starts_return_expr()))
        {
            statements.push(self.parse_function_statement()?);
        }
        let returns = if self.peek_keyword("if") {
            self.parse_return_if_else_expr()?
        } else {
            self.expect_keyword("return")?;
            let returns = self.parse_return_expr()?;
            self.expect_symbol(';')?;
            returns
        };
        Ok(FunctionBlock {
            statements,
            returns,
        })
    }

    pub(super) fn parse_function_statement(&mut self) -> Result<Statement> {
        if self.peek_keyword("match") {
            return Err(self.error_here("match body must be the whole function body"));
        }
        if self.peek_keyword("for") {
            return self.parse_for_each_statement();
        }
        if self.peek_keyword("if") {
            return self.parse_if_else_statement();
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
            if self.peek_keyword("spawn") {
                self.expect_keyword("spawn")?;
                let target = self.expect_identifier()?;
                self.expect_symbol(';')?;
                if is_direct_result_type(&ty) {
                    return Ok(Statement::LetSpawnOutcome { name, ty, target });
                }
                return Ok(Statement::LetProcessRef { name, ty, target });
            }
            if self.peek_keyword("send") {
                self.expect_keyword("send")?;
                let (target, port, message, payload) = self.parse_send_parts()?;
                self.expect_symbol(';')?;
                return Ok(Statement::LetSendOutcome {
                    name,
                    ty,
                    target,
                    port,
                    message,
                    payload,
                });
            }
            let value = self.parse_value_expr()?;
            self.expect_symbol(';')?;
            return Ok(Statement::LetValue { name, ty, value });
        }
        if self.peek_keyword("send") {
            self.expect_keyword("send")?;
            let (target, port, message, payload) = self.parse_send_parts()?;
            self.expect_symbol(';')?;
            return Ok(Statement::Send {
                target,
                port,
                message,
                payload,
            });
        }
        if self.peek_ident_followed_by_symbol('=') {
            return Err(self.error_here(
                "assignment statements are not supported; Strata source bindings are immutable",
            ));
        }
        Err(self.error_here("expected emit, for, if, let, send, or return statement"))
    }

    fn parse_send_parts(
        &mut self,
    ) -> Result<(
        Identifier,
        Option<Identifier>,
        Identifier,
        Option<ValueExpr>,
    )> {
        let target = self.expect_identifier()?;
        let port = if self.peek_keyword("via") {
            self.expect_keyword("via")?;
            Some(self.expect_identifier()?)
        } else {
            None
        };
        let message = self.expect_identifier()?;
        let payload = if self.consume_symbol('(') {
            let value = self.parse_value_expr()?;
            self.expect_symbol(')')?;
            Some(value)
        } else {
            None
        };
        Ok((target, port, message, payload))
    }

    pub(super) fn parse_if_else_statement(&mut self) -> Result<Statement> {
        self.parse_if_else_statement_at_depth(1)
    }

    pub(super) fn parse_if_else_statement_branch(
        &mut self,
        statement_if_depth: usize,
    ) -> Result<Vec<Statement>> {
        self.expect_symbol('{')?;
        let mut statements = Vec::new();
        while !self.peek_symbol('}') {
            if self.peek_keyword("return") {
                return Err(self.error_here("statement-level if branches must not return"));
            }
            if self.peek_keyword("if") {
                if statement_if_depth >= Self::MAX_DIRECT_STATEMENT_IF_DEPTH {
                    return Err(self.error_here(format!(
                        "statement-level if action nesting exceeds maximum depth of {}",
                        Self::MAX_DIRECT_STATEMENT_IF_DEPTH
                    )));
                }
                let nested_depth = statement_if_depth
                    .checked_add(1)
                    .ok_or_else(|| self.error_here("statement-level if nesting overflowed"))?;
                statements.push(self.parse_if_else_statement_at_depth(nested_depth)?);
                continue;
            }
            if self.peek_keyword("let") {
                return Err(self.error_here(
                    "statement-level if branches cannot bind local values or process references",
                ));
            }
            statements.push(self.parse_function_statement()?);
        }
        self.expect_symbol('}')?;
        Ok(statements)
    }

    fn parse_if_else_statement_at_depth(&mut self, statement_if_depth: usize) -> Result<Statement> {
        self.expect_keyword("if")?;
        self.expect_symbol('(')?;
        let condition = self.parse_value_expr()?;
        self.expect_symbol(')')?;
        let then_body = self.parse_if_else_statement_branch(statement_if_depth)?;
        let else_body = if self.peek_keyword("else") {
            self.expect_keyword("else")?;
            self.parse_if_else_statement_branch(statement_if_depth)?
        } else {
            Vec::new()
        };
        Ok(Statement::IfElse {
            condition,
            then_body,
            else_body,
        })
    }

    pub(super) fn parse_for_each_statement(&mut self) -> Result<Statement> {
        self.expect_keyword("for")?;
        let item_name = self.expect_identifier()?;
        let item = if self.consume_symbol('{') {
            ForEachItem::RecordPattern {
                name: item_name.clone(),
                fields: self.parse_record_pattern_fields(&item_name)?,
            }
        } else {
            ForEachItem::Binding(item_name)
        };
        self.expect_keyword("in")?;
        let collection_name = self.expect_identifier()?;
        if !self.peek_symbol('{') {
            return Err(self.error_here("for loop collection must be an identifier binding"));
        }
        let collection = ValueExpr::Identifier(collection_name);
        self.expect_symbol('{')?;
        let mut body = Vec::new();
        while !self.peek_symbol('}') {
            if self.peek_keyword("return") {
                return Err(self.error_here("for loop bodies are statement-only"));
            }
            if self.peek_keyword("for") {
                return Err(self.error_here("nested for loops are not supported"));
            }
            body.push(self.parse_function_statement()?);
        }
        self.expect_symbol('}')?;
        Ok(Statement::ForEach {
            item,
            collection,
            body,
        })
    }

    pub(super) fn parse_return_if_else_expr(&mut self) -> Result<ReturnExpr> {
        self.expect_keyword("if")?;
        self.expect_symbol('(')?;
        let condition = self.parse_value_expr()?;
        self.expect_symbol(')')?;
        self.expect_symbol('{')?;
        let then_branch_start = self.index.saturating_sub(1);
        if !self.block_contains_top_level_return(then_branch_start) {
            return Err(
                self.error_previous("return-if then branch must contain a top-level return")
            );
        }
        let then_branch = self.parse_function_block()?;
        self.expect_symbol('}')?;
        self.expect_keyword("else")?;
        self.expect_symbol('{')?;
        let else_branch_start = self.index.saturating_sub(1);
        if !self.block_contains_top_level_return(else_branch_start) {
            return Err(
                self.error_previous("return-if else branch must contain a top-level return")
            );
        }
        let else_branch = self.parse_function_block()?;
        self.expect_symbol('}')?;
        Ok(ReturnExpr::IfElse {
            condition,
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }

    pub(super) fn parse_return_expr(&mut self) -> Result<ReturnExpr> {
        if self.peek_keyword("match") {
            return Ok(ReturnExpr::Match(self.parse_match_body()?));
        }
        let value = self.parse_value_expr()?;
        if let ValueExpr::Call { name, arg } = value {
            return Ok(ReturnExpr::Call { name, arg: *arg });
        }
        Ok(ReturnExpr::Value(value))
    }
}

fn is_direct_result_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Applied {
            constructor,
            const_args,
            ..
        } if constructor.as_str() == RESULT_TYPE && const_args.is_empty()
    )
}
