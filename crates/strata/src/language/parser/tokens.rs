use super::*;
use mantle_artifact::MAX_VALUE_TEMPLATE_DEPTH;

impl Parser {
    pub(super) fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        if self.peek_keyword(keyword) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here(format!("expected keyword {keyword}")))
        }
    }

    pub(super) fn peek_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(value) if value == keyword)
    }

    pub(super) fn expect_ident(&mut self) -> Result<String> {
        if let TokenKind::Ident(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected identifier"))
        }
    }

    pub(super) fn expect_identifier(&mut self) -> Result<Identifier> {
        Identifier::new(self.expect_ident()?)
    }

    pub(super) fn expect_number(&mut self) -> Result<String> {
        if let TokenKind::Number(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected number"))
        }
    }

    pub(super) fn peek_number(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Number(_))
    }

    pub(super) fn parse_capacity_arg(&mut self, constructor: &Identifier) -> Result<usize> {
        let capacity = self.expect_number()?;
        capacity.parse::<usize>().map_err(|_| {
            self.error_previous(format!(
                "type {constructor} numeric capacity must fit in usize"
            ))
        })
    }

    pub(super) fn expect_string_literal(&mut self) -> Result<String> {
        if let TokenKind::StringLiteral(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected string literal"))
        }
    }

    pub(super) fn expect_at_ident(&mut self) -> Result<String> {
        if let TokenKind::AtIdent(value) = self.peek_kind() {
            let value = value.clone();
            self.advance();
            Ok(value)
        } else {
            Err(self.error_here("expected @identifier"))
        }
    }

    pub(super) fn expect_arrow(&mut self) -> Result<()> {
        if matches!(self.peek_kind(), TokenKind::Arrow) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here("expected ->"))
        }
    }

    pub(super) fn expect_fat_arrow(&mut self) -> Result<()> {
        if matches!(self.peek_kind(), TokenKind::FatArrow) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here("expected =>"))
        }
    }

    pub(super) fn consume_value_equality_operator(&mut self) -> Option<ValueEqualityOperator> {
        match self.peek_kind() {
            TokenKind::EqualEqual => {
                self.advance();
                Some(ValueEqualityOperator::Equal)
            }
            TokenKind::BangEqual => {
                self.advance();
                Some(ValueEqualityOperator::NotEqual)
            }
            _ => None,
        }
    }

    pub(super) fn peek_value_equality_operator(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::EqualEqual | TokenKind::BangEqual
        )
    }

    pub(super) fn consume_value_ordering_operator(
        &mut self,
    ) -> Option<ValueScalarOrderingOperator> {
        match self.peek_kind() {
            TokenKind::Symbol('<') => {
                self.advance();
                Some(ValueScalarOrderingOperator::Less)
            }
            TokenKind::LessEqual => {
                self.advance();
                Some(ValueScalarOrderingOperator::LessEqual)
            }
            TokenKind::Symbol('>') => {
                self.advance();
                Some(ValueScalarOrderingOperator::Greater)
            }
            TokenKind::GreaterEqual => {
                self.advance();
                Some(ValueScalarOrderingOperator::GreaterEqual)
            }
            _ => None,
        }
    }

    pub(super) fn peek_value_ordering_operator(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Symbol('<')
                | TokenKind::LessEqual
                | TokenKind::Symbol('>')
                | TokenKind::GreaterEqual
        )
    }

    pub(super) fn consume_value_additive_operator(
        &mut self,
    ) -> Option<ValueScalarArithmeticOperator> {
        match self.peek_kind() {
            TokenKind::Symbol('+') => {
                self.advance();
                Some(ValueScalarArithmeticOperator::Add)
            }
            TokenKind::Symbol('-') => {
                self.advance();
                Some(ValueScalarArithmeticOperator::Subtract)
            }
            _ => None,
        }
    }

    pub(super) fn consume_value_multiplicative_operator(
        &mut self,
    ) -> Option<ValueScalarArithmeticOperator> {
        match self.peek_kind() {
            TokenKind::Symbol('*') => {
                self.advance();
                Some(ValueScalarArithmeticOperator::Multiply)
            }
            TokenKind::Symbol('/') => {
                self.advance();
                Some(ValueScalarArithmeticOperator::Divide)
            }
            TokenKind::Symbol('%') => {
                self.advance();
                Some(ValueScalarArithmeticOperator::Modulo)
            }
            _ => None,
        }
    }

    pub(super) fn expect_symbol(&mut self, symbol: char) -> Result<()> {
        if self.consume_symbol(symbol) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected symbol {symbol:?}")))
        }
    }

    pub(super) fn consume_symbol(&mut self, symbol: char) -> bool {
        if matches!(self.peek_kind(), TokenKind::Symbol(value) if *value == symbol) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn consume_dotdot(&mut self) -> bool {
        if matches!(self.peek_kind(), TokenKind::DotDot) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn reject_braced_type_semicolon(&mut self, declaration_kind: &str) -> Result<()> {
        if self.peek_symbol(';') {
            return Err(self.error_here(format!(
                "braced {declaration_kind} declarations are terminated by '}}', not ';'"
            )));
        }
        Ok(())
    }

    pub(super) fn peek_symbol(&self, symbol: char) -> bool {
        matches!(self.peek_kind(), TokenKind::Symbol(value) if *value == symbol)
    }

    pub(super) fn peek_ident_followed_by_symbol(&self, symbol: char) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Symbol(value)) if *value == symbol
            )
    }

    pub(super) fn if_starts_return_expr(&self) -> bool {
        self.if_starts_return_expr_at(self.index, 0)
    }

    fn if_starts_return_expr_at(&self, if_index: usize, probe_depth: usize) -> bool {
        if probe_depth > MAX_VALUE_TEMPLATE_DEPTH {
            return false;
        }
        if !matches!(
            self.tokens.get(if_index).map(|token| &token.kind),
            Some(TokenKind::Ident(value)) if value == "if"
        ) {
            return false;
        }
        let Some(condition_start) = if_index.checked_add(1) else {
            return false;
        };
        if !matches!(
            self.tokens.get(condition_start).map(|token| &token.kind),
            Some(TokenKind::Symbol('('))
        ) {
            return false;
        }

        let Some(condition_end) = self.matching_symbol_index(condition_start, '(', ')') else {
            return false;
        };
        let branch_start = condition_end + 1;
        if !matches!(
            self.tokens.get(branch_start).map(|token| &token.kind),
            Some(TokenKind::Symbol('{'))
        ) {
            return false;
        }
        if !self.block_contains_terminal_return(branch_start, probe_depth + 1) {
            return false;
        }
        let Some(branch_end) = self.matching_symbol_index(branch_start, '{', '}') else {
            return false;
        };
        let Some(else_index) = branch_end.checked_add(1) else {
            return false;
        };
        if !matches!(
            self.tokens.get(else_index).map(|token| &token.kind),
            Some(TokenKind::Ident(value)) if value == "else"
        ) {
            return false;
        }
        let Some(else_branch_start) = else_index.checked_add(1) else {
            return false;
        };
        if !matches!(
            self.tokens.get(else_branch_start).map(|token| &token.kind),
            Some(TokenKind::Symbol('{'))
        ) {
            return false;
        }
        let Some(else_branch_end) = self.matching_symbol_index(else_branch_start, '{', '}') else {
            return false;
        };
        let Some(after_else_branch) = else_branch_end.checked_add(1) else {
            return false;
        };
        matches!(
            self.tokens.get(after_else_branch).map(|token| &token.kind),
            Some(TokenKind::Symbol('}')) | Some(TokenKind::Eof)
        )
    }

    pub(super) fn block_contains_top_level_return(&self, block_start: usize) -> bool {
        self.block_contains_terminal_return(block_start, 0)
    }

    fn block_contains_terminal_return(&self, block_start: usize, probe_depth: usize) -> bool {
        if probe_depth > MAX_VALUE_TEMPLATE_DEPTH {
            return false;
        }
        let mut depth = 0usize;
        let mut index = block_start;
        while let Some(token) = self.tokens.get(index) {
            match &token.kind {
                TokenKind::Symbol('{') => depth = depth.saturating_add(1),
                TokenKind::Symbol('}') => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::Ident(value) if value == "return" && depth == 1 => return true,
                TokenKind::Ident(value)
                    if value == "if"
                        && depth == 1
                        && self.if_starts_return_expr_at(index, probe_depth + 1) =>
                {
                    return true;
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            index += 1;
        }
        false
    }

    pub(super) fn matching_symbol_index(
        &self,
        start: usize,
        open: char,
        close: char,
    ) -> Option<usize> {
        let mut depth = 0usize;
        for (index, token) in self.tokens.iter().enumerate().skip(start) {
            match &token.kind {
                TokenKind::Symbol(value) if *value == open => depth = depth.checked_add(1)?,
                TokenKind::Symbol(value) if *value == close => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                TokenKind::Eof => return None,
                _ => {}
            }
        }
        None
    }

    pub(super) fn advance(&mut self) {
        if !self.at_eof() {
            self.index += 1;
        }
    }

    pub(super) fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.index].kind
    }

    pub(super) fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    pub(super) fn error_here(&self, message: impl Into<String>) -> Error {
        Error::new(format!(
            "{} at byte {}",
            message.into(),
            self.tokens[self.index].offset
        ))
    }

    pub(super) fn error_previous(&self, message: impl Into<String>) -> Error {
        let offset = self
            .tokens
            .get(self.index.saturating_sub(1))
            .map(|token| token.offset)
            .unwrap_or(0);
        Error::new(format!("{} at byte {offset}", message.into()))
    }

    pub(super) fn next_value_depth(&self, depth: usize) -> Result<usize> {
        let next = depth.checked_add(1).ok_or_else(|| {
            self.error_here(format!(
                "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
            ))
        })?;
        if next > MAX_VALUE_NESTING {
            return Err(self.error_here(format!(
                "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
            )));
        }
        Ok(next)
    }
}
