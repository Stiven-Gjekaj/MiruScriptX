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
    True,
    False,
    Nil,

    // Operators.
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %
    Assign,  // =
    Eq,      // ==
    NotEq,   // !=
    Lt,      // <
    Gt,      // >
    LtEq,    // <=
    GtEq,    // >=
    Bang,    // !
    And,     // &&
    Or,      // ||

    // Delimiters.
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Colon,    // :
    Dot,      // .

    // Structural.
    Newline, // a statement separator (also produced by ';')
    Eof,
}

impl TokenKind {
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
            TokenKind::True => "'true'".to_string(),
            TokenKind::False => "'false'".to_string(),
            TokenKind::Nil => "'nil'".to_string(),
            TokenKind::Plus => "'+'".to_string(),
            TokenKind::Minus => "'-'".to_string(),
            TokenKind::Star => "'*'".to_string(),
            TokenKind::Slash => "'/'".to_string(),
            TokenKind::Percent => "'%'".to_string(),
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
            TokenKind::Colon => "':'".to_string(),
            TokenKind::Dot => "'.'".to_string(),
            TokenKind::Newline => "end of line".to_string(),
            TokenKind::Eof => "end of input".to_string(),
        }
    }
}
