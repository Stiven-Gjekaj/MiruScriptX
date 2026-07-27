//! The MiruScriptX lexer: turns source text into a flat list of tokens.
//!
//! Newlines are significant: they separate statements, so the lexer emits a
//! `Newline` token at the end of a line. To let expressions span several lines
//! comfortably, newlines are suppressed while inside parentheses or brackets
//! (tracked with `group_depth`). A `;` is treated as an explicit statement
//! separator, so semicolons are always optional.

use std::collections::HashSet;

use crate::formatter::{Comment, Trivia};
use crate::token::{Token, TokenKind};
use crate::MiruError;

/// Where each token and comment sits in the source, in **char** offsets.
///
/// Char offsets rather than byte offsets, because that is the lexer's own model
/// (it scans a `Vec<char>`) and the one `MiruError::render` already uses when it
/// builds a caret indent. A consumer indexing UTF-16 units, as JavaScript does
/// by default, has to iterate code points instead or every span after a
/// multi-byte character will be wrong.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Spans {
    /// One `(start, len)` per token, in the same order as the returned tokens.
    pub tokens: Vec<(usize, usize)>,
    /// One `(start, len)` per comment, including the leading `//`. Comments are
    /// not tokens, but they still have to be coloured.
    pub comments: Vec<(usize, usize)>,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    line_start: usize,
    group_depth: usize,
    /// The `group_depth` each open brace suspended, restored by its match. A
    /// stack rather than a counter because braces and groups interleave.
    group_stack: Vec<usize>,
    /// When set, the lexer records comments and blank-line positions for the
    /// formatter instead of discarding them.
    collect_trivia: bool,
    comments: Vec<Comment>,
    blank_before: HashSet<usize>,
    /// When set, the lexer records where every token and comment sits, which a
    /// syntax highlighter needs and nothing else does.
    collect_spans: bool,
    spans: Spans,
    /// Where the token currently being scanned began, so its span can be closed
    /// once its end is known.
    token_start: usize,
    /// The last line that carried a token or comment, used to spot blank lines.
    last_content_line: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Lexer {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            line_start: 0,
            group_depth: 0,
            group_stack: Vec::new(),
            collect_trivia: false,
            comments: Vec::new(),
            blank_before: HashSet::new(),
            collect_spans: false,
            spans: Spans::default(),
            token_start: 0,
            last_content_line: 0,
        }
    }

    /// Tokenize an entire source string, ending with an `Eof` token.
    pub fn tokenize(source: &str) -> Result<Vec<Token>, MiruError> {
        Lexer::new(source).run()
    }

    /// Tokenize like [`tokenize`](Lexer::tokenize), and also gather the comments
    /// and blank-line positions that `miru fmt` needs to reprint a program.
    pub fn tokenize_with_trivia(source: &str) -> Result<(Vec<Token>, Trivia), MiruError> {
        let mut lexer = Lexer::new(source);
        lexer.collect_trivia = true;
        let tokens = lexer.run()?;
        let trivia = Trivia {
            comments: lexer.comments,
            blank_before: lexer.blank_before,
        };
        Ok((tokens, trivia))
    }

    /// Tokenize like [`tokenize`](Lexer::tokenize), and also record where every
    /// token and comment sits in the source.
    ///
    /// A span cannot be recovered from a token afterwards, because a token's
    /// value does not determine the text it came from: `"a\nb"` is seven source
    /// characters and a three-character string, and `1.50` and `1.5` lex to the
    /// same float. The lexer knows the answer while it is scanning and used to
    /// throw it away.
    pub fn tokenize_with_spans(source: &str) -> Result<(Vec<Token>, Spans), MiruError> {
        let mut lexer = Lexer::new(source);
        lexer.collect_spans = true;
        let tokens = lexer.run()?;
        Ok((tokens, lexer.spans))
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

            if !matches!(token.kind, TokenKind::Newline | TokenKind::Eof) {
                self.note_content_line(token.line);
            }

            if self.collect_spans {
                self.spans
                    .tokens
                    .push((self.token_start, self.pos - self.token_start));
            }
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    /// Record that content appeared on `line`, marking a blank line before it
    /// when there is a gap from the previous content line. Only active while
    /// collecting trivia for the formatter.
    fn note_content_line(&mut self, line: usize) {
        if !self.collect_trivia {
            return;
        }
        if self.last_content_line != 0 && line >= self.last_content_line + 2 {
            self.blank_before.insert(line);
        }
        if line > self.last_content_line {
            self.last_content_line = line;
        }
    }

    fn next_token(&mut self) -> Result<Token, MiruError> {
        loop {
            let c = match self.peek() {
                None => {
                    // Eof stands at the end and covers nothing. Without setting
                    // this it would inherit the previous token's start and
                    // report a span overlapping it.
                    self.token_start = self.pos;
                    return Ok(Token::new(TokenKind::Eof, self.line, self.column()));
                }
                Some(c) => c,
            };

            if c == '\n' {
                let line = self.line;
                let column = self.column();
                self.token_start = self.pos;
                self.advance();
                self.line += 1;
                self.line_start = self.pos;
                if self.group_depth > 0 {
                    continue; // insignificant inside ( ) or [ ]
                }
                return Ok(Token::new(TokenKind::Newline, line, column));
            }

            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
                continue;
            }

            if c == '/' && self.peek_at(1) == Some('/') {
                // A comment is a leading (own-line) comment when only whitespace
                // precedes it on the line; otherwise it trails code.
                let comment_line = self.line;
                let comment_start = self.pos;
                let own_line = self.chars[self.line_start..self.pos]
                    .iter()
                    .all(|ch| ch.is_whitespace());
                self.advance(); // first '/'
                self.advance(); // second '/'
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                }
                if self.collect_spans {
                    self.spans
                        .comments
                        .push((comment_start, self.pos - comment_start));
                }
                if self.collect_trivia {
                    let text: String = self.chars[start..self.pos].iter().collect();
                    self.note_content_line(comment_line);
                    self.comments.push(Comment {
                        line: comment_line,
                        own_line,
                        text,
                    });
                }
                continue;
            }

            self.token_start = self.pos;
            return self.read_token(c);
        }
    }

    fn read_token(&mut self, c: char) -> Result<Token, MiruError> {
        let line = self.line;
        let column = self.column();

        if c.is_ascii_digit() {
            return self.read_number(line, column);
        }
        if c == '_' || c.is_alphabetic() {
            return Ok(self.read_identifier(line, column));
        }
        if c == '"' {
            return self.read_string(line, column);
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
            // A brace restores newline significance, whatever is open outside
            // it. Without this a function body written inside a call argument
            // loses the separators its statements need, so a multi-line
            // callback handed straight to `map` did not parse.
            //
            // Map literals are unaffected: braces never suppressed newlines, so
            // `Parser::parse_map` already skips them between entries itself.
            '{' => {
                self.group_stack.push(self.group_depth);
                self.group_depth = 0;
                TokenKind::LBrace
            }
            '}' => {
                self.group_depth = self.group_stack.pop().unwrap_or(0);
                TokenKind::RBrace
            }
            '[' => {
                self.group_depth += 1;
                TokenKind::LBracket
            }
            ']' => {
                self.group_depth = self.group_depth.saturating_sub(1);
                TokenKind::RBracket
            }
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            // A '.' only reaches here when it is not part of a number:
            // `read_number` takes one that is followed by a digit, so `1.5` is
            // a single float while `1.foo` is an int, a dot, and a name.
            '.' => TokenKind::Dot,
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
                    return Err(MiruError::with_column(
                        line,
                        column,
                        "unexpected '&' (did you mean '&&'?)",
                    ));
                }
            }
            '|' => {
                if self.match_char('|') {
                    TokenKind::Or
                } else {
                    return Err(MiruError::with_column(
                        line,
                        column,
                        "unexpected '|' (did you mean '||'?)",
                    ));
                }
            }
            other => {
                return Err(MiruError::with_column(
                    line,
                    column,
                    format!("unexpected character '{other}'"),
                ));
            }
        };
        Ok(Token::new(kind, line, column))
    }

    fn read_number(&mut self, line: usize, column: usize) -> Result<Token, MiruError> {
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
                Ok(value) => Ok(Token::new(TokenKind::Float(value), line, column)),
                Err(_) => Err(MiruError::with_column(
                    line,
                    column,
                    format!("invalid number '{text}'"),
                )),
            }
        } else {
            match text.parse::<i64>() {
                Ok(value) => Ok(Token::new(TokenKind::Int(value), line, column)),
                Err(_) => Err(MiruError::with_column(
                    line,
                    column,
                    format!("integer literal '{text}' is out of range"),
                )),
            }
        }
    }

    fn read_identifier(&mut self, line: usize, column: usize) -> Token {
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
        Token::new(kind, line, column)
    }

    fn read_string(&mut self, line: usize, column: usize) -> Result<Token, MiruError> {
        self.advance(); // consume opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(MiruError::with_column(
                        line,
                        column,
                        "unterminated string literal",
                    ))
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\n') => {
                    return Err(MiruError::with_column(
                        line,
                        column,
                        "unterminated string literal",
                    ));
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
                            return Err(MiruError::with_column(
                                line,
                                column,
                                format!("unknown escape sequence '\\{other}'"),
                            ));
                        }
                        None => {
                            return Err(MiruError::with_column(
                                line,
                                column,
                                "unterminated string literal",
                            ));
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
        Ok(Token::new(TokenKind::Str(value), line, column))
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

    /// The 1-based column of the current position within the current line.
    fn column(&self) -> usize {
        self.pos - self.line_start + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slice a source string by a char span, the way a consumer must.
    fn slice(source: &str, (start, len): (usize, usize)) -> String {
        source.chars().skip(start).take(len).collect()
    }

    #[test]
    fn spans_cover_the_text_a_token_came_from() {
        // Each of these is a case where the token's value does not determine its
        // source text, which is the whole reason spans have to be recorded
        // rather than reconstructed.
        let source = "let s = \"a\\nb\"\nlet f = 1.50\n";
        let (tokens, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        assert_eq!(tokens.len(), spans.tokens.len());

        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        // The string keeps its quotes and its two-character escape, seven source
        // characters for a value of three.
        assert!(text.contains(&"\"a\\nb\"".to_string()), "{text:?}");
        // The float keeps the trailing zero that its value does not remember.
        assert!(text.contains(&"1.50".to_string()), "{text:?}");
    }

    #[test]
    fn spans_stay_aligned_after_a_multi_byte_character() {
        // Offsets are chars, not bytes. A three-byte character before a token
        // must move its span by one, not by three.
        let source = "// \u{4e2d}\nlet x = 1";
        let (_, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        assert!(text.contains(&"let".to_string()), "{text:?}");
        assert!(text.contains(&"x".to_string()), "{text:?}");
        assert_eq!(spans.comments.len(), 1);
        assert_eq!(slice(source, spans.comments[0]), "// \u{4e2d}");
    }

    #[test]
    fn every_span_slices_back_to_something_and_none_overlap() {
        let source = "fn f(a) {\n  // add one\n  return a + 1\n}\nf(2)\n";
        let (tokens, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        assert_eq!(tokens.len(), spans.tokens.len());

        // Every span lies inside the source, and the ones with width run in
        // order without overlapping. Newline tokens have width, Eof has none.
        let total = source.chars().count();
        let mut previous_end = 0;
        for span in &spans.tokens {
            assert!(span.0 + span.1 <= total, "span {span:?} past the end");
            assert!(
                span.0 >= previous_end,
                "span {span:?} overlaps the one before"
            );
            previous_end = span.0 + span.1;
        }
        assert_eq!(slice(source, spans.comments[0]), "// add one");
    }

    #[test]
    fn collecting_spans_does_not_change_the_tokens() {
        // The ordinary path must be unaffected, which is the point of making
        // this opt in.
        for source in ["", "let x = 1", "fn f() {\n  // c\n  return 1\n}\n"] {
            let plain = Lexer::tokenize(source).expect("lexes");
            let (with_spans, _) = Lexer::tokenize_with_spans(source).expect("lexes");
            assert_eq!(plain, with_spans, "differed for {source:?}");
        }
    }
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
    fn a_dot_is_a_token_unless_a_number_is_taking_it() {
        // `read_number` consumes a '.' only when a digit follows it, so the two
        // uses do not collide. These pin that boundary from both sides, since
        // it is the one place adding this token could have changed an existing
        // program's meaning.
        assert_eq!(kinds("2.5"), vec![TokenKind::Float(2.5), TokenKind::Eof]);
        assert_eq!(
            kinds("a.b"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Dot,
                TokenKind::Ident("b".to_string()),
                TokenKind::Eof,
            ]
        );
        // A dot after a number, not followed by a digit, is a dot. Whether the
        // parser accepts this is its business; the lexer just does not merge it
        // into the number.
        assert_eq!(
            kinds("1.foo"),
            vec![
                TokenKind::Int(1),
                TokenKind::Dot,
                TokenKind::Ident("foo".to_string()),
                TokenKind::Eof,
            ]
        );
        // And a trailing dot with nothing after it at all.
        assert_eq!(
            kinds("1."),
            vec![TokenKind::Int(1), TokenKind::Dot, TokenKind::Eof]
        );
        // A digit after the dot belongs to whatever follows, not to the number
        // before it, because `read_number` already finished.
        assert_eq!(
            kinds("a.1"),
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Dot,
                TokenKind::Int(1),
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
    fn tokenizes_colon() {
        assert_eq!(kinds(":"), vec![TokenKind::Colon, TokenKind::Eof]);
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

    #[test]
    fn tracks_columns_within_a_line() {
        let tokens = Lexer::tokenize("let x = 42").expect("tokenizes");
        assert_eq!(tokens[0].column, 1); // let
        assert_eq!(tokens[1].column, 5); // x
        assert_eq!(tokens[2].column, 7); // =
        assert_eq!(tokens[3].column, 9); // 42
    }

    #[test]
    fn columns_reset_on_each_line() {
        let tokens = Lexer::tokenize("ab\n  cd").expect("tokenizes");
        let cd = tokens
            .iter()
            .find(|token| token.kind == TokenKind::Ident("cd".to_string()))
            .expect("has cd");
        assert_eq!(cd.line, 2);
        assert_eq!(cd.column, 3);
    }

    #[test]
    fn lexer_error_carries_a_column() {
        let err = Lexer::tokenize("  @").unwrap_err();
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 3);
    }

    #[test]
    fn trivia_collects_comments_and_blank_lines() {
        let (_tokens, trivia) =
            Lexer::tokenize_with_trivia("// header\n\nlet x = 1 // trailing").expect("lexes");
        assert_eq!(trivia.comments.len(), 2);
        assert!(trivia.comments[0].own_line);
        assert_eq!(trivia.comments[0].text.trim(), "header");
        assert!(!trivia.comments[1].own_line);
        assert_eq!(trivia.comments[1].text.trim(), "trailing");
        // "let x = 1" sits on line 3, with a blank line 2 above it.
        assert!(trivia.blank_before.contains(&3));
    }

    #[test]
    fn trivia_ignores_gaps_filled_by_comments() {
        // Consecutive lines, one of them a comment: no blank line anywhere.
        let (_tokens, trivia) =
            Lexer::tokenize_with_trivia("let a = 1\n// note\nlet b = 2").expect("lexes");
        assert!(trivia.blank_before.is_empty());
    }
}
