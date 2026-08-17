//! The MiruScriptX lexer: turns source text into a flat list of tokens.
//!
//! Newlines are significant: they separate statements, so the lexer emits a
//! `Newline` token at the end of a line. To let expressions span several lines
//! comfortably, newlines are suppressed while inside parentheses or brackets
//! (tracked with `group_depth`). A `;` is treated as an explicit statement
//! separator, so semicolons are always optional.

use std::collections::HashSet;

use crate::formatter::{Comment, Trivia};
use crate::token::{reserved_as_a_name, reserved_word, FStringPart, Token, TokenKind};
use crate::MiruError;

/// The largest number of hexadecimal digits a `\u{...}` escape takes.
///
/// Six is enough for every character, because the largest one is `10FFFF`. It
/// is also what lets `read_unicode_escape` accumulate with ordinary
/// arithmetic: six hexadecimal digits cannot overflow a `u32`.
const MAX_UNICODE_ESCAPE_DIGITS: usize = 6;

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
    /// When set, the words version 2 reserves lex as identifiers, the way
    /// version 1 read them. Set only by
    /// [`tokenize_1x_with_spans`](Lexer::tokenize_1x_with_spans).
    reserved_words_are_names: bool,
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
            reserved_words_are_names: false,
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

    /// Tokenize like [`tokenize_with_spans`](Lexer::tokenize_with_spans), but
    /// read the words in [`RESERVED_WORDS`] as ordinary names, the way version
    /// 1 did.
    ///
    /// **This exists so the off-ramp still works from inside version 2.** The
    /// program that needs migrating is exactly the program this lexer refuses,
    /// so a `miru migrate` built on the 2.0 lexer could not read a single file
    /// it was written for. Without this, upgrading first and migrating second,
    /// which is the order most people will do it in, would mean reinstalling
    /// 1.12 to get out.
    ///
    /// Nothing else may use it. It is not a compatibility mode for running old
    /// programs: it lexes them, and 2.0 still means what 2.0 means.
    pub fn tokenize_1x_with_spans(source: &str) -> Result<(Vec<Token>, Spans), MiruError> {
        let mut lexer = Lexer::new(source);
        lexer.collect_spans = true;
        lexer.reserved_words_are_names = true;
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
        // `f"` with nothing between is an f-string. The quote has to follow
        // immediately, so a variable named `f` and a function called `f(x)`
        // are untouched, and `f "x"` with a space stays the error it was.
        if c == 'f' && self.peek_at(1) == Some('"') {
            return self.read_fstring(line, column);
        }
        if c == '_' || c.is_alphabetic() {
            return Ok(self.read_identifier(line, column));
        }
        if c == '"' {
            return self.read_string(line, column);
        }

        self.advance();
        let kind = match c {
            // Each of the five may be followed by '=', which makes it a
            // compound assignment. `+=` was an error before 1.9, so reading it
            // as a token changes no program that ever ran.
            '+' => self.assigning(TokenKind::PlusAssign, TokenKind::Plus),
            '-' => self.assigning(TokenKind::MinusAssign, TokenKind::Minus),
            '*' => self.assigning(TokenKind::StarAssign, TokenKind::Star),
            '/' => self.assigning(TokenKind::SlashAssign, TokenKind::Slash),
            '%' => self.assigning(TokenKind::PercentAssign, TokenKind::Percent),
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
            // `...` before `.`, so the longer token wins. Two dots are still
            // two dots: nothing spells `..`, and a wrong number of them reads
            // better as a repeated `.` than as a token nobody wrote.
            '.' if self.peek() == Some('.') && self.peek_at(1) == Some('.') => {
                self.advance();
                self.advance();
                TokenKind::Ellipsis
            }
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

    /// Read a run of decimal digits, permitting `_` between two of them.
    ///
    /// The digit *before* a separator needs no check here. Both callers start
    /// on a digit: `read_number` is only entered when the current character is
    /// one, and the fractional part only begins after a `.` that was consumed
    /// because a digit followed it. So a `_` can never be the first character
    /// this sees, and the rule reduces to the half below.
    ///
    /// The line and the column are those of the start of the number, which is
    /// where the other errors in `read_number` are reported.
    fn read_digits(&mut self, line: usize, column: usize) -> Result<(), MiruError> {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '_' {
                if !self.peek_at(1).is_some_and(|next| next.is_ascii_digit()) {
                    return Err(MiruError::with_column(
                        line,
                        column,
                        "a digit separator must be between two digits",
                    ));
                }
                self.advance(); // the separator
                self.advance(); // the digit that made it one
            } else {
                break;
            }
        }
        Ok(())
    }

    fn read_number(&mut self, line: usize, column: usize) -> Result<Token, MiruError> {
        let start = self.pos;
        self.read_digits(line, column)?;

        let mut is_float = false;
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.advance(); // consume '.'
            self.read_digits(line, column)?;
        }

        // The text is kept as written, because the two errors below quote it
        // and a reader needs to see what they typed. The value is parsed from
        // the digits alone: a separator is a mark for a reader and is not part
        // of the number.
        let text: String = self.chars[start..self.pos].iter().collect();
        let digits: String = text.chars().filter(|c| *c != '_').collect();
        if is_float {
            match digits.parse::<f64>() {
                Ok(value) => Ok(Token::new(TokenKind::Float(value), line, column)),
                Err(_) => Err(MiruError::with_column(
                    line,
                    column,
                    format!("invalid number '{text}'"),
                )),
            }
        } else {
            match digits.parse::<i64>() {
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
        // Version 1 read every word in `RESERVED_WORDS` as a name, and this
        // mode reads them the same way so `miru migrate` can still open the
        // files it exists to fix. It comes before the table rather than after
        // it, because `match` is in the table with a token of its own.
        if self.reserved_words_are_names && reserved_word(&text).is_some() {
            return Token::new(TokenKind::Ident(text), line, column);
        }
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
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "try" => TokenKind::Try,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            "match" => TokenKind::Match,
            // The fifteen version 2 reserved without giving a grammar. They are
            // refused as names rather than given a meaning, so that the release
            // which gives one of them a meaning costs nothing.
            other => match reserved_word(other) {
                Some(word) => TokenKind::Reserved(word),
                None => TokenKind::Ident(text),
            },
        };
        Token::new(kind, line, column)
    }

    /// Read an `f"..."` literal into alternating text and names.
    ///
    /// **Why a prefix rather than interpolating every string.**
    /// `print("n is ${n}")` is a working program that prints the braces, so
    /// interpolating a plain literal would change what an existing program
    /// means, which section 5 of the guarantee puts in version 2. `f"` is a
    /// parse error today, so this costs no program anything.
    ///
    /// Each name carries its own line and column. That is the whole reason the
    /// parts are collected here rather than the string being rebuilt and
    /// re-lexed: an unknown name inside the literal has to point a caret at
    /// itself, and by the time a string is one token that position is gone.
    fn read_fstring(&mut self, line: usize, column: usize) -> Result<Token, MiruError> {
        self.advance(); // consume 'f'
        self.advance(); // consume opening quote
        let mut parts: Vec<FStringPart> = Vec::new();
        let mut text = String::new();
        let unterminated = || MiruError::with_column(line, column, "unterminated string literal");
        loop {
            match self.peek() {
                None | Some('\n') => return Err(unterminated()),
                Some('"') => {
                    self.advance();
                    break;
                }
                // `{{` and `}}` are the literal braces. Without them there is
                // no way to write one, and a program that wants a brace in an
                // interpolated string is not unusual.
                Some('{') if self.peek_at(1) == Some('{') => {
                    text.push('{');
                    self.advance();
                    self.advance();
                }
                Some('}') if self.peek_at(1) == Some('}') => {
                    text.push('}');
                    self.advance();
                    self.advance();
                }
                Some('}') => {
                    return Err(MiruError::with_column(
                        self.line,
                        self.column(),
                        "a '}' in an f-string needs a '{' before it, or '}}' for a literal one",
                    ));
                }
                Some('{') => {
                    self.advance(); // consume '{'
                    if !text.is_empty() {
                        parts.push(FStringPart::Text(std::mem::take(&mut text)));
                    }
                    let name_line = self.line;
                    let name_column = self.column();
                    let start = self.pos;
                    while let Some(c) = self.peek() {
                        if c == '_' || c.is_alphanumeric() {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let name: String = self.chars[start..self.pos].iter().collect();
                    if name.is_empty() {
                        return Err(MiruError::with_column(
                            name_line,
                            name_column,
                            "an f-string needs a name between '{' and '}'",
                        ));
                    }
                    // A name here never becomes a token, so the check that
                    // catches `let match = 1` cannot see it. Without this,
                    // `f"{match}"` would compile to a reference to a global
                    // nothing is allowed to declare, and fail at run time as an
                    // undefined variable rather than here as what it is.
                    if !self.reserved_words_are_names {
                        if let Some(word) = reserved_word(&name) {
                            return Err(MiruError::with_column(
                                name_line,
                                name_column,
                                reserved_as_a_name(word),
                            ));
                        }
                    }
                    // A name only, deliberately. Any expression is what people
                    // will eventually want, and it puts the parser inside the
                    // lexer, which 0.8 already had to do once. Refusing it now
                    // keeps that release free to allow it: today's error can
                    // become tomorrow's meaning, and the reverse cannot.
                    match self.peek() {
                        Some('}') => {
                            self.advance();
                        }
                        _ => {
                            return Err(MiruError::with_column(
                                name_line,
                                name_column,
                                format!("f-string '{{{name}}}' needs a '}}' after the name"),
                            ))
                        }
                    }
                    parts.push(FStringPart::Name {
                        name,
                        line: name_line,
                        column: name_column,
                    });
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => text.push('\n'),
                        Some('t') => text.push('\t'),
                        Some('r') => text.push('\r'),
                        Some('\\') => text.push('\\'),
                        Some('"') => text.push('"'),
                        Some('0') => text.push('\0'),
                        Some('u') => {
                            text.push(self.read_unicode_escape(line, column)?);
                            continue;
                        }
                        Some(other) => {
                            return Err(MiruError::with_column(
                                line,
                                column,
                                format!("unknown escape sequence '\\{other}'"),
                            ));
                        }
                        None => return Err(unterminated()),
                    }
                    self.advance();
                }
                Some(c) => {
                    text.push(c);
                    self.advance();
                }
            }
        }
        if !text.is_empty() {
            parts.push(FStringPart::Text(text));
        }
        Ok(Token::new(TokenKind::FString(parts), line, column))
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
                        // Each escape above is one character, which the advance
                        // below steps over. This one is not, and consumes its
                        // own text, so it goes straight back round the loop.
                        Some('u') => {
                            value.push(self.read_unicode_escape(line, column)?);
                            continue;
                        }
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

    /// Read a `\u{...}` escape, with the position on the `u`, and give back the
    /// character it names.
    ///
    /// `char::from_u32` decides what is a character. It refuses a value above
    /// `10FFFF`, and it refuses a surrogate, which together are the whole rule.
    /// So no range is checked here.
    ///
    /// The line and the column are those of the opening quotation mark, which
    /// is where every other error in `read_string` is reported.
    fn read_unicode_escape(&mut self, line: usize, column: usize) -> Result<char, MiruError> {
        self.advance(); // consume 'u'
        let fail = |message: String| MiruError::with_column(line, column, message);

        match self.peek() {
            // The string ran out before the escape even opened. What is wrong
            // is the string, not the escape, so it reports as the string.
            None | Some('\n') => return Err(fail("unterminated string literal".to_string())),
            Some('{') => {
                self.advance();
            }
            Some(_) => return Err(fail("escape sequence '\\u' needs a '{'".to_string())),
        }

        let mut value: u32 = 0;
        let mut digits = String::new();
        loop {
            match self.peek() {
                // The string itself ran out before the escape did. That is the
                // error the rest of `read_string` already reports here.
                None | Some('\n') => {
                    return Err(fail("unterminated string literal".to_string()));
                }
                // The string is closed, so the source is complete but the
                // escape is not.
                Some('"') => {
                    return Err(fail("escape sequence '\\u{...}' needs a '}'".to_string()));
                }
                Some('}') => {
                    self.advance();
                    break;
                }
                Some(c) => match c.to_digit(16) {
                    None => {
                        return Err(fail(format!(
                            "escape sequence '\\u{{...}}' takes hexadecimal digits, found '{c}'"
                        )));
                    }
                    Some(digit) => {
                        if digits.len() == MAX_UNICODE_ESCAPE_DIGITS {
                            return Err(fail(format!(
                                "escape sequence '\\u{{...}}' takes at most \
                                 {MAX_UNICODE_ESCAPE_DIGITS} hexadecimal digits"
                            )));
                        }
                        value = value * 16 + digit;
                        digits.push(c);
                        self.advance();
                    }
                },
            }
        }

        if digits.is_empty() {
            return Err(fail(
                "escape sequence '\\u{}' needs at least one hexadecimal digit".to_string(),
            ));
        }

        char::from_u32(value).ok_or_else(|| fail(format!("'\\u{{{digits}}}' is not a character")))
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

    /// An arithmetic operator, or its compound-assignment form if `=` follows.
    ///
    /// The two are told apart here rather than in the parser so that `a +=- 1`
    /// reads as `a += (-1)`, which is the only sensible answer and the one
    /// every language with these operators gives.
    fn assigning(&mut self, compound: TokenKind, plain: TokenKind) -> TokenKind {
        if self.match_char('=') {
            compound
        } else {
            plain
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
        let source = "let s = \"a\\nb\"\nlet f = 1.50\nlet n = 1_000\n";
        let (tokens, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        assert_eq!(tokens.len(), spans.tokens.len());

        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        // The string keeps its quotes and its two-character escape, seven source
        // characters for a value of three.
        assert!(text.contains(&"\"a\\nb\"".to_string()), "{text:?}");
        // The float keeps the trailing zero that its value does not remember.
        assert!(text.contains(&"1.50".to_string()), "{text:?}");
        // The integer keeps the separator, five source characters for a value
        // that remembers four digits.
        assert!(text.contains(&"1_000".to_string()), "{text:?}");
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
    fn a_unicode_escape_keeps_its_source_width_in_its_span() {
        // The widest gap there is between what a token says and what it came
        // from. The literal below is eleven source characters, its value is one
        // character, and that character is four bytes. A span taken from the
        // value would colour ten characters of whatever follows.
        let source = "let e = \"\\u{1F600}\"";
        let (tokens, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        assert_eq!(tokens.len(), spans.tokens.len());
        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        assert!(text.contains(&"\"\\u{1F600}\"".to_string()), "{text:?}");
    }

    #[test]
    fn spans_stay_aligned_after_a_character_outside_the_basic_plane() {
        // `Spans` says a consumer counting UTF-16 units has to iterate code
        // points instead. An emoji is where that bites, being one char here,
        // four bytes, and two UTF-16 units, and `\u{...}` is what makes one
        // easy to put in a file.
        let source = "// \u{1F600}\nlet x = 1";
        let (_, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        assert!(text.contains(&"let".to_string()), "{text:?}");
        assert!(text.contains(&"x".to_string()), "{text:?}");
        assert_eq!(slice(source, spans.comments[0]), "// \u{1F600}");
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
    fn a_dot_gets_a_span_like_any_other_token() {
        // Spans are recorded around `read_token`, so a new token kind gets one
        // without any work. That is worth an assertion rather than an
        // assumption: `Spans::tokens` has to stay index-parallel with the token
        // vector, and both the playground's highlighting and the underline an
        // error draws are wrong the moment it is not.
        let source = "a.b.c";
        let (tokens, spans) = Lexer::tokenize_with_spans(source).expect("lexes");
        assert_eq!(tokens.len(), spans.tokens.len());
        let text: Vec<String> = spans.tokens.iter().map(|s| slice(source, *s)).collect();
        assert_eq!(text, vec!["a", ".", "b", ".", "c", ""]);
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

    /// The value of a source that is one string literal.
    fn string(source: &str) -> String {
        match &kinds(source)[0] {
            TokenKind::Str(value) => value.clone(),
            other => panic!("expected a string token, got {other:?}"),
        }
    }

    /// The message a source that does not lex reports.
    fn error(source: &str) -> String {
        Lexer::tokenize(source)
            .expect_err("source should not tokenize")
            .message
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
    fn a_digit_separator_groups_a_number_without_changing_it() {
        assert_eq!(kinds("1_000"), vec![TokenKind::Int(1000), TokenKind::Eof]);
        assert_eq!(
            kinds("1_000_000"),
            vec![TokenKind::Int(1000000), TokenKind::Eof]
        );
        // The separator is a mark for a reader and no part of the value, so
        // both spellings give one number.
        assert_eq!(kinds("1_000"), kinds("1000"));
    }

    #[test]
    fn a_digit_separator_works_in_both_parts_of_a_float() {
        assert_eq!(
            kinds("1_000.5"),
            vec![TokenKind::Float(1000.5), TokenKind::Eof]
        );
        assert_eq!(
            kinds("1.000_5"),
            vec![TokenKind::Float(1.0005), TokenKind::Eof]
        );
        assert_eq!(kinds("1_0.0_5"), kinds("10.05"));
    }

    #[test]
    fn a_digit_separator_that_is_not_between_two_digits_is_refused() {
        // One rule and one message. A trailing separator, a doubled one, and
        // one before the decimal point are the same mistake seen three ways:
        // nothing that is a digit follows it.
        //
        // Stricter than Rust, which permits `1_` and `1__0`. Nothing valid
        // becomes invalid, because each of these was already an error; what
        // changes is that the lexer says why instead of the parser reporting
        // an identifier the writer never wrote.
        for source in ["1_", "1__0", "1_.5", "1_x", "1.0_", "1.0__5", "1_ 000"] {
            assert_eq!(
                error(source),
                "a digit separator must be between two digits",
                "{source} should be refused"
            );
        }
    }

    #[test]
    fn a_leading_underscore_is_still_a_name_and_not_a_number() {
        // The one thing this change could have broken. A separator is only
        // read inside a number, and `read_number` is only entered when the
        // current character is a digit, so a leading `_` never reaches it.
        assert_eq!(
            kinds("_1"),
            vec![TokenKind::Ident("_1".to_string()), TokenKind::Eof]
        );
        assert_eq!(
            kinds("_"),
            vec![TokenKind::Ident("_".to_string()), TokenKind::Eof]
        );
        assert_eq!(
            kinds("_1_000"),
            vec![TokenKind::Ident("_1_000".to_string()), TokenKind::Eof]
        );

        // The other place a `_` follows a number, and the reason `1_.5` had to
        // be decided rather than left to fall out. A point with no digit after
        // it is still the operator, so this is a field access and not a float.
        assert_eq!(
            kinds("1._5"),
            vec![
                TokenKind::Int(1),
                TokenKind::Dot,
                TokenKind::Ident("_5".to_string()),
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
    fn a_unicode_escape_names_a_character_by_its_value() {
        assert_eq!(string("\"\\u{41}\""), "A");
        assert_eq!(string("\"\\u{1F600}\""), "\u{1F600}");
        // Either case of digit gives the same character.
        assert_eq!(string("\"\\u{1f600}\""), string("\"\\u{1F600}\""));
    }

    #[test]
    fn a_unicode_escape_holds_one_character_however_wide_it_is() {
        // This is what lets the escape stop at the lexer. The emoji below is
        // ten source characters, four bytes, and one character, and every
        // builtin that measures a string counts characters.
        let value = string("\"\\u{1F600}\"");
        assert_eq!(value.chars().count(), 1);
        assert_eq!(value.len(), 4);
    }

    #[test]
    fn a_unicode_escape_reaches_both_ends_of_the_range() {
        assert_eq!(string("\"\\u{0}\""), "\0");
        assert_eq!(string("\"\\u{10FFFF}\""), "\u{10ffff}");
        // A leading zero is a digit like any other, up to the six the escape
        // takes.
        assert_eq!(string("\"\\u{000041}\""), "A");
    }

    #[test]
    fn a_unicode_escape_sits_beside_the_older_escapes() {
        assert_eq!(string("\"a\\n\\u{42}\\tc\""), "a\nB\tc");
    }

    #[test]
    fn a_unicode_escape_with_no_digits_is_refused() {
        // Empty braces are the one malformed escape that has a plausible
        // reading: zero digits could mean the value zero. It does not, because
        // then `\u{}` and `\u{0}` would be two spellings of one character and
        // the first would look like a mistake either way.
        assert_eq!(
            error("\"\\u{}\""),
            "escape sequence '\\u{}' needs at least one hexadecimal digit"
        );
    }

    #[test]
    fn a_unicode_escape_whose_value_is_not_a_character_is_refused() {
        // Two different reasons and one code path. `char::from_u32` refuses a
        // value above the largest character, and it refuses a surrogate, which
        // is not a character either. Neither is checked here, so both report
        // the same way.
        assert_eq!(error("\"\\u{110000}\""), "'\\u{110000}' is not a character");
        assert_eq!(error("\"\\u{D800}\""), "'\\u{D800}' is not a character");
        assert_eq!(error("\"\\u{DFFF}\""), "'\\u{DFFF}' is not a character");

        // Both boundaries, from the accepted side. Without these the refusals
        // above would still pass if the escape refused far too much.
        assert_eq!(string("\"\\u{10FFFF}\"").chars().count(), 1);
        assert_eq!(string("\"\\u{D7FF}\"").chars().count(), 1);
        assert_eq!(string("\"\\u{E000}\"").chars().count(), 1);
    }

    #[test]
    fn a_unicode_escape_that_does_not_end_is_refused_rather_than_panicking() {
        // What this test is for is the panic that is not there. Every source
        // below runs off the end of something while the escape is open, which
        // is where a reader that indexes ahead without looking first falls
        // over.
        assert_eq!(error("\"\\u{41"), "unterminated string literal");
        assert_eq!(error("\"\\u{"), "unterminated string literal");
        assert_eq!(error("\"\\u"), "unterminated string literal");
        assert_eq!(error("\"\\u{41\nx\""), "unterminated string literal");

        // Here the source is not short of anything: the string closes while
        // the escape is still open, so the string is not what is wrong.
        assert_eq!(
            error("\"\\u{41\""),
            "escape sequence '\\u{...}' needs a '}'"
        );
    }

    #[test]
    fn a_unicode_escape_needs_its_braces_and_hexadecimal_digits() {
        // The braces are not decoration. Without them the escape would have to
        // guess where the digits stop, and `"\u41z"` would be a question with
        // no right answer.
        assert_eq!(error("\"\\u41\""), "escape sequence '\\u' needs a '{'");
        assert_eq!(
            error("\"\\u{4G}\""),
            "escape sequence '\\u{...}' takes hexadecimal digits, found 'G'"
        );
        assert_eq!(
            error("\"\\u{4 1}\""),
            "escape sequence '\\u{...}' takes hexadecimal digits, found ' '"
        );
        // A digit that is not ASCII is not a digit here. Devanagari four reads
        // as a four and does not convert, which is what `to_digit` already
        // decides, so this pins the decision rather than adding one.
        assert_eq!(
            error("\"\\u{\u{096a}}\""),
            "escape sequence '\\u{...}' takes hexadecimal digits, found '\u{096a}'"
        );
    }

    #[test]
    fn a_unicode_escape_takes_at_most_six_digits() {
        // Six is the width of the largest character, so nothing is lost, and a
        // seventh digit is refused before it is added rather than after. That
        // is what keeps the accumulator inside a u32 with ordinary arithmetic:
        // without the bound, `"\u{FFFFFFFFFFFF}"` overflows it.
        assert_eq!(string("\"\\u{000041}\""), "A");
        let too_many = format!("\"\\u{{{}}}\"", "0".repeat(MAX_UNICODE_ESCAPE_DIGITS + 1));
        assert_eq!(
            error(&too_many),
            format!(
                "escape sequence '\\u{{...}}' takes at most \
                 {MAX_UNICODE_ESCAPE_DIGITS} hexadecimal digits"
            )
        );
        assert_eq!(
            error("\"\\u{FFFFFFFFFFFF}\""),
            "escape sequence '\\u{...}' takes at most 6 hexadecimal digits"
        );

        // The two bounds are separate. Six digits is a bound on the source, and
        // a value that fits in six digits still has to be a character.
        assert_eq!(error("\"\\u{FFFFFF}\""), "'\\u{FFFFFF}' is not a character");
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
