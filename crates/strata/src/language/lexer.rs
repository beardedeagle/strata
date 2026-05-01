use mantle_artifact::{Error, Result};

use super::MAX_TOKEN_COUNT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    Ident(String),
    Number(String),
    StringLiteral(String),
    Symbol(char),
    Arrow,
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
        let mut tokens = Vec::new();
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
                push_token(&mut tokens, TokenKind::Arrow, offset)?;
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
                push_token(&mut tokens, TokenKind::AtIdent(ident), offset)?;
                continue;
            }
            if ch == '"' {
                let literal = self.read_string_literal(offset)?;
                push_token(&mut tokens, TokenKind::StringLiteral(literal), offset)?;
                continue;
            }
            if is_ident_start(ch) {
                let ident = self.read_ident()?;
                push_token(&mut tokens, TokenKind::Ident(ident), offset)?;
                continue;
            }
            if ch.is_ascii_digit() {
                let number = self.read_number();
                push_token(&mut tokens, TokenKind::Number(number), offset)?;
                continue;
            }
            if "{}()[];:,=<>!~".contains(ch) {
                self.bump_char();
                push_token(&mut tokens, TokenKind::Symbol(ch), offset)?;
                continue;
            }
            return Err(Error::new(format!(
                "unsupported character {ch:?} at byte {offset}"
            )));
        }
        push_token(&mut tokens, TokenKind::Eof, self.source.len())?;
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
        let mut ident = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if is_ident_continue(ch) {
                ident.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        if ident.is_empty() {
            Err(Error::new(format!(
                "expected identifier at byte {}",
                self.offset
            )))
        } else {
            Ok(ident)
        }
    }

    fn read_number(&mut self) -> String {
        let mut number = String::new();
        while let Some((_, ch)) = self.peek_char() {
            if ch.is_ascii_digit() {
                number.push(ch);
                self.bump_char();
            } else {
                break;
            }
        }
        number
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
                    return Err(Error::new(format!(
                        "string escapes are not supported in this source slice at byte {offset}"
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
}

fn push_token(tokens: &mut Vec<Token>, kind: TokenKind, offset: usize) -> Result<()> {
    if tokens.len() >= MAX_TOKEN_COUNT {
        return Err(Error::new(format!(
            "source exceeds maximum token count of {MAX_TOKEN_COUNT}"
        )));
    }
    tokens.push(Token { kind, offset });
    Ok(())
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
