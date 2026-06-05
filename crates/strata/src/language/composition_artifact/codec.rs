use std::borrow::Cow;

use super::super::diagnostic::{Error, Result};

const MAX_JSON_CONTAINER_DEPTH: usize = 64;

pub(in crate::language) struct JsonObject<'a> {
    context: String,
    text: &'a str,
}

impl<'a> JsonObject<'a> {
    pub(in crate::language) fn new(text: &'a str, context: impl Into<String>) -> Result<Self> {
        let text = trim_json_ascii_whitespace(text);
        if !(text.starts_with('{') && text.ends_with('}')) {
            return Err(Error::new(format!(
                "{} must be a JSON object",
                context.into()
            )));
        }
        let object = Self {
            context: context.into(),
            text,
        };
        object.validate_object_shape()?;
        Ok(object)
    }

    pub(in crate::language) fn require_exact_fields(&self, fields: &[&str]) -> Result<()> {
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

    pub(in crate::language) fn required_string(&self, field: &str) -> Result<Cow<'a, str>> {
        match self.required_value(field)? {
            JsonValue::String(value) => decode_json_string_body(value)
                .map_err(|message| self.error(format!("field {field:?} {message}"))),
            _ => Err(self.error(format!("field {field:?} must be a string"))),
        }
    }

    pub(in crate::language) fn required_string_eq(
        &self,
        field: &str,
        expected: &str,
    ) -> Result<()> {
        let actual = self.required_string(field)?;
        if actual.as_ref() == expected {
            Ok(())
        } else {
            Err(self.error(format!(
                "field {field:?} must be {expected:?}, got {actual:?}"
            )))
        }
    }

    pub(in crate::language) fn required_u32(&self, field: &str) -> Result<u32> {
        match self.required_value(field)? {
            JsonValue::Number(value) => {
                parse_u32(value).map_err(|message| self.error(format!("field {field:?} {message}")))
            }
            _ => Err(self.error(format!("field {field:?} must be an unsigned integer"))),
        }
    }

    pub(in crate::language) fn required_optional_u32(&self, field: &str) -> Result<Option<u32>> {
        let raw = self
            .raw_value(field)?
            .ok_or_else(|| self.error(format!("missing field {field:?}")))?;
        if trim_json_ascii_whitespace(raw) == "null" {
            return Ok(None);
        }
        match JsonValue::parse(raw) {
            JsonValue::Number(value) => parse_u32(value)
                .map(Some)
                .map_err(|message| self.error(format!("field {field:?} {message}"))),
            _ => Err(self.error(format!(
                "field {field:?} must be an unsigned integer or null"
            ))),
        }
    }

    pub(in crate::language) fn required_array(&self, field: &str) -> Result<JsonArray<'a>> {
        match self.required_value(field)? {
            JsonValue::Array(value) => JsonArray::new(value, format!("{}.{}", self.context, field)),
            _ => Err(self.error(format!("field {field:?} must be an array"))),
        }
    }

    pub(in crate::language) fn required_object(&self, field: &str) -> Result<JsonObject<'a>> {
        match self.required_value(field)? {
            JsonValue::Object(value) => {
                JsonObject::new(value, format!("{}.{}", self.context, field))
            }
            _ => Err(self.error(format!("field {field:?} must be an object"))),
        }
    }

    pub(in crate::language) fn required_null(&self, field: &str) -> Result<()> {
        match self.raw_value(field)? {
            Some(value) if trim_json_ascii_whitespace(value) == "null" => Ok(()),
            Some(_) => Err(self.error(format!("field {field:?} must be null"))),
            None => Err(self.error(format!("missing field {field:?}"))),
        }
    }

    pub(in crate::language) fn required_empty_object(&self, field: &str) -> Result<()> {
        self.required_object(field)?.require_exact_fields(&[])
    }

    fn require_unique_field(&self, field: &str) -> Result<()> {
        match self.field_count(field)? {
            0 => Err(self.error(format!("missing field {field:?}"))),
            1 => Ok(()),
            _ => Err(self.error(format!("field {field:?} is duplicated"))),
        }
    }

    fn required_value(&self, field: &str) -> Result<JsonValue<'a>> {
        self.value(field)?
            .ok_or_else(|| self.error(format!("missing field {field:?}")))
    }

    fn value(&self, field: &str) -> Result<Option<JsonValue<'a>>> {
        self.raw_value(field)
            .map(|value| value.map(JsonValue::parse))
    }

    fn raw_value(&self, field: &str) -> Result<Option<&'a str>> {
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
            if &self.text[key_start..key_end] == field {
                return Ok(Some(&self.text[value_start..index]));
            }
        }
        Ok(None)
    }

    fn for_each_field(&self, mut visit: impl FnMut(&'a str) -> Result<()>) -> Result<()> {
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
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("object field value is malformed"))?;
            visit(&self.text[key_start..key_end])?;
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
            if bytes[key_start..key_end].contains(&b'\\') {
                return Err(self.error("object field name escapes are unsupported"));
            }
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

pub(in crate::language) struct JsonArray<'a> {
    context: String,
    text: &'a str,
}

impl<'a> JsonArray<'a> {
    fn new(text: &'a str, context: impl Into<String>) -> Result<Self> {
        let text = trim_json_ascii_whitespace(text);
        if !(text.starts_with('[') && text.ends_with(']')) {
            return Err(Error::new(format!(
                "{} must be a JSON array",
                context.into()
            )));
        }
        Ok(Self {
            context: context.into(),
            text,
        })
    }

    pub(in crate::language) fn count_values(&self) -> Result<usize> {
        let mut count = 0usize;
        self.for_each_raw_value(|_, _| {
            count = count
                .checked_add(1)
                .ok_or_else(|| self.error("array count overflowed"))?;
            Ok(())
        })?;
        Ok(count)
    }

    pub(in crate::language) fn for_each_object(
        &self,
        mut visit: impl FnMut(usize, JsonObject<'a>) -> Result<()>,
    ) -> Result<()> {
        self.for_each_raw_value(|index, value| match JsonValue::parse(value) {
            JsonValue::Object(_) => visit(
                index,
                JsonObject::new(value, format!("{}[{index}]", self.context))?,
            ),
            _ => Err(self.error(format!("array item {index} must be an object"))),
        })
    }

    fn for_each_raw_value(
        &self,
        mut visit: impl FnMut(usize, &'a str) -> Result<()>,
    ) -> Result<()> {
        let bytes = self.text.as_bytes();
        let mut index = skip_ascii_whitespace(bytes, 1);
        if index == bytes.len() - 1 {
            return Ok(());
        }
        let mut item_index = 0usize;
        loop {
            let value_start = index;
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error(format!("array item {item_index} is malformed")))?;
            visit(item_index, &self.text[value_start..index])?;
            item_index = item_index
                .checked_add(1)
                .ok_or_else(|| self.error("array index overflowed"))?;
            index = skip_ascii_whitespace(bytes, index);
            match bytes.get(index) {
                Some(b',') => {
                    index = skip_ascii_whitespace(bytes, index + 1);
                    if index >= bytes.len() - 1 {
                        return Err(self.error("array has a trailing separator"));
                    }
                }
                Some(b']') if index == bytes.len() - 1 => return Ok(()),
                Some(_) => return Err(self.error("array separator is malformed")),
                None => return Err(self.error("array is unterminated")),
            }
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::new(format!("{}: {}", self.context, message.into()))
    }
}

#[derive(Clone, Copy)]
enum JsonValue<'a> {
    String(&'a str),
    Number(&'a str),
    Array(&'a str),
    Object(&'a str),
    Other,
}

impl<'a> JsonValue<'a> {
    fn parse(raw: &'a str) -> Self {
        let raw = trim_json_ascii_whitespace(raw);
        if let Some(value) = raw
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            Self::String(value)
        } else if raw.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            Self::Number(raw)
        } else if raw.starts_with('[') {
            Self::Array(raw)
        } else if raw.starts_with('{') {
            Self::Object(raw)
        } else {
            Self::Other
        }
    }
}

fn parse_u32(value: &str) -> std::result::Result<u32, &'static str> {
    if !is_json_unsigned_integer(value) {
        return Err("must be a JSON unsigned integer");
    }
    value.parse().map_err(|_| "does not fit into u32")
}

fn decode_json_string_body(value: &str) -> std::result::Result<Cow<'_, str>, &'static str> {
    if !value.as_bytes().contains(&b'\\') {
        return Ok(Cow::Borrowed(value));
    }
    let mut decoded = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            let ch = value[index..]
                .chars()
                .next()
                .ok_or("has malformed UTF-8 contents")?;
            decoded.push(ch);
            index += ch.len_utf8();
            continue;
        }
        index = index.checked_add(1).ok_or("escape index overflowed")?;
        let escaped = *bytes.get(index).ok_or("has an unterminated escape")?;
        match escaped {
            b'"' => decoded.push('"'),
            b'\\' => decoded.push('\\'),
            b'/' => decoded.push('/'),
            b'b' => decoded.push('\u{08}'),
            b'f' => decoded.push('\u{0c}'),
            b'n' => decoded.push('\n'),
            b'r' => decoded.push('\r'),
            b't' => decoded.push('\t'),
            b'u' => {
                let (ch, next_index) = decode_unicode_escape(bytes, index)?;
                decoded.push(ch);
                index = next_index;
                continue;
            }
            _ => return Err("has an unsupported escape"),
        }
        index = index.checked_add(1).ok_or("escape index overflowed")?;
    }
    Ok(Cow::Owned(decoded))
}

fn decode_unicode_escape(
    bytes: &[u8],
    escape_index: usize,
) -> std::result::Result<(char, usize), &'static str> {
    let first = parse_json_u16_escape(bytes, escape_index)?;
    let next_index = escape_index
        .checked_add(5)
        .ok_or("unicode escape index overflowed")?;
    match first {
        0xD800..=0xDBFF => {
            let marker_end = next_index
                .checked_add(2)
                .ok_or("unicode escape index overflowed")?;
            if bytes.get(next_index..marker_end) != Some(b"\\u") {
                return Err("has an unpaired high surrogate escape");
            }
            let second_escape = next_index
                .checked_add(1)
                .ok_or("unicode escape index overflowed")?;
            let second = parse_json_u16_escape(bytes, second_escape)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err("has an unpaired high surrogate escape");
            }
            let high = u32::from(first - 0xD800);
            let low = u32::from(second - 0xDC00);
            let codepoint = 0x1_0000 + ((high << 10) | low);
            let ch = char::from_u32(codepoint).ok_or("has an invalid unicode escape")?;
            let final_index = second_escape
                .checked_add(5)
                .ok_or("unicode escape index overflowed")?;
            Ok((ch, final_index))
        }
        0xDC00..=0xDFFF => Err("has an unpaired low surrogate escape"),
        value => {
            let ch = char::from_u32(u32::from(value)).ok_or("has an invalid unicode escape")?;
            Ok((ch, next_index))
        }
    }
}

fn parse_json_u16_escape(
    bytes: &[u8],
    escape_index: usize,
) -> std::result::Result<u16, &'static str> {
    let start = escape_index
        .checked_add(1)
        .ok_or("unicode escape index overflowed")?;
    let end = start
        .checked_add(4)
        .ok_or("unicode escape index overflowed")?;
    let digits = bytes
        .get(start..end)
        .ok_or("has a truncated unicode escape")?;
    let mut value = 0u16;
    for digit in digits {
        value = value
            .checked_mul(16)
            .and_then(|value| {
                hex_value(*digit)
                    .map(u16::from)
                    .and_then(|digit| value.checked_add(digit))
            })
            .ok_or("unicode escape value overflowed")?;
    }
    Ok(value)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
            b'\\' => index = validate_json_escape(bytes, index.checked_add(1)?)?,
            b'"' => return Some(index),
            byte if byte < 0x20 => return None,
            _ => index += 1,
        }
    }
    None
}

fn validate_json_escape(bytes: &[u8], index: usize) -> Option<usize> {
    match *bytes.get(index)? {
        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => index.checked_add(1),
        b'u' => {
            let end = index.checked_add(5)?;
            bytes
                .get(index + 1..end)?
                .iter()
                .all(u8::is_ascii_hexdigit)
                .then_some(end)
        }
        _ => None,
    }
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
    if depth >= MAX_JSON_CONTAINER_DEPTH {
        return None;
    }
    let opener = *bytes.get(index)?;
    let closer = match opener {
        b'[' => b']',
        b'{' => b'}',
        _ => return None,
    };
    let wrong_closer = match opener {
        b'[' => b'}',
        b'{' => b']',
        _ => return None,
    };
    let mut cursor = index.checked_add(1)?;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => cursor = scan_json_string_end(bytes, cursor + 1)?.checked_add(1)?,
            b'[' | b'{' => cursor = skip_json_container(bytes, cursor, depth.checked_add(1)?)?,
            value if value == closer => return cursor.checked_add(1),
            value if value == wrong_closer => return None,
            _ => cursor += 1,
        }
    }
    None
}
