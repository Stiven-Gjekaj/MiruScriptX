//! Token definitions for the MiruScriptX lexer.

/// A single token produced by the lexer, tagged with the 1-based source line and
/// column it starts on so later stages can report precise error locations.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Token {
        Token { kind, line, column }
    }
}

/// One piece of an `f"..."` literal: fixed text, or a name to render.
///
/// A name carries its own position so a caret can point inside the string
/// rather than at the quotation mark that opens it.
#[derive(Debug, Clone, PartialEq)]
pub enum FStringPart {
    Text(String),
    Name {
        name: String,
        line: usize,
        column: usize,
    },
}

/// The words version 2 reserves, which version 1 allowed as names.
///
/// **This is the whole keyword budget, spent once.** Sixteen words were already
/// keywords, so this doubles the count in a single release, and that is the
/// point: a language that reserves one word per major version needs a major
/// version for every feature that wants a word. Each of these is wanted by a
/// construct somebody has asked for, and reserving them together costs one
/// break instead of sixteen.
///
/// **`type` is not here although it nearly was.** It is the only candidate that
/// collided with a builtin, and reserving it would have deleted `type(x)` from
/// the language in exchange for a type-alias construct no issue has asked for.
/// Check a keyword list against the builtin names before agreeing to it.
///
/// **This is the rename list, not the token list.** A word here is one that
/// version 1 allowed as a name and version 2 does not, which is exactly what
/// `miru migrate` has to rewrite. Whether the lexer gives it a token of its own
/// is a separate question: `match` has a grammar and a [`TokenKind::Match`],
/// and the other fifteen share [`TokenKind::Reserved`] because they mean
/// nothing yet. Both are still words a version 1 program may have used.
///
/// Sorted, and no word here ends in an underscore, which is what lets
/// [`crate::migrate`] rename `match` to `match_` and know the result is free.
pub const RESERVED_WORDS: [&str; 16] = [
    "async", "await", "case", "const", "default", "defer", "enum", "finally", "is", "loop",
    "match", "pub", "struct", "until", "use", "yield",
];

/// The reserved word `name` spells, as a `'static` string, or `None`.
///
/// Returns the entry from [`RESERVED_WORDS`] rather than the caller's string so
/// that a token can hold a `&'static str` and stay cheap to clone.
pub fn reserved_word(name: &str) -> Option<&'static str> {
    RESERVED_WORDS.iter().copied().find(|word| *word == name)
}

/// What to say when a program uses a reserved word where a name belongs.
///
/// One wording, because a program reaches it from eight positions (`let`, a
/// `fn` name, a parameter, a loop variable, an `import` alias, a field, a read
/// of the variable, and inside an f-string) through five call sites, and being
/// told the same thing each time is what makes it read as one change rather
/// than eight. It names the fix, because the fix is a command the reader
/// already has.
pub fn reserved_as_a_name(word: &str) -> String {
    format!(
        "'{word}' is a keyword and cannot be a name. \
         'miru migrate -w' renames it, and reads a version 1 program to do it."
    )
}

/// Every kind of token MiruScriptX recognizes.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals.
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),

    // Keywords.
    Fn,
    Let,
    Return,
    If,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Import,
    As,
    /// `try expr`: evaluate the expression and, if it fails, produce the
    /// error as a value rather than stopping the program.
    Try,
    True,
    False,
    Nil,
    /// `match subject { .. }`. Reserved by version 2 along with the fifteen
    /// below, and given a grammar in the same release, which is why it has a
    /// variant of its own and they do not.
    Match,

    /// One of the words in [`RESERVED_WORDS`]: a keyword with no grammar yet.
    ///
    /// **One variant for all sixteen, rather than sixteen variants.** None of
    /// them means anything yet, so a variant each would be sixteen names the
    /// parser never matches on. A release that gives one of these words a
    /// grammar takes it out of the table and gives it a variant then, which is
    /// where the variant earns its place.
    Reserved(&'static str),

    // Operators.
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %
    /// An `f"..."` literal, already split into its text and its names.
    FString(Vec<FStringPart>),
    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=
    Assign,        // =
    Eq,            // ==
    NotEq,         // !=
    Lt,            // <
    Gt,            // >
    LtEq,          // <=
    GtEq,          // >=
    Bang,          // !
    And,           // &&
    Or,            // ||

    // Delimiters.
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    /// `...`, which marks the rest parameter in a parameter list.
    ///
    /// One token rather than three dots, so that a call site spreading an
    /// array back out (`f(...args)`) has a token waiting for it. Issue #50
    /// asked for that room to be left.
    Ellipsis, // ...
    Colon,    // :
    Dot,      // .

    // Structural.
    Newline, // a statement separator (also produced by ';')
    Eof,
}

impl TokenKind {
    /// The reserved word this token spells, if it is one.
    ///
    /// **A word promoted out of [`TokenKind::Reserved`] keeps its message.**
    /// `match` has a grammar and a token of its own, and it is still a word
    /// version 1 allowed as a name, so `let match = 1` has to say what happened
    /// to it rather than "expected an identifier". Every reserved word answers
    /// here whether or not it has a variant.
    pub fn reserved_name(&self) -> Option<&'static str> {
        match self {
            TokenKind::Reserved(word) => Some(word),
            TokenKind::Match => Some("match"),
            _ => None,
        }
    }

    /// A short, human friendly description used in parser error messages.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int(n) => format!("integer '{n}'"),
            TokenKind::Float(n) => format!("float '{n}'"),
            TokenKind::Str(s) => format!("string \"{s}\""),
            TokenKind::Ident(name) => format!("identifier '{name}'"),
            TokenKind::Fn => "'fn'".to_string(),
            TokenKind::Let => "'let'".to_string(),
            TokenKind::Return => "'return'".to_string(),
            TokenKind::If => "'if'".to_string(),
            TokenKind::Else => "'else'".to_string(),
            TokenKind::While => "'while'".to_string(),
            TokenKind::For => "'for'".to_string(),
            TokenKind::In => "'in'".to_string(),
            TokenKind::Break => "'break'".to_string(),
            TokenKind::Continue => "'continue'".to_string(),
            TokenKind::Import => "'import'".to_string(),
            TokenKind::As => "'as'".to_string(),
            TokenKind::Try => "'try'".to_string(),
            TokenKind::True => "'true'".to_string(),
            TokenKind::False => "'false'".to_string(),
            TokenKind::Nil => "'nil'".to_string(),
            TokenKind::Match => "'match'".to_string(),
            TokenKind::Reserved(word) => format!("'{word}'"),
            TokenKind::Plus => "'+'".to_string(),
            TokenKind::Minus => "'-'".to_string(),
            TokenKind::Star => "'*'".to_string(),
            TokenKind::Slash => "'/'".to_string(),
            TokenKind::Percent => "'%'".to_string(),
            TokenKind::FString(_) => "an f-string".to_string(),
            TokenKind::PlusAssign => "'+='".to_string(),
            TokenKind::MinusAssign => "'-='".to_string(),
            TokenKind::StarAssign => "'*='".to_string(),
            TokenKind::SlashAssign => "'/='".to_string(),
            TokenKind::PercentAssign => "'%='".to_string(),
            TokenKind::Assign => "'='".to_string(),
            TokenKind::Eq => "'=='".to_string(),
            TokenKind::NotEq => "'!='".to_string(),
            TokenKind::Lt => "'<'".to_string(),
            TokenKind::Gt => "'>'".to_string(),
            TokenKind::LtEq => "'<='".to_string(),
            TokenKind::GtEq => "'>='".to_string(),
            TokenKind::Bang => "'!'".to_string(),
            TokenKind::And => "'&&'".to_string(),
            TokenKind::Or => "'||'".to_string(),
            TokenKind::LParen => "'('".to_string(),
            TokenKind::RParen => "')'".to_string(),
            TokenKind::LBrace => "'{'".to_string(),
            TokenKind::RBrace => "'}'".to_string(),
            TokenKind::LBracket => "'['".to_string(),
            TokenKind::RBracket => "']'".to_string(),
            TokenKind::Comma => "','".to_string(),
            TokenKind::Ellipsis => "'...'".to_string(),
            TokenKind::Colon => "':'".to_string(),
            TokenKind::Dot => "'.'".to_string(),
            TokenKind::Newline => "end of line".to_string(),
            TokenKind::Eof => "end of input".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reserved_word_is_also_a_builtin() {
        // The check that came one step too late. `type` was agreed onto the
        // reserved list and then found to be a builtin that the specification,
        // the wiki and the examples call in eleven places: reserving it would
        // have deleted `type(x)` in exchange for a construct nobody has asked
        // for. Nothing stops that happening again except this.
        let collisions: Vec<&str> = RESERVED_WORDS
            .iter()
            .copied()
            .filter(|word| crate::builtins::BUILTIN_NAMES.contains(word))
            .collect();
        assert!(
            collisions.is_empty(),
            "reserving {collisions:?} would delete a builtin of the same name"
        );
    }

    #[test]
    fn no_reserved_word_ends_in_an_underscore() {
        // What lets `miru migrate` rename `match` to `match_` and know the
        // result is a name rather than another refusal. Its loop appends
        // underscores until the result is free, and would not terminate if a
        // reserved word could be reached that way.
        for word in RESERVED_WORDS {
            assert!(!word.ends_with('_'), "{word} ends in an underscore");
        }
    }

    #[test]
    fn the_reserved_words_are_sorted_and_distinct() {
        let mut sorted = RESERVED_WORDS;
        sorted.sort_unstable();
        assert_eq!(sorted, RESERVED_WORDS, "the list is not in order");
        let mut seen = std::collections::HashSet::new();
        for word in RESERVED_WORDS {
            assert!(seen.insert(word), "{word} appears twice");
        }
    }

    #[test]
    fn no_reserved_word_lexes_as_a_name() {
        // What the list is for. Whether a word gets a token of its own is a
        // separate question, and `match` has one because it has a grammar; what
        // matters here is that none of them arrives as an identifier, because
        // an identifier is what the whole break was about.
        for word in RESERVED_WORDS {
            assert!(
                reserved_word(word).is_some(),
                "{word} is not in its own list"
            );
            let tokens = crate::lexer::Lexer::tokenize(&format!("{word}\n")).expect("lexes");
            assert!(
                !matches!(tokens[0].kind, TokenKind::Ident(_)),
                "{word} still lexes as a name: {:?}",
                tokens[0].kind
            );
        }
    }

    #[test]
    fn a_reserved_word_with_a_grammar_gets_a_token_of_its_own() {
        // The rule `TokenKind::Reserved` states: a release that gives one of
        // these words a grammar takes it out of the shared variant then. If
        // `match` were still `Reserved("match")`, the parser would be matching
        // on a string to find a construct.
        let tokens = crate::lexer::Lexer::tokenize("match\n").expect("lexes");
        assert_eq!(tokens[0].kind, TokenKind::Match);
        for word in RESERVED_WORDS.iter().filter(|w| **w != "match") {
            let tokens = crate::lexer::Lexer::tokenize(&format!("{word}\n")).expect("lexes");
            assert_eq!(
                tokens[0].kind,
                TokenKind::Reserved(word),
                "{word} has no grammar, so it should share the reserved variant"
            );
        }
    }

    #[test]
    fn the_version_1_lexer_reads_every_reserved_word_as_a_name() {
        // The whole reason `miru migrate` still works from a version 2 binary.
        for word in RESERVED_WORDS {
            let (tokens, _) =
                crate::lexer::Lexer::tokenize_1x_with_spans(&format!("{word}\n")).expect("lexes");
            assert_eq!(
                tokens[0].kind,
                TokenKind::Ident(word.to_string()),
                "{word} is not readable as a name by the version 1 lexer"
            );
        }
    }
}
