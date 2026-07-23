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
                    return Err(MiruError::new(line, "unexpected '&' (did you mean '&&'?)"));
                }
            }
            '|' => {
                if self.match_char('|') {
                    TokenKind::Or
                } else {
                    return Err(MiruError::new(line, "unexpected '|' (did you mean '||'?)"));
                }
            }
            other => {
                return Err(MiruError::new(
                    line,
                    format!("unexpected character '{other}'"),
                ));
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
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    fn kinds(source: &str) -> Vec<TokenKind> {
        Lexer::tokenize(source)
            .expect("source should tokenize")
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn tokenizes_integers_and_floats() {
        assert_eq!(
            kinds("1 42 2.5"),
            vec![
                TokenKind::Int(1),
                TokenKind::Int(42),
                TokenKind::Float(2.5),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_keywords_and_identifiers() {
        assert_eq!(
            kinds("fn add total"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("add".to_string()),
                TokenKind::Ident("total".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_break_and_continue() {
        assert_eq!(
            kinds("break continue"),
            vec![TokenKind::Break, TokenKind::Continue, TokenKind::Eof]
        );
    }

    #[test]
    fn tokenizes_two_character_operators() {
        assert_eq!(
            kinds("== != <= >= && ||"),
            vec![
                TokenKind::Eq,
                TokenKind::NotEq,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn single_equals_is_assignment() {
        assert_eq!(
            kinds("x = 1"),
            vec![
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn parses_string_escapes() {
        assert_eq!(
            kinds("\"a\\nb\\t\\\"c\""),
            vec![TokenKind::Str("a\nb\t\"c".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn skips_line_comments() {
        assert_eq!(
            kinds("1 // this is ignored\n2"),
            vec![
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Int(2),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn semicolons_act_as_newlines() {
        assert_eq!(
            kinds("1;2"),
            vec![
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Int(2),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn newlines_are_suppressed_inside_brackets() {
        assert_eq!(
            kinds("[\n  1,\n  2,\n]"),
            vec![
                TokenKind::LBracket,
                TokenKind::Int(1),
                TokenKind::Comma,
                TokenKind::Int(2),
                TokenKind::Comma,
                TokenKind::RBracket,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tracks_line_numbers_across_blank_lines() {
        let tokens = Lexer::tokenize("1\n\n2").expect("tokenizes");
        assert_eq!(tokens[0].kind, TokenKind::Int(1));
        assert_eq!(tokens[0].line, 1);
        let two = tokens
            .iter()
            .find(|token| token.kind == TokenKind::Int(2))
            .expect("has a second integer");
        assert_eq!(two.line, 3);
    }

    #[test]
    fn reports_unterminated_string() {
        let err = Lexer::tokenize("\"oops").unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn reports_unexpected_character_with_line() {
        let err = Lexer::tokenize("1\n@").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("unexpected character"));
    }
}
