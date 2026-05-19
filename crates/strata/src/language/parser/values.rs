use super::*;

impl Parser {
    pub(super) fn parse_value_expr(&mut self) -> Result<ValueExpr> {
        self.parse_value_expr_with_depth(0)
    }

    pub(super) fn parse_value_expr_with_depth(&mut self, depth: usize) -> Result<ValueExpr> {
        self.parse_value_or_expr_with_depth(depth)
    }

    pub(super) fn parse_value_or_expr_with_depth(&mut self, depth: usize) -> Result<ValueExpr> {
        let mut value = self.parse_value_and_expr_with_depth(depth)?;
        let mut composition_depth = depth;
        while matches!(self.peek_kind(), TokenKind::PipePipe) {
            self.advance();
            composition_depth = self.next_value_depth(composition_depth)?;
            let right = self.parse_value_and_expr_with_depth(composition_depth)?;
            value = ValueExpr::BooleanBinary {
                operator: ValueBooleanOperator::Or,
                left: Box::new(value),
                right: Box::new(right),
            };
        }
        Ok(value)
    }

    pub(super) fn parse_value_and_expr_with_depth(&mut self, depth: usize) -> Result<ValueExpr> {
        let mut value = self.parse_value_equality_expr_with_depth(depth)?;
        let mut composition_depth = depth;
        while matches!(self.peek_kind(), TokenKind::AmpAmp) {
            self.advance();
            composition_depth = self.next_value_depth(composition_depth)?;
            let right = self.parse_value_equality_expr_with_depth(composition_depth)?;
            value = ValueExpr::BooleanBinary {
                operator: ValueBooleanOperator::And,
                left: Box::new(value),
                right: Box::new(right),
            };
        }
        Ok(value)
    }

    pub(super) fn parse_value_equality_expr_with_depth(
        &mut self,
        depth: usize,
    ) -> Result<ValueExpr> {
        let left = self.parse_value_unary_expr_with_depth(depth)?;
        if let Some(operator) = self.consume_value_equality_operator() {
            let right = self.parse_value_unary_expr_with_depth(self.next_value_depth(depth)?)?;
            if self.peek_value_equality_operator() {
                return Err(self.error_here(
                    "chained equality expressions are not supported in this source slice",
                ));
            }
            return Ok(ValueExpr::Equality {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_value_unary_expr_with_depth(&mut self, depth: usize) -> Result<ValueExpr> {
        if self.consume_symbol('!') {
            let operand = self.parse_value_unary_expr_with_depth(self.next_value_depth(depth)?)?;
            return Ok(ValueExpr::BooleanNot {
                operand: Box::new(operand),
            });
        }
        self.parse_value_primary_expr_with_depth(depth)
    }

    pub(super) fn parse_value_primary_expr_with_depth(
        &mut self,
        depth: usize,
    ) -> Result<ValueExpr> {
        if depth > MAX_VALUE_NESTING {
            return Err(self.error_here(format!(
                "value nesting exceeds maximum depth of {MAX_VALUE_NESTING}"
            )));
        }
        if self.consume_symbol('(') {
            let value = self.parse_value_expr_with_depth(self.next_value_depth(depth)?)?;
            self.expect_symbol(')')?;
            return Ok(ValueExpr::Grouped {
                value: Box::new(value),
            });
        }
        if self.peek_keyword("match") {
            return Err(self.error_here(
                "match expressions are only admitted as whole function bodies or return match expressions in this source slice",
            ));
        }
        if self.peek_keyword("if") {
            return self.parse_if_else_value_expr(depth);
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

    pub(super) fn parse_if_else_value_expr(&mut self, depth: usize) -> Result<ValueExpr> {
        self.expect_keyword("if")?;
        self.expect_symbol('(')?;
        let condition = self.parse_value_expr_with_depth(depth + 1)?;
        self.expect_symbol(')')?;
        let then_branch = self.parse_if_else_branch_value(depth)?;
        self.expect_keyword("else")?;
        let else_branch = self.parse_if_else_branch_value(depth)?;
        Ok(ValueExpr::IfElse {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }

    pub(super) fn parse_if_else_branch_value(&mut self, depth: usize) -> Result<ValueExpr> {
        self.expect_symbol('{')?;
        if self.peek_keyword("emit")
            || self.peek_keyword("let")
            || self.peek_keyword("return")
            || self.peek_keyword("send")
        {
            return Err(self.error_here(
                "if branches are pure value expressions and must not perform statements",
            ));
        }
        let value = self.parse_value_expr_with_depth(depth + 1)?;
        if self.consume_symbol(';')
            || self.peek_keyword("emit")
            || self.peek_keyword("let")
            || self.peek_keyword("return")
            || self.peek_keyword("send")
        {
            return Err(self.error_here(
                "if branches are pure value expressions and must not perform statements",
            ));
        }
        self.expect_symbol('}')?;
        Ok(value)
    }

    pub(super) fn parse_list_value_items(&mut self, depth: usize) -> Result<Vec<ValueExpr>> {
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

    pub(super) fn parse_map_value_entries(&mut self, depth: usize) -> Result<Vec<MapValueEntry>> {
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

    pub(super) fn parse_record_value_fields(
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
}
