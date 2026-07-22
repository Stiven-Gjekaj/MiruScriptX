//! MiruScriptX: a small, general-purpose scripting language written in Rust.
//!
//! This crate exposes the language as a library. The `miru` binary in
//! `src/main.rs` wraps it with a command line interface and a REPL.
//!
//! The pipeline is a classic tree walker: source text is turned into tokens by
//! the [`lexer`], parsed into an abstract syntax tree by the [`parser`], and
//! then evaluated by the [`interpreter`].
//!
//! Learn the language in the `wiki/` folder and look things up in
//! `docs/language-reference.md`.

use std::cell::RefCell;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

pub mod ast;
pub mod builtins;
pub mod environment;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod value;

/// The MiruScriptX version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lex and parse a source string into a program (a list of statements).
pub fn parse_program(source: &str) -> Result<Vec<ast::Stmt>, MiruError> {
    let tokens = lexer::Lexer::tokenize(source)?;
    parser::Parser::parse(tokens)
}

/// Lex, parse, and run a source string, sending `print` output to `out`.
pub fn run_source(source: &str, out: Box<dyn Write>) -> Result<(), MiruError> {
    let program = parse_program(source)?;
    let mut interpreter = interpreter::Interpreter::with_output(out);
    interpreter.run_program(&program)?;
    interpreter.flush();
    Ok(())
}

/// Run a source string and capture everything it printed. Handy for tests and
/// tooling that needs the output as a string rather than on a stream.
pub fn run_capture(source: &str) -> Result<String, MiruError> {
    let program = parse_program(source)?;
    let buffer = Rc::new(RefCell::new(Vec::<u8>::new()));
    let mut interpreter =
        interpreter::Interpreter::with_output(Box::new(SharedBuffer(Rc::clone(&buffer))));
    interpreter.run_program(&program)?;
    interpreter.flush();
    let bytes = buffer.borrow();
    Ok(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

/// A `Write` sink backed by a shared byte buffer, used by [`run_capture`].
struct SharedBuffer(Rc<RefCell<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn out(source: &str) -> String {
        run_capture(source).expect("program should run")
    }

    #[test]
    fn print_writes_a_line() {
        assert_eq!(out("print(1 + 2)"), "3\n");
    }

    #[test]
    fn print_joins_arguments_with_spaces() {
        assert_eq!(out("print(1, \"two\", true)"), "1 two true\n");
    }

    #[test]
    fn len_of_strings_and_arrays() {
        assert_eq!(out("print(len(\"hello\"))\nprint(len([1, 2, 3]))"), "5\n3\n");
    }

    #[test]
    fn type_names() {
        assert_eq!(
            out("print(type(1), type(1.5), type(\"a\"), type([]), type(nil))"),
            "int float string array nil\n"
        );
    }

    #[test]
    fn range_builds_a_half_open_interval() {
        assert_eq!(out("print(range(3))"), "[0, 1, 2]\n");
        assert_eq!(out("print(range(2, 5))"), "[2, 3, 4]\n");
    }

    #[test]
    fn push_appends_in_place() {
        assert_eq!(out("let a = [1]\npush(a, 2)\nprint(a)"), "[1, 2]\n");
    }

    #[test]
    fn str_converts_and_concatenates() {
        assert_eq!(out("print(str(42) + \"!\")"), "42!\n");
    }
}
