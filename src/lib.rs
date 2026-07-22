//! MiruScriptX: a small, general-purpose scripting language written in Rust.
//!
//! This crate exposes the language as a library. The `miru` binary in
//! `src/main.rs` wraps it with a command line interface and a REPL.
//!
//! The pipeline is a classic tree walker: source text is turned into tokens by
//! the [`lexer`], parsed into an abstract syntax tree, and then evaluated.
//!
//! Learn the language in the `wiki/` folder and look things up in
//! `docs/language-reference.md`.

use std::fmt;

pub mod lexer;
pub mod token;

/// The MiruScriptX version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An error produced anywhere in the pipeline (lexing, parsing, or running),
/// tagged with the 1-based source line where it occurred (0 when unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiruError {
    pub line: usize,
    pub message: String,
}

impl MiruError {
    pub fn new(line: usize, message: impl Into<String>) -> MiruError {
        MiruError {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for MiruError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "error: {}", self.message)
        } else {
            write!(f, "error (line {}): {}", self.line, self.message)
        }
    }
}

impl std::error::Error for MiruError {}
