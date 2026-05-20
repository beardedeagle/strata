use super::*;

impl Parser {
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
            return Err(
                self.error_here("match body must be the whole function body in this source slice")
            );
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
        if self.peek_ident_followed_by_symbol('=') {
            return Err(self.error_here(
                "assignment statements are not supported; Strata source bindings are immutable",
            ));
        }
        Err(self.error_here("expected emit, for, if, let, send, or return statement"))
    }

    pub(super) fn parse_if_else_statement(&mut self) -> Result<Statement> {
        self.expect_keyword("if")?;
        self.expect_symbol('(')?;
        let condition = self.parse_value_expr()?;
        self.expect_symbol(')')?;
        let then_body = self.parse_if_else_statement_branch()?;
        let else_body = if self.peek_keyword("else") {
            self.expect_keyword("else")?;
            self.parse_if_else_statement_branch()?
        } else {
            Vec::new()
        };
        Ok(Statement::IfElse {
            condition,
            then_body,
            else_body,
        })
    }

    pub(super) fn parse_if_else_statement_branch(&mut self) -> Result<Vec<Statement>> {
        self.expect_symbol('{')?;
        let mut statements = Vec::new();
        while !self.peek_symbol('}') {
            if self.peek_keyword("return") {
                return Err(self.error_here(
                    "statement-level if branches must not return in this source slice",
                ));
            }
            if self.peek_keyword("if") {
                return Err(self.error_here(
                    "nested statement-level if branches are not supported in this source slice",
                ));
            }
            if self.peek_keyword("let") {
                return Err(
                    self.error_here("statement-level if branches cannot bind process references")
                );
            }
            statements.push(self.parse_function_statement()?);
        }
        self.expect_symbol('}')?;
        Ok(statements)
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
            return Err(self.error_here(
                "for loop collection must be an identifier binding in this source slice",
            ));
        }
        let collection = ValueExpr::Identifier(collection_name);
        self.expect_symbol('{')?;
        let mut body = Vec::new();
        while !self.peek_symbol('}') {
            if self.peek_keyword("return") {
                return Err(
                    self.error_here("for loop bodies are statement-only in this source slice")
                );
            }
            if self.peek_keyword("for") {
                return Err(
                    self.error_here("nested for loops are not supported in this source slice")
                );
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
            return Err(self
                .error_previous("runtime return if then branch must contain a top-level return"));
        }
        let then_branch = self.parse_function_block()?;
        self.expect_symbol('}')?;
        self.expect_keyword("else")?;
        self.expect_symbol('{')?;
        let else_branch_start = self.index.saturating_sub(1);
        if !self.block_contains_top_level_return(else_branch_start) {
            return Err(self
                .error_previous("runtime return if else branch must contain a top-level return"));
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
