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

use crate::value::Input;

pub mod ast;
pub mod builtins;
pub mod chunk;
pub mod compiler;
pub mod formatter;
pub mod lexer;
pub mod ops;
pub mod parser;
pub mod token;
pub mod value;
pub mod vm;

/// The MiruScriptX version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lex and parse a source string into a program (a list of statements).
pub fn parse_program(source: &str) -> Result<Vec<ast::Stmt>, MiruError> {
    let tokens = lexer::Lexer::tokenize(source)?;
    parser::Parser::parse(tokens)
}

/// Run a program and return the value of its final expression (what the REPL
/// echoes), discarding anything it printed.
pub fn eval_source(source: &str) -> Result<value::Value, MiruError> {
    let program = parse_program(source)?;
    let script = compiler::Compiler::compile(&program)?;
    vm::Vm::with_output(Box::new(std::io::sink())).interpret(script)
}

/// Lex, parse, and reprint a source string in the canonical `miru fmt` style,
/// preserving comments and single blank lines. This is what the `fmt` command
/// runs on a file.
pub fn format_source(source: &str) -> Result<String, MiruError> {
    let (tokens, trivia) = lexer::Lexer::tokenize_with_trivia(source)?;
    let program = parser::Parser::parse(tokens)?;
    Ok(formatter::format_program(&program, &trivia))
}

/// Lex, parse, compile, and run a source string, sending `print` output to
/// `out` and reading `input()` from standard input.
pub fn run_source(source: &str, out: Box<dyn Write>) -> Result<(), MiruError> {
    let program = parse_program(source)?;
    let script = compiler::Compiler::compile(&program)?;
    let mut vm = vm::Vm::with_output(out);
    vm.set_input(Box::new(StdinInput));
    vm.interpret(script)?;
    vm.flush();
    Ok(())
}

/// An interactive session: a virtual machine whose globals persist from one
/// program to the next, so a variable or function defined by one input is still
/// there for the next. The REPL is built on this.
///
/// A failed input reports its error and leaves the session usable, so a typo or
/// a runtime error does not end the session or corrupt what came before.
pub struct Session {
    vm: vm::Vm,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    /// A session that prints to standard output and reads standard input.
    pub fn new() -> Session {
        let mut session = Session::with_output(Box::new(std::io::stdout()));
        session.vm.set_input(Box::new(StdinInput));
        session
    }

    /// A session that prints to a custom sink, as tests do.
    pub fn with_output(out: Box<dyn Write>) -> Session {
        Session {
            vm: vm::Vm::with_output(out),
        }
    }

    /// Replace the input source that `input()` reads from.
    pub fn set_input(&mut self, input: Box<dyn Input>) {
        self.vm.set_input(input);
    }

    /// Parse, compile, and run one input against the session's accumulated
    /// state, returning the value of its final expression (or `nil`).
    pub fn eval(&mut self, source: &str) -> Result<value::Value, MiruError> {
        let program = parse_program(source)?;
        let script = compiler::Compiler::compile(&program)?;
        let value = self.vm.interpret(script)?;
        self.vm.flush();
        Ok(value)
    }

    /// Flush anything the session has buffered.
    pub fn flush(&mut self) {
        self.vm.flush();
    }
}

/// Run a source string and capture everything it printed. Handy for tests and
/// tooling that needs the output as a string rather than on a stream.
pub fn run_capture(source: &str) -> Result<String, MiruError> {
    run_capture_with_input(source, &[])
}

/// Like [`run_capture`], but feeds the given lines to `input()` in order.
pub fn run_capture_with_input(source: &str, input: &[&str]) -> Result<String, MiruError> {
    let program = parse_program(source)?;
    let script = compiler::Compiler::compile(&program)?;
    let buffer = Rc::new(RefCell::new(Vec::<u8>::new()));
    let mut vm = vm::Vm::with_output(Box::new(SharedBuffer(Rc::clone(&buffer))));
    vm.set_input(Box::new(ScriptedInput::new(input)));
    vm.interpret(script)?;
    vm.flush();
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

/// Reads lines from the process's standard input, one per `input()` call.
struct StdinInput;

impl Input for StdinInput {
    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(strip_trailing_newline(line)),
            Err(_) => None,
        }
    }
}

/// An [`Input`] backed by a fixed list of lines, used by the capture helpers.
struct ScriptedInput {
    lines: std::vec::IntoIter<String>,
}

impl ScriptedInput {
    fn new(lines: &[&str]) -> ScriptedInput {
        let lines: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
        ScriptedInput {
            lines: lines.into_iter(),
        }
    }
}

impl Input for ScriptedInput {
    fn read_line(&mut self) -> Option<String> {
        self.lines.next()
    }
}

/// Remove a single trailing newline (and any preceding carriage return).
fn strip_trailing_newline(mut line: String) -> String {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    line
}

/// An error produced anywhere in the pipeline (lexing, parsing, or running),
/// tagged with the 1-based source line and column where it occurred (0 when
/// unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiruError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl MiruError {
    /// Create an error at a line, with an unknown column (`0`).
    pub fn new(line: usize, message: impl Into<String>) -> MiruError {
        MiruError::with_column(line, 0, message)
    }

    /// Create an error at a specific line and column. A `column` of `0` means
    /// the column is unknown.
    pub fn with_column(line: usize, column: usize, message: impl Into<String>) -> MiruError {
        MiruError {
            line,
            column,
            message: message.into(),
        }
    }

    /// Render the error with the offending source line and a caret under the
    /// column. Falls back to the one-line [`Display`](std::fmt::Display) form
    /// when the line or column is unknown or out of range.
    pub fn render(&self, source: &str) -> String {
        let header = self.to_string();
        if self.line == 0 || self.column == 0 {
            return header;
        }
        let Some(text) = source.lines().nth(self.line - 1) else {
            return header;
        };
        // Build the caret indent from the source itself so that tabs before the
        // column keep the caret aligned.
        let mut caret = String::new();
        for ch in text.chars().take(self.column - 1) {
            caret.push(if ch == '\t' { '\t' } else { ' ' });
        }
        caret.push('^');
        format!("{header}\n    {text}\n    {caret}")
    }
}

impl fmt::Display for MiruError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (0, _) => write!(f, "error: {}", self.message),
            (line, 0) => write!(f, "error (line {line}): {}", self.message),
            (line, column) => write!(f, "error (line {line}, column {column}): {}", self.message),
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
    fn render_points_a_caret_at_the_column() {
        let err = MiruError::with_column(1, 5, "boom");
        assert_eq!(
            err.render("abcdefg"),
            "error (line 1, column 5): boom\n    abcdefg\n        ^"
        );
    }

    #[test]
    fn render_preserves_leading_tabs_in_the_caret() {
        let err = MiruError::with_column(1, 2, "boom");
        assert_eq!(
            err.render("\tx"),
            "error (line 1, column 2): boom\n    \tx\n    \t^"
        );
    }

    #[test]
    fn render_without_a_column_is_one_line() {
        let err = MiruError::new(3, "oops");
        assert_eq!(err.render("a\nb\nc"), "error (line 3): oops");
    }

    #[test]
    fn print_joins_arguments_with_spaces() {
        assert_eq!(out("print(1, \"two\", true)"), "1 two true\n");
    }

    #[test]
    fn len_of_strings_and_arrays() {
        assert_eq!(
            out("print(len(\"hello\"))\nprint(len([1, 2, 3]))"),
            "5\n3\n"
        );
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

    #[test]
    fn map_builtins() {
        let source = "let m = {\"b\": 2, \"a\": 1}\nprint(keys(m))\nprint(values(m))\nprint(has(m, \"a\"))\nprint(has(m, \"z\"))\nprint(len(m))";
        assert_eq!(out(source), "[\"a\", \"b\"]\n[1, 2]\ntrue\nfalse\n2\n");
    }

    const EXAMPLES: [&str; 6] = [
        include_str!("../examples/greet.miru"),
        include_str!("../examples/fib.miru"),
        include_str!("../examples/fizzbuzz.miru"),
        include_str!("../examples/contacts.miru"),
        include_str!("../examples/greeter.miru"),
        include_str!("../examples/transform.miru"),
    ];

    #[test]
    fn format_source_is_idempotent_on_examples() {
        for source in EXAMPLES {
            let once = format_source(source).expect("formats");
            let twice = format_source(&once).expect("reformats");
            assert_eq!(once, twice, "formatting is not idempotent");
        }
    }

    #[test]
    fn format_source_does_not_change_behavior() {
        // Reprinting must never change what a program does. Every example runs
        // deterministically (greeter reads from an empty input and says goodbye).
        for source in EXAMPLES {
            let formatted = format_source(source).expect("formats");
            assert_eq!(
                run_capture(source).expect("source runs"),
                run_capture(&formatted).expect("formatted runs"),
                "formatting changed program behavior"
            );
        }
    }

    #[test]
    fn both_engines_produce_the_same_output_for_every_example() {
        // The whole point of running two engines in v0.4: they must be
        // indistinguishable on real programs, not just on unit-test snippets.
        for source in EXAMPLES {
            let tree = run_capture(source).expect("the tree walker runs the example");
            let vm = run_capture(source).expect("the VM runs the example");
            assert_eq!(tree, vm, "the engines printed different output");
        }
    }

    #[test]
    fn both_engines_read_input_the_same_way() {
        let source = include_str!("../examples/greeter.miru");
        let tree = run_capture_with_input(source, &["Aiko"]).expect("tree walker runs");
        let vm = run_capture_with_input(source, &["Aiko"]).expect("VM runs");
        assert_eq!(tree, vm);
    }

    #[test]
    fn format_source_preserves_comments() {
        let source = include_str!("../examples/contacts.miru");
        let formatted = format_source(source).expect("formats");
        assert!(formatted.contains("// Build a small phone book"));
        assert!(formatted.contains("// Add a new entry and update an existing one."));
        assert!(formatted.contains("// Look up a name, guarding against a missing one."));
    }
}
