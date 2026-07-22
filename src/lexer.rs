//! The MiruScriptX lexer: turns source text into a flat list of tokens.
//!
//! Newlines are significant: they separate statements, so the lexer emits a
//! `Newline` token at the end of a line. To let expressions span several lines
//! comfortably, newlines are suppressed while inside parentheses or brackets
//! (tracked with `group_depth`). A `;` is treated as an explicit statement
//! separator, so semicolons are always optional.

use crate::token::{Token, TokenKind};
use crate::MiruError;

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    group_depth: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Lexer {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            group_depth: 0,
        }
    }

    /// Tokenize an entire source string, ending with an `Eof` token.
    pub fn tokenize(source: &str) -> Result<Vec<Token>, MiruError> {
        Lexer::new(source).run()
    }

    fn run(&mut self) -> Result<Vec<Token>, MiruError> {
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;

            // Collapse runs of blank lines and drop any leading newline so the
            // parser never has to wade through empty statements.
            if token.kind == TokenKind::Newline {
                match tokens.last() {
                    None => continue,
                    Some(last) if last.kind == TokenKind::Newline => continue,
                    _ => {}
                }
            }

            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, MiruError> {
        loop {
            let c = match self.peek() {
                None => return Ok(Token::new(TokenKind::Eof, self.line)),
                Some(c) => c,
            };

            if c == '\n' {
                let line = self.line;
                self.advance();
                self.line += 1;
                if self.group_depth > 0 {
                    continue; // insignificant inside ( ) or [ ]
                }
                return Ok(Token::new(TokenKind::Newline, line));
            }

            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
                continue;
            }

            if c == '/' && self.peek_at(1) == Some('/') {
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                }
                continue;
            }

            return self.read_token(c);
        }
    }

    fn read_token(&mut self, c: char) -> Result<Token, MiruError> {
        let line = self.line;

        if c.is_ascii_digit() {
            return self.read_number(line);
        }
        if c == '_' || c.is_alphabetic() {
            return Ok(self.read_identifier(line));
        }
        if c == '"' {
            return self.read_string(line);
        }

        self.advance();
        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '(' => {
                self.group_depth += 1;
                TokenKind::LParen
            }
            ')' => {
                self.group_depth = self.group_depth.saturating_sub(1);
                TokenKind::RParen
            }
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => {
                self.group_depth += 1;
                TokenKind::LBracket
            }
            ']' => {
                self.group_depth = self.group_depth.saturating_sub(1);
                TokenKind::RBracket
            }
            ',' => TokenKind::Comma,
            ';' => TokenKind::Newline,
            '=' => {
                if self.match_char('=') {
                    TokenKind::Eq
                } else {
                    TokenKind::Assign
                }
            }
            '!' => {
                if self.match_char('=') {
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.match_char('=') {
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.match_char('=') {
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                if self.match_char('&') {
                    TokenKind::And
                } else {
                    return Err(MiruError::new(
                        line,
                        "unexpected '&' (did you mean '&&'?)",
                    ));
                }
            }
            '|' => {
                if self.match_char('|') {
                    TokenKind::Or
                } else {
                    return Err(MiruError::new(
                        line,
                        "unexpected '|' (did you mean '||'?)",
                    ));
                }
            }
            other => {
                return Err(MiruError::new(line, format!("unexpected character '{other}'")));
            }
        };
        Ok(Token::new(kind, line))
    }

    fn read_number(&mut self, line: usize) -> Result<Token, MiruError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        let mut is_float = false;
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.advance(); // consume '.'
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let text: String = self.chars[start..self.pos].iter().collect();
        if is_float {
            match text.parse::<f64>() {
                Ok(value) => Ok(Token::new(TokenKind::Float(value), line)),
                Err(_) => Err(MiruError::new(line, format!("invalid number '{text}'"))),
            }
        } else {
            match text.parse::<i64>() {
                Ok(value) => Ok(Token::new(TokenKind::Int(value), line)),
                Err(_) => Err(MiruError::new(
                    line,
                    format!("integer literal '{text}' is out of range"),
                )),
            }
        }
    }

    fn read_identifier(&mut self, line: usize) -> Token {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '_' || c.is_alphanumeric() {
                self.advance();
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let kind = match text.as_str() {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            _ => TokenKind::Ident(text),
        };
        Token::new(kind, line)
    }

    fn read_string(&mut self, line: usize) -> Result<Token, MiruError> {
        self.advance(); // consume opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None => return Err(MiruError::new(line, "unterminated string literal")),
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\n') => {
                    return Err(MiruError::new(line, "unterminated string literal"));
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('r') => value.push('\r'),
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some('0') => value.push('\0'),
                        Some(other) => {
                            return Err(MiruError::new(
                                line,
                                format!("unknown escape sequence '\\{other}'"),
                            ));
                        }
                        None => {
                            return Err(MiruError::new(line, "unterminated string literal"));
                        }
                    }
                    self.advance();
                }
                Some(c) => {
                    value.push(c);
                    self.advance();
                }
            }
        }
        Ok(Token::new(TokenKind::Str(value), line))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}
