use super::*;

impl Parser {
    pub(super) fn parse_pattern(&mut self, value: String) -> Result<Pattern> {
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
                let items = self.parse_list_pattern_elements()?;
                return Ok(Pattern::List(ListPattern {
                    element_type,
                    capacity,
                    elements: items.elements,
                    rest: items.rest,
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
                let (entries, completeness, rest) = self.parse_map_pattern_entries()?;
                return Ok(Pattern::Map(MapPattern {
                    key_type,
                    value_type,
                    capacity,
                    completeness,
                    rest,
                    entries,
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
                ConstructorPayloadPattern::Destructure(Box::new(Pattern::Constructor {
                    name: Identifier::new(value)?,
                    payload: None,
                }))
            };
            self.expect_symbol(')')?;
            Some(payload)
        } else {
            None
        };
        Ok(Pattern::Constructor { name, payload })
    }

    pub(super) fn starts_payload_destructuring_pattern(&self, value: &str) -> bool {
        value == "_"
            || self.peek_symbol('{')
            || self.peek_symbol('(')
            || ((value == LIST_TYPE || value == MAP_TYPE)
                && (self.peek_symbol('[') || self.peek_symbol('<')))
    }

    pub(super) fn parse_optional_collection_type_args(
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
            } else {
                if capacity.is_some() {
                    return Err(self.error_here(format!(
                        "type {constructor} must declare type arguments before its numeric capacity"
                    )));
                }
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
        if type_args.len() != expected_type_count {
            return Err(self.error_previous(format!(
                "type {constructor} must declare {expected_type_count} type arguments and one numeric capacity"
            )));
        }
        let Some(capacity) = capacity else {
            return Err(self.error_previous(format!(
                "type {constructor} must declare {expected_type_count} type arguments and one numeric capacity"
            )));
        };
        Ok(Some(CollectionTypeArgs {
            types: type_args,
            capacity,
        }))
    }

    pub(super) fn parse_list_pattern_elements(&mut self) -> Result<ListPatternItems> {
        let mut elements = Vec::new();
        if self.consume_symbol(']') {
            return Ok(ListPatternItems {
                elements,
                rest: None,
            });
        }
        if self.consume_dotdot() {
            let rest = self.parse_list_pattern_rest_binding()?;
            self.expect_list_rest_pattern_end()?;
            return Ok(ListPatternItems {
                elements,
                rest: Some(rest),
            });
        }
        let mut rest = None;
        loop {
            elements.push(self.parse_collection_pattern_binding()?);
            if self.consume_symbol(',') {
                if self.consume_dotdot() {
                    rest = Some(self.parse_list_pattern_rest_binding()?);
                    self.expect_list_rest_pattern_end()?;
                    break;
                }
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok(ListPatternItems { elements, rest })
    }

    pub(super) fn parse_list_pattern_rest_binding(&mut self) -> Result<Identifier> {
        let binding = self.expect_ident()?;
        if binding == "_" {
            return Err(self.error_previous(
                "list rest binding cannot be a wildcard; bind the suffix with `..name`",
            ));
        }
        Identifier::new(binding)
    }

    pub(super) fn expect_list_rest_pattern_end(&mut self) -> Result<()> {
        self.consume_symbol(',');
        self.expect_symbol(']')
    }

    pub(super) fn parse_map_pattern_entries(
        &mut self,
    ) -> Result<(
        Vec<MapPatternEntry>,
        MapPatternCompleteness,
        Option<Identifier>,
    )> {
        let mut entries = Vec::new();
        let mut completeness = MapPatternCompleteness::Exact;
        if self.consume_symbol(']') {
            return Ok((entries, completeness, None));
        }
        if self.consume_dotdot() {
            let rest = self.parse_map_pattern_rest_binding()?;
            self.expect_map_subset_pattern_end()?;
            return Ok((entries, MapPatternCompleteness::Subset, rest));
        }
        let mut rest = None;
        loop {
            let key = self.parse_value_expr()?;
            self.expect_fat_arrow()?;
            let binding = self.parse_collection_pattern_binding()?;
            entries.push(MapPatternEntry { key, binding });
            if self.consume_symbol(',') {
                if self.consume_dotdot() {
                    completeness = MapPatternCompleteness::Subset;
                    rest = self.parse_map_pattern_rest_binding()?;
                    self.expect_map_subset_pattern_end()?;
                    break;
                }
                if self.consume_symbol(']') {
                    break;
                }
                continue;
            }
            self.expect_symbol(']')?;
            break;
        }
        Ok((entries, completeness, rest))
    }

    pub(super) fn parse_map_pattern_rest_binding(&mut self) -> Result<Option<Identifier>> {
        if matches!(self.peek_kind(), TokenKind::Ident(_)) {
            let binding = self.expect_ident()?;
            if binding == "_" {
                return Err(self.error_previous(
                    "map rest binding cannot be a wildcard; use `..` to ignore the remainder",
                ));
            }
            return Ok(Some(Identifier::new(binding)?));
        }
        Ok(None)
    }

    pub(super) fn expect_map_subset_pattern_end(&mut self) -> Result<()> {
        self.consume_symbol(',');
        self.expect_symbol(']')
    }

    pub(super) fn parse_collection_pattern_binding(&mut self) -> Result<CollectionPatternBinding> {
        if self.peek_keyword("mut") || self.peek_keyword("var") {
            return Err(self.error_here(
                "collection pattern bindings are immutable; mutable bindings are not supported",
            ));
        }
        let binding = self.expect_ident()?;
        if binding == "_" {
            Ok(CollectionPatternBinding::Wildcard)
        } else if self.starts_payload_destructuring_pattern(&binding) {
            Ok(CollectionPatternBinding::Pattern(Box::new(
                self.parse_pattern(binding)?,
            )))
        } else {
            Ok(CollectionPatternBinding::Binding(Identifier::new(binding)?))
        }
    }

    pub(super) fn parse_record_pattern_fields(
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
}
