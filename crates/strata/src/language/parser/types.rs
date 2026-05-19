use super::*;

impl Parser {
    pub(super) fn parse_type(&mut self) -> Result<TypeRef> {
        self.parse_type_with_depth(0)
    }

    pub(super) fn parse_type_with_depth(&mut self, depth: usize) -> Result<TypeRef> {
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

    pub(super) fn parse_effect_list(&mut self) -> Result<Vec<Effect>> {
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

    pub(super) fn parse_identifier_list(&mut self) -> Result<Vec<Identifier>> {
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
}
