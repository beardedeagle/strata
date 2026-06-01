use mantle_artifact::{Error, Result};

const MAX_JSON_CONTAINER_DEPTH: usize = 64;

pub(super) struct JsonLine<'a> {
    number: usize,
    text: &'a str,
}

impl<'a> JsonLine<'a> {
    pub(super) fn new(number: usize, text: &'a str) -> Result<Self> {
        let text = trim_json_ascii_whitespace(text);
        if text.is_empty() {
            return Err(Error::new(format!("runtime trace line {number} is empty")));
        }
        if !(text.starts_with('{') && text.ends_with('}')) {
            return Err(Error::new(format!(
                "runtime trace line {number} is not a JSON object"
            )));
        }
        let line = Self { number, text };
        line.validate_object_shape()?;
        Ok(line)
    }

    pub(super) fn for_each_field(
        &self,
        mut visit: impl FnMut(&'a str) -> Result<()>,
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
                return Err(self.error("runtime trace object field name must be a string"));
            }
            let key_start = index + 1;
            let key_end = scan_json_string_end(bytes, key_start)
                .ok_or_else(|| self.error("runtime trace object field name is unterminated"))?;
            index = skip_ascii_whitespace(bytes, key_end + 1);
            if bytes.get(index) != Some(&b':') {
                return Err(self.error("runtime trace object field separator is missing"));
            }
            index = skip_ascii_whitespace(bytes, index + 1);
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("runtime trace object field value is malformed"))?;
            visit(&self.text[key_start..key_end])?;
        }
        Ok(())
    }

    pub(super) fn require_unique_field(&self, field: &str) -> Result<()> {
        match self.field_count(field)? {
            0 => Err(self.error(format!("runtime trace event is missing field {field:?}"))),
            1 => Ok(()),
            _ => Err(self.error(format!("runtime trace field {field:?} is duplicated"))),
        }
    }

    pub(super) fn require_unique_optional_field(&self, field: &str) -> Result<()> {
        if self.field_count(field)? > 1 {
            return Err(self.error(format!("runtime trace field {field:?} is duplicated")));
        }
        Ok(())
    }

    pub(super) fn required_string(&self, field: &str) -> Result<&'a str> {
        match self.required_value(field)? {
            JsonValue::String(value) => Ok(value),
            _ => Err(self.error(format!("runtime trace field {field:?} must be a string"))),
        }
    }

    pub(super) fn optional_string(&self, field: &str) -> Result<Option<&'a str>> {
        match self.value(field)? {
            Some(JsonValue::String(value)) => Ok(Some(value)),
            None => Ok(None),
            Some(_) => Err(self.error(format!("runtime trace field {field:?} must be a string"))),
        }
    }

    pub(super) fn required_u64(&self, field: &str) -> Result<u64> {
        match self.required_value(field)? {
            JsonValue::Number(value) => parse_u64(value)
                .map_err(|message| self.error(format!("runtime trace field {field:?} {message}"))),
            _ => Err(self.error(format!(
                "runtime trace field {field:?} must be an unsigned integer"
            ))),
        }
    }

    pub(super) fn optional_u64_or_null(&self, field: &str) -> Result<Option<u64>> {
        match self.value(field)? {
            Some(JsonValue::Number(value)) => parse_u64(value)
                .map(Some)
                .map_err(|message| self.error(format!("runtime trace field {field:?} {message}"))),
            Some(JsonValue::Null) | None => Ok(None),
            Some(_) => Err(self.error(format!(
                "runtime trace field {field:?} must be an unsigned integer or null"
            ))),
        }
    }

    pub(super) fn optional_u64(&self, field: &str) -> Result<Option<u64>> {
        match self.value(field)? {
            Some(JsonValue::Number(value)) => parse_u64(value)
                .map(Some)
                .map_err(|message| self.error(format!("runtime trace field {field:?} {message}"))),
            None => Ok(None),
            Some(_) => Err(self.error(format!(
                "runtime trace field {field:?} must be an unsigned integer"
            ))),
        }
    }

    pub(super) fn required_bounded_u16_array(
        &self,
        field: &str,
        max_segments: usize,
        visit: impl FnMut(u16) -> std::result::Result<(), &'static str>,
    ) -> Result<()> {
        match self.required_value(field)? {
            JsonValue::Array(value) => {
                let segment_count = validate_u64_array(
                    value,
                    Some((
                        "contains a segment that does not fit into u16",
                        u64::from(u16::MAX),
                    )),
                    visit,
                )
                .map_err(|message| {
                    self.error(format!("runtime trace field {field:?} {message}"))
                })?;
                if segment_count > max_segments {
                    return Err(self.error(format!(
                        "runtime trace field {field:?} contains {segment_count} segment(s) and exceeds maximum {max_segments}"
                    )));
                }
                Ok(())
            }
            _ => Err(self.error(format!("runtime trace field {field:?} must be an array"))),
        }
    }

    pub(super) fn value(&self, field: &str) -> Result<Option<JsonValue<'a>>> {
        self.raw_value(field)
            .map(|value| value.map(JsonValue::parse))
    }

    pub(super) fn error(&self, message: impl Into<String>) -> Error {
        Error::new(format!(
            "runtime trace line {}: {}",
            self.number,
            message.into()
        ))
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
                return Err(self.error("runtime trace object has an unexpected field separator"));
            }
            if bytes[index] != b'"' {
                return Err(self.error("runtime trace object field name must be a string"));
            }
            let key_start = index + 1;
            let key_end = scan_json_string_end(bytes, key_start)
                .ok_or_else(|| self.error("runtime trace object field name is malformed"))?;
            if bytes[key_start..key_end].contains(&b'\\') {
                return Err(self.error("runtime trace object field name escape is unsupported"));
            }
            index = skip_ascii_whitespace(bytes, key_end + 1);
            if bytes.get(index) != Some(&b':') {
                return Err(self.error("runtime trace object field separator is missing"));
            }
            index = skip_ascii_whitespace(bytes, index + 1);
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("runtime trace object field value is malformed"))?;
            index = skip_ascii_whitespace(bytes, index);
            match bytes.get(index) {
                Some(b',') => {
                    index += 1;
                    if skip_ascii_whitespace(bytes, index) == bytes.len() - 1 {
                        return Err(self.error("runtime trace object has a trailing separator"));
                    }
                }
                Some(b'}') if index == bytes.len() - 1 => return Ok(()),
                Some(_) => {
                    return Err(self.error("runtime trace object field separator is malformed"));
                }
                None => return Err(self.error("runtime trace object is unterminated")),
            }
        }
    }

    fn required_value(&self, field: &str) -> Result<JsonValue<'a>> {
        self.value(field)?
            .ok_or_else(|| self.error(format!("runtime trace event is missing field {field:?}")))
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
                return Err(self.error("runtime trace object field name must be a string"));
            }
            let key_start = index + 1;
            let key_end = scan_json_string_end(bytes, key_start)
                .ok_or_else(|| self.error("runtime trace object field name is unterminated"))?;
            index = skip_ascii_whitespace(bytes, key_end + 1);
            if bytes.get(index) != Some(&b':') {
                return Err(self.error("runtime trace object field separator is missing"));
            }
            index = skip_ascii_whitespace(bytes, index + 1);
            let value_start = index;
            index = skip_json_value(bytes, index)
                .ok_or_else(|| self.error("runtime trace object field value is malformed"))?;
            let key = &self.text[key_start..key_end];
            if key == field {
                return Ok(Some(&self.text[value_start..index]));
            }
        }
        Ok(None)
    }

    fn field_count(&self, field: &str) -> Result<usize> {
        let mut count = 0usize;
        self.for_each_field(|key| {
            if key == field {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| self.error("runtime trace field count overflowed"))?;
            }
            Ok(())
        })?;
        Ok(count)
    }
}

#[derive(Clone, Copy)]
pub(super) enum JsonValue<'a> {
    String(&'a str),
    Number(&'a str),
    Null,
    Array(&'a str),
    Other,
}

impl<'a> JsonValue<'a> {
    fn parse(raw: &'a str) -> Self {
        let raw = trim_json_ascii_whitespace(raw);
        if raw == "null" {
            Self::Null
        } else if let Some(value) = raw
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            Self::String(value)
        } else if raw.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            Self::Number(raw)
        } else if raw.starts_with('[') {
            Self::Array(raw)
        } else {
            Self::Other
        }
    }
}

fn validate_u64_array(
    value: &str,
    max_segment: Option<(&'static str, u64)>,
    mut visit: impl FnMut(u16) -> std::result::Result<(), &'static str>,
) -> std::result::Result<usize, &'static str> {
    let bytes = value.as_bytes();
    if !(bytes.first() == Some(&b'[') && bytes.last() == Some(&b']')) {
        return Err("must be an array of unsigned integers");
    }

    let mut index = skip_ascii_whitespace(bytes, 1);
    if index == bytes.len() - 1 {
        return Ok(0);
    }

    let mut segment_count = 0usize;
    loop {
        let value_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == value_start {
            return Err("must contain only unsigned integer segments");
        }
        let segment = parse_u64(
            std::str::from_utf8(&bytes[value_start..index])
                .map_err(|_| "contains non-UTF-8 numeric data")?,
        )?;
        if let Some((message, max_segment)) = max_segment {
            if segment > max_segment {
                return Err(message);
            }
        }
        visit(
            u16::try_from(segment).map_err(|_| "contains a segment that does not fit into u16")?,
        )?;
        segment_count = segment_count
            .checked_add(1)
            .ok_or("contains too many segments")?;
        index = skip_ascii_whitespace(bytes, index);
        match bytes.get(index) {
            Some(b',') => {
                index = skip_ascii_whitespace(bytes, index + 1);
                if index >= bytes.len() - 1 {
                    return Err("must not contain a trailing separator");
                }
            }
            Some(b']') if index == bytes.len() - 1 => return Ok(segment_count),
            Some(_) => return Err("must contain comma-separated unsigned integer segments"),
            None => return Err("is unterminated"),
        }
    }
}

fn parse_u64(value: &str) -> std::result::Result<u64, &'static str> {
    if !is_json_unsigned_integer(value) {
        return Err("must be a JSON unsigned integer");
    }
    value.parse().map_err(|_| "does not fit into u64")
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
            b'\\' => {
                index = validate_json_escape(bytes, index.checked_add(1)?)?;
            }
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
            while cursor < bytes.len() && !matches!(bytes[cursor], b',' | b'}') {
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
            b'[' | b'{' => {
                cursor = skip_json_container(bytes, cursor, depth.checked_add(1)?)?;
            }
            value if value == closer => return cursor.checked_add(1),
            value if value == wrong_closer => return None,
            _ => cursor += 1,
        }
    }
    None
}
