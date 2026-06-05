use mantle_artifact::{Error, Result};

fn validate_metadata_string(value: &str, field: &str) -> Result<()> {
    if value.is_empty() || value.len() > mantle_artifact::MAX_FIELD_VALUE_BYTES {
        return Err(Error::new(format!(
            "field {field:?} has invalid metadata length"
        )));
    }
    Ok(())
}

pub(super) struct JsonObject<'a> {
    context: &'static str,
    text: &'a str,
}

impl<'a> JsonObject<'a> {
    pub(super) fn new(text: &'a str, context: &'static str) -> Result<Self> {
        let text = trim_json_ascii_whitespace(text);
        if !(text.starts_with('{') && text.ends_with('}')) {
            return Err(Error::new(format!("{context} must be a JSON object")));
        }
        let object = Self { context, text };
        object.validate_object_shape()?;
        Ok(object)
    }

    pub(super) fn require_exact_fields(&self, fields: &[&str]) -> Result<()> {
        self.for_each_field(|field| {
            if fields.contains(&field) {
                Ok(())
            } else {
                Err(self.error(format!("unknown field {field:?}")))
            }
        })?;
        for field in fields {
            self.require_unique_field(field)?;
        }
        Ok(())
    }

    pub(super) fn required_string(&self, field: &str) -> Result<&'a str> {
        let raw = self.required_raw_string(field)?;
        if raw.as_bytes().contains(&b'\\') {
            return Err(self.error(format!("field {field:?} must be a canonical string")));
        }
        validate_metadata_string(raw, field)?;
        Ok(raw)
    }

    pub(super) fn required_string_eq(&self, field: &str, expected: &str) -> Result<()> {
        let actual = self.required_string(field)?;
        if actual == expected {
            Ok(())
        } else {
            Err(self.error(format!(
                "field {field:?} must be {expected:?}, got {actual:?}"
            )))
        }
    }

    pub(super) fn required_schema_id_with_suffix(
        &self,
        field: &str,
        source_language: &str,
        suffix: &str,
        schema_label: &str,
    ) -> Result<()> {
        let actual = self.required_string(field)?;
        if actual.len() == source_language.len() + suffix.len()
            && actual.starts_with(source_language)
            && &actual[source_language.len()..] == suffix
        {
            Ok(())
        } else {
            Err(self.error(format!(
                "field {field:?} must match source language and {schema_label} schema, got {actual:?}"
            )))
        }
    }

    pub(super) fn required_u32(&self, field: &str) -> Result<u32> {
        parse_u32(self.required_raw(field)?)
            .map_err(|message| self.error(format!("field {field:?} {message}")))
    }

    pub(super) fn required_optional_u32(&self, field: &str) -> Result<Option<u32>> {
        let raw = trim_json_ascii_whitespace(self.required_raw(field)?);
        if raw == "null" {
            Ok(None)
        } else {
            parse_u32(raw)
                .map(Some)
                .map_err(|message| self.error(format!("field {field:?} {message}")))
        }
    }

    pub(super) fn required_u32_eq(&self, field: &str, expected: u32) -> Result<()> {
        let actual = self.required_u32(field)?;
        if actual == expected {
            Ok(())
        } else {
            Err(self.error(format!("field {field:?} must be {expected}, got {actual}")))
        }
    }

    pub(super) fn required_array(&self, field: &'static str) -> Result<JsonArray<'a>> {
        let raw = self.required_raw(field)?;
        if !(raw.starts_with('[') && raw.ends_with(']')) {
            return Err(self.error(format!("field {field:?} must be an array")));
        }
        Ok(JsonArray {
            context: field,
            text: raw,
        })
    }

    pub(super) fn required_object(&self, field: &'static str) -> Result<JsonObject<'a>> {
        let raw = self.required_raw(field)?;
        JsonObject::new(raw, field)
    }

    pub(super) fn required_empty_object(&self, field: &str) -> Result<()> {
        let raw = trim_json_ascii_whitespace(self.required_raw(field)?);
        if raw == "{}" {
            Ok(())
        } else {
            Err(self.error(format!("field {field:?} must be an empty object")))
        }
    }

    fn required_raw_string(&self, field: &str) -> Result<&'a str> {
        let raw = trim_json_ascii_whitespace(self.required_raw(field)?);
        raw.strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .ok_or_else(|| self.error(format!("field {field:?} must be a string")))
    }

    fn required_raw(&self, field: &str) -> Result<&'a str> {
        self.raw_value(field)?
            .ok_or_else(|| self.error(format!("missing field {field:?}")))
    }

    fn require_unique_field(&self, field: &str) -> Result<()> {
        match self.field_count(field)? {
            0 => Err(self.error(format!("missing field {field:?}"))),
            1 => Ok(()),
            _ => Err(self.error(format!("field {field:?} is duplicated"))),
        }
    }

    fn for_each_field(&self, mut visit: impl FnMut(&'a str) -> Result<()>) -> Result<()> {
        self.for_each_raw_field(|field, _| visit(field))
    }

    fn raw_value(&self, field: &str) -> Result<Option<&'a str>> {
        let mut found = None;
        self.for_each_raw_field(|candidate, value| {
            if candidate == field {
                found = Some(value);
            }
            Ok(())
        })?;
        Ok(found)
    }

    fn for_each_raw_field(
        &self,
        mut visit: impl FnMut(&'a str, &'a str) -> Result<()>,
    ) -> Result<()> {
        let bytes = self.text.as_bytes();
        let mut index = 1usize;
        while index < bytes.len() - 1 {
            index = skip_ascii_whitespace(bytes, index);
            if index >= bytes.len() - 1 {
                break;
            }
            if bytes[index] == b',' {
                index += 1;
                continue;
            }
            if bytes[index] != b'"' {
                return Err(self.error("object field name must be a string"));
            }
            let key_start = index + 1;
            let key_end = scan_json_string_end(bytes, key_start)
                .ok_or_else(|| self.error("object field name is malformed"))?;
            index = skip_ascii_whitespace(bytes, key_end + 1);
            if bytes.get(index) != Some(&b':') {
                return Err(self.error("object field separator is missing"));
            }
            index = skip_ascii_whitespace(bytes, index + 1);
            let value_start = index;
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("object field value is malformed"))?;
            visit(
                &self.text[key_start..key_end],
                &self.text[value_start..index],
            )?;
        }
        Ok(())
    }

    fn field_count(&self, field: &str) -> Result<usize> {
        let mut count = 0usize;
        self.for_each_field(|candidate| {
            if candidate == field {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| self.error("field count overflowed"))?;
            }
            Ok(())
        })?;
        Ok(count)
    }

    fn validate_object_shape(&self) -> Result<()> {
        let bytes = self.text.as_bytes();
        let mut index = 1usize;
        loop {
            index = skip_ascii_whitespace(bytes, index);
            if index == bytes.len() - 1 {
                return Ok(());
            }
            if bytes[index] == b',' {
                return Err(self.error("object has an unexpected field separator"));
            }
            if bytes[index] != b'"' {
                return Err(self.error("object field name must be a string"));
            }
            let key_start = index + 1;
            let key_end = scan_json_string_end(bytes, key_start)
                .ok_or_else(|| self.error("object field name is malformed"))?;
            index = skip_ascii_whitespace(bytes, key_end + 1);
            if bytes.get(index) != Some(&b':') {
                return Err(self.error("object field separator is missing"));
            }
            index = skip_ascii_whitespace(bytes, index + 1);
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("object field value is malformed"))?;
            index = skip_ascii_whitespace(bytes, index);
            match bytes.get(index) {
                Some(b',') => {
                    index += 1;
                    if skip_ascii_whitespace(bytes, index) == bytes.len() - 1 {
                        return Err(self.error("object has a trailing separator"));
                    }
                }
                Some(b'}') if index == bytes.len() - 1 => return Ok(()),
                Some(_) => return Err(self.error("object field separator is malformed")),
                None => return Err(self.error("object is unterminated")),
            }
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::new(format!("{}: {}", self.context, message.into()))
    }
}

pub(super) struct JsonArray<'a> {
    context: &'static str,
    text: &'a str,
}

impl<'a> JsonArray<'a> {
    pub(super) fn for_each_object(
        &self,
        mut visit: impl FnMut(usize, JsonObject<'a>) -> Result<()>,
    ) -> Result<()> {
        let bytes = self.text.as_bytes();
        let mut index = skip_ascii_whitespace(bytes, 1);
        if index == bytes.len() - 1 {
            return Ok(());
        }
        let mut item_index = 0usize;
        loop {
            let value_start = index;
            index = skip_json_value(bytes, index).ok_or_else(|| {
                Error::new(format!(
                    "{} array item {item_index} is malformed",
                    self.context
                ))
            })?;
            let raw = &self.text[value_start..index];
            visit(item_index, JsonObject::new(raw, self.context)?)?;
            item_index = item_index
                .checked_add(1)
                .ok_or_else(|| Error::new("array index overflowed"))?;
            index = skip_ascii_whitespace(bytes, index);
            match bytes.get(index) {
                Some(b',') => {
                    index = skip_ascii_whitespace(bytes, index + 1);
                    if index >= bytes.len() - 1 {
                        return Err(Error::new(format!(
                            "{} array has a trailing separator",
                            self.context
                        )));
                    }
                }
                Some(b']') if index == bytes.len() - 1 => return Ok(()),
                _ => {
                    return Err(Error::new(format!(
                        "{} array separator is malformed",
                        self.context
                    )));
                }
            }
        }
    }
}

fn parse_u32(raw: &str) -> std::result::Result<u32, &'static str> {
    let raw = trim_json_ascii_whitespace(raw);
    if !is_json_unsigned_integer(raw) {
        return Err("must be a JSON unsigned integer");
    }
    raw.parse().map_err(|_| "does not fit into u32")
}

fn is_json_unsigned_integer(value: &str) -> bool {
    let mut bytes = value.as_bytes().iter();
    match bytes.next() {
        Some(b'0') => bytes.next().is_none(),
        Some(b'1'..=b'9') => bytes.all(u8::is_ascii_digit),
        _ => false,
    }
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        index += 1;
    }
    index
}

fn trim_json_ascii_whitespace(text: &str) -> &str {
    let bytes = text.as_bytes();
    let start = skip_ascii_whitespace(bytes, 0);
    let mut end = bytes.len();
    while end > start && matches!(bytes[end - 1], b' ' | b'\n' | b'\r' | b'\t') {
        end -= 1;
    }
    &text[start..end]
}

fn scan_json_string_end(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => return None,
            b'"' => return Some(index),
            byte if byte < 0x20 => return None,
            _ => index += 1,
        }
    }
    None
}

fn skip_json_value(bytes: &[u8], index: usize) -> Option<usize> {
    match *bytes.get(index)? {
        b'"' => scan_json_string_end(bytes, index + 1).and_then(|end| end.checked_add(1)),
        b'[' | b'{' => skip_json_container(bytes, index, 0),
        _ => {
            let mut cursor = index;
            while cursor < bytes.len() && !matches!(bytes[cursor], b',' | b'}' | b']') {
                cursor += 1;
            }
            (cursor > index).then_some(cursor)
        }
    }
}

fn skip_json_container(bytes: &[u8], index: usize, depth: usize) -> Option<usize> {
    if depth >= 64 {
        return None;
    }
    let opener = *bytes.get(index)?;
    let closer = match opener {
        b'[' => b']',
        b'{' => b'}',
        _ => return None,
    };
    let wrong = if opener == b'[' { b'}' } else { b']' };
    let mut cursor = index.checked_add(1)?;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => cursor = scan_json_string_end(bytes, cursor + 1)?.checked_add(1)?,
            b'[' | b'{' => cursor = skip_json_container(bytes, cursor, depth.checked_add(1)?)?,
            value if value == closer => return cursor.checked_add(1),
            value if value == wrong => return None,
            _ => cursor += 1,
        }
    }
    None
}
