use super::MAX_TOKEN_COUNT;
use super::diagnostic::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    Ident(String),
    Number(String),
    StringLiteral(String),
    BytesLiteral(Vec<u8>),
    Symbol(char),
    EqualEqual,
    BangEqual,
    LessEqual,
    GreaterEqual,
    AmpAmp,
    PipePipe,
    DotDot,
    Arrow,
    FatArrow,
    AtIdent(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) offset: usize,
}

pub(super) struct Lexer<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    pub(super) fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::with_capacity(initial_token_capacity(self.source));
        while let Some((offset, ch)) = self.peek_char() {
            if ch.is_whitespace() {
                self.bump_char();
                continue;
            }
            if ch == '/' && self.peek_next_char() == Some('/') {
                self.bump_char();
                self.bump_char();
                while let Some((_, next)) = self.peek_char() {
                    self.bump_char();
                    if next == '\n' {
                        break;
                    }
                }
                continue;
            }
            if ch == '-' && self.peek_next_char() == Some('>') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::Arrow, offset)?;
                continue;
            }
            if ch == '=' && self.peek_next_char() == Some('>') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::FatArrow, offset)?;
                continue;
            }
            if ch == '=' && self.peek_next_char() == Some('=') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::EqualEqual, offset)?;
                continue;
            }
            if ch == '!' && self.peek_next_char() == Some('=') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::BangEqual, offset)?;
                continue;
            }
            if ch == '<' && self.peek_next_char() == Some('=') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::LessEqual, offset)?;
                continue;
            }
            if ch == '>' && self.peek_next_char() == Some('=') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::GreaterEqual, offset)?;
                continue;
            }
            if ch == '&' && self.peek_next_char() == Some('&') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::AmpAmp, offset)?;
                continue;
            }
            if ch == '|' && self.peek_next_char() == Some('|') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::PipePipe, offset)?;
                continue;
            }
            if ch == '.' && self.peek_next_char() == Some('.') {
                self.bump_char();
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::DotDot, offset)?;
                continue;
            }
            if ch == '@' {
                self.bump_char();
                match self.peek_char() {
                    Some((_, next)) if is_ident_start(next) => {}
                    _ => {
                        return Err(Error::new(format!(
                            "expected identifier after '@' at byte {offset}"
                        )));
                    }
                }
                let ident = self.read_ident()?;
                push_source_token(&mut tokens, TokenKind::AtIdent(ident), offset)?;
                continue;
            }
            if ch == '"' {
                let literal = self.read_string_literal(offset)?;
                push_source_token(&mut tokens, TokenKind::StringLiteral(literal), offset)?;
                continue;
            }
            if ch == 'b' && self.peek_next_char() == Some('"') {
                self.bump_char();
                let literal = self.read_bytes_literal(offset)?;
                push_source_token(&mut tokens, TokenKind::BytesLiteral(literal), offset)?;
                continue;
            }
            if is_ident_start(ch) {
                let ident = self.read_ident()?;
                push_source_token(&mut tokens, TokenKind::Ident(ident), offset)?;
                continue;
            }
            if ch.is_ascii_digit() {
                let number = self.read_number();
                push_source_token(&mut tokens, TokenKind::Number(number), offset)?;
                continue;
            }
            if "{}()[];:,=<>!~+-*/%".contains(ch) {
                self.bump_char();
                push_source_token(&mut tokens, TokenKind::Symbol(ch), offset)?;
                continue;
            }
            return Err(Error::new(format!(
                "unsupported character {ch:?} at byte {offset}"
            )));
        }
        push_eof_token(&mut tokens, self.source.len());
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<(usize, char)> {
        self.source[self.offset..]
            .char_indices()
            .next()
            .map(|(local, ch)| (self.offset + local, ch))
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.offset..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.source[self.offset..].chars().next()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn read_ident(&mut self) -> Result<String> {
        let start = self.offset;
        while let Some((_, ch)) = self.peek_char() {
            if is_ident_continue(ch) {
                self.bump_char();
            } else {
                break;
            }
        }
        if self.offset == start {
            Err(Error::new(format!(
                "expected identifier at byte {}",
                self.offset
            )))
        } else {
            Ok(self.source[start..self.offset].to_string())
        }
    }

    fn read_number(&mut self) -> String {
        let start = self.offset;
        while let Some((_, ch)) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.bump_char();
            } else {
                break;
            }
        }
        self.source[start..self.offset].to_string()
    }

    fn read_string_literal(&mut self, start: usize) -> Result<String> {
        self.bump_char();
        let mut literal = String::new();
        while let Some((offset, ch)) = self.peek_char() {
            match ch {
                '"' => {
                    self.bump_char();
                    return Ok(literal);
                }
                '\n' | '\r' => {
                    return Err(Error::new(format!(
                        "unterminated string literal at byte {start}"
                    )));
                }
                '\\' => {
                    self.bump_char();
                    literal.push(self.read_string_escape(start, offset)?);
                }
                ch if ch.is_control() => {
                    return Err(Error::new(format!(
                        "string literal contains unescaped control character at byte {offset}"
                    )));
                }
                _ => {
                    literal.push(ch);
                    self.bump_char();
                }
            }
        }
        Err(Error::new(format!(
            "unterminated string literal at byte {start}"
        )))
    }

    fn read_string_escape(&mut self, start: usize, offset: usize) -> Result<char> {
        let Some((_, escape)) = self.peek_char() else {
            return Err(Error::new(format!(
                "unterminated string literal at byte {start}"
            )));
        };
        self.bump_char();
        match escape {
            '"' => Ok('"'),
            '\\' => Ok('\\'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            'u' => self.read_unicode_escape(start, offset),
            _ => Err(Error::new(format!(
                "unsupported string escape \\{escape} at byte {offset}"
            ))),
        }
    }

    fn read_unicode_escape(&mut self, start: usize, offset: usize) -> Result<char> {
        if self.bump_char() != Some('{') {
            return Err(Error::new(format!(
                "unicode string escape at byte {offset} must use \\u{{HEX}}"
            )));
        }
        let mut value = 0u32;
        let mut len = 0usize;
        while let Some((ch_offset, ch)) = self.peek_char() {
            if ch == '}' {
                self.bump_char();
                if len == 0 {
                    return Err(Error::new(format!(
                        "unicode string escape at byte {offset} must contain hex digits"
                    )));
                }
                return char::from_u32(value).ok_or_else(|| {
                    Error::new(format!(
                        "unicode string escape at byte {offset} is not a valid scalar value"
                    ))
                });
            }
            let Some(nibble) = ch.to_digit(16) else {
                return Err(Error::new(format!(
                    "unicode string escape at byte {ch_offset} contains non-hex digit"
                )));
            };
            if len >= 6 {
                return Err(Error::new(format!(
                    "unicode string escape at byte {offset} exceeds six hex digits"
                )));
            }
            value = (value << 4) | nibble;
            len += 1;
            self.bump_char();
        }
        Err(Error::new(format!(
            "unterminated string literal at byte {start}"
        )))
    }

    fn read_bytes_literal(&mut self, start: usize) -> Result<Vec<u8>> {
        self.bump_char();
        let mut literal = Vec::new();
        while let Some((offset, ch)) = self.peek_char() {
            match ch {
                '"' => {
                    self.bump_char();
                    return Ok(literal);
                }
                '\n' | '\r' => {
                    return Err(Error::new(format!(
                        "unterminated bytes literal at byte {start}"
                    )));
                }
                '\\' => {
                    self.bump_char();
                    literal.push(self.read_bytes_escape(start, offset)?);
                }
                ch if ch.is_ascii() && !ch.is_control() => {
                    literal.push(ch as u8);
                    self.bump_char();
                }
                _ => {
                    return Err(Error::new(format!(
                        "bytes literal raw data must be printable ASCII or escaped at byte {offset}"
                    )));
                }
            }
        }
        Err(Error::new(format!(
            "unterminated bytes literal at byte {start}"
        )))
    }

    fn read_bytes_escape(&mut self, start: usize, offset: usize) -> Result<u8> {
        let Some((_, escape)) = self.peek_char() else {
            return Err(Error::new(format!(
                "unterminated bytes literal at byte {start}"
            )));
        };
        self.bump_char();
        match escape {
            '"' => Ok(b'"'),
            '\\' => Ok(b'\\'),
            'n' => Ok(b'\n'),
            'r' => Ok(b'\r'),
            't' => Ok(b'\t'),
            'x' => self.read_hex_escape_byte(start, offset),
            _ => Err(Error::new(format!(
                "unsupported bytes escape \\{escape} at byte {offset}"
            ))),
        }
    }

    fn read_hex_escape_byte(&mut self, start: usize, offset: usize) -> Result<u8> {
        let Some((_, high)) = self.peek_char() else {
            return Err(Error::new(format!(
                "unterminated bytes literal at byte {start}"
            )));
        };
        self.bump_char();
        let Some((_, low)) = self.peek_char() else {
            return Err(Error::new(format!(
                "unterminated bytes literal at byte {start}"
            )));
        };
        self.bump_char();
        let high = hex_nibble(high).ok_or_else(|| {
            Error::new(format!(
                "bytes escape at byte {offset} must use two hex digits"
            ))
        })?;
        let low = hex_nibble(low).ok_or_else(|| {
            Error::new(format!(
                "bytes escape at byte {offset} must use two hex digits"
            ))
        })?;
        Ok((high << 4) | low)
    }
}

fn hex_nibble(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(ch as u8 - b'0'),
        'a'..='f' => Some(ch as u8 - b'a' + 10),
        'A'..='F' => Some(ch as u8 - b'A' + 10),
        _ => None,
    }
}

fn push_source_token(tokens: &mut Vec<Token>, kind: TokenKind, offset: usize) -> Result<()> {
    if tokens.len() >= MAX_TOKEN_COUNT {
        return Err(Error::new(format!(
            "source exceeds maximum token count of {MAX_TOKEN_COUNT}"
        )));
    }
    tokens.push(Token { kind, offset });
    Ok(())
}

fn push_eof_token(tokens: &mut Vec<Token>, offset: usize) {
    tokens.push(Token {
        kind: TokenKind::Eof,
        offset,
    });
}

fn initial_token_capacity(source: &str) -> usize {
    source.len().div_ceil(3).clamp(1, 512)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
