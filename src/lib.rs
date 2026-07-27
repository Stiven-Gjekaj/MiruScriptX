//! MiruScriptX: a small, general-purpose scripting language written in Rust.
//!
//! This crate exposes the language as a library. The `miru` binary in
//! `src/main.rs` wraps it with a command line interface and a REPL.
//!
//! Source text is turned into tokens by the [`lexer`], parsed into an abstract
//! syntax tree by the [`parser`], compiled to bytecode by the [`compiler`], and
//! executed by the [`vm`].
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
pub mod globals;
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
    vm::Vm::with_output(Box::new(std::io::sink())).run(&program)
}

/// Compile a source string and return its bytecode as readable assembly: the
/// top-level script followed by every function nested inside it. This is what
/// the `disasm` command prints, and it is the only way to see what the compiler
/// actually produced.
pub fn disassemble_source(source: &str) -> Result<String, MiruError> {
    let program = parse_program(source)?;
    // Compiling needs a global table to resolve names against. Nothing runs, so
    // a throwaway one seeded with the builtins gives the same slots a real run
    // would.
    let mut globals = globals::Globals::new();
    builtins::register(&mut globals);
    let script = compiler::Compiler::compile(&program, &mut globals)?;
    Ok(chunk::disassemble_program(&script))
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
    run_source_from(source, None, out)
}

/// Like [`run_source`], but for a program that came from `path`, which is what
/// an `import` inside it resolves against.
///
/// The path-free spelling above delegates here with `None`. Keeping one
/// implementation and a one-line wrapper, rather than two entry points, is why
/// the forty-odd existing callers did not have to change: a wrapper that is
/// literally `f(source, None, out)` cannot drift from what it wraps.
pub fn run_source_from(
    source: &str,
    path: Option<&std::path::Path>,
    out: Box<dyn Write>,
) -> Result<(), MiruError> {
    let program = parse_program(source)?;
    let mut vm = vm::Vm::with_output(out);
    vm.set_input(Box::new(StdinInput));
    vm.run_from(&program, path)?;
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
        let value = self.vm.run(&program)?;
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
    let buffer = Rc::new(RefCell::new(Vec::<u8>::new()));
    let mut vm = vm::Vm::with_output(Box::new(SharedBuffer(Rc::clone(&buffer))));
    vm.set_input(Box::new(ScriptedInput::new(input)));
    vm.run(&program)?;
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

/// One function on the call path an error came through.
///
/// `line` is where the *call* was written, not where that function will resume,
/// so a trace reads as the sequence of call sites a reader can look up in their
/// own source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    /// The name of the function that was entered, or `None` for an anonymous
    /// one.
    pub function: Option<String>,
    pub line: usize,
}

/// How many innermost and outermost entries a rendered trace keeps when the
/// path is too long to print whole.
///
/// Runaway recursion reaches ten thousand frames, and printing every one buries
/// the error in its own trace. The two ends are the parts that carry
/// information: where it broke, and how the program got into the recursion.
const TRACE_HEAD: usize = 5;
const TRACE_TAIL: usize = 5;

/// Append one trace line.
fn write_trace_entry(out: &mut String, entry: &TraceEntry) {
    use std::fmt::Write;

    let name = entry.function.as_deref().unwrap_or("<anonymous>");
    let _ = write!(out, "\n  in {name}, called from line {}", entry.line);
}

/// An error produced anywhere in the pipeline (lexing, parsing, or running),
/// tagged with the 1-based source line and column where it occurred (0 when
/// unknown).
///
/// A runtime error raised inside a call also carries the path of calls it came
/// through, innermost first. Lexing and parsing errors leave `trace` empty:
/// there is no call stack yet when they happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiruError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub trace: Vec<TraceEntry>,
    /// The file this error is about, when it is not the one being rendered.
    ///
    /// An error raised inside an imported module has a line and column in *that*
    /// file, and whoever renders it holds the importing file's source. Naming
    /// the file is what stops a caret being drawn on an unrelated line.
    pub file: Option<String>,
}

impl MiruError {
    /// Create an error at a line, with an unknown column (`0`).
    pub fn new(line: usize, message: impl Into<String>) -> MiruError {
        MiruError::with_column(line, 0, message)
    }

    /// Create an error at a specific line and column. A `column` of `0` means
    /// the column is unknown.
    ///
    /// The trace starts empty. The VM fills it in as the error leaves the frame
    /// it was raised in, which is the only place the frames still exist.
    pub fn with_column(line: usize, column: usize, message: impl Into<String>) -> MiruError {
        MiruError {
            line,
            column,
            message: message.into(),
            trace: Vec::new(),
            file: None,
        }
    }

    /// How wide to underline at this error's position: the length of the token
    /// that starts there, or 1 when there is no such token.
    ///
    /// The spans come from the lexer, which records them for the playground's
    /// syntax highlighting, and are index-parallel with the tokens it returns.
    /// Every token carries its own line and column, so the one at this error's
    /// position is found by matching on those rather than by converting the
    /// position into an offset and comparing that.
    ///
    /// Re-lexing here costs nothing worth counting. It happens once, on the
    /// error path, for a program that has already failed.
    ///
    /// Both fallbacks are real cases rather than defensive padding. A source
    /// that does not lex is reporting a lexer error, whose position points at a
    /// character rather than a token, so there is nothing to measure. And `Eof`
    /// covers no text at all, so every "found end of input" error would
    /// otherwise underline zero characters.
    fn underline_width(&self, source: &str) -> usize {
        let Ok((tokens, spans)) = crate::lexer::Lexer::tokenize_with_spans(source) else {
            return 1;
        };
        tokens
            .iter()
            .zip(&spans.tokens)
            .find(|(token, _)| token.line == self.line && token.column == self.column)
            .map_or(1, |(_, (_, len))| (*len).max(1))
    }

    /// Render the error with the offending source line, an underline under the
    /// token it blames, and the path of calls it came through.
    ///
    /// The source line and underline are omitted when the line or column is
    /// unknown or out of range, and the trace when there is none, so an error
    /// with neither renders as the one-line
    /// [`Display`](std::fmt::Display) form.
    pub fn render(&self, source: &str) -> String {
        use std::fmt::Write;

        let mut out = self.to_string();
        if let Some(text) = self
            .line
            .checked_sub(1)
            .filter(|_| self.column > 0)
            // An error from another file has a position in that file, and this
            // is not that file's text. Drawing a line from it would point at
            // something unrelated with total confidence.
            .filter(|_| self.file.is_none())
            .and_then(|index| source.lines().nth(index))
        {
            // Build the indent from the source itself so that tabs before the
            // column keep the underline aligned.
            let mut caret = String::new();
            for ch in text.chars().take(self.column - 1) {
                caret.push(if ch == '\t' { '\t' } else { ' ' });
            }
            // Underline the whole token rather than pointing at its first
            // character. Capped by what is left of the line, so a token that
            // somehow ran past the end could not carry the underline with it,
            // and never below one, since an error at end of input sits one
            // column past the last character and still has to mark something.
            let remaining = text.chars().count().saturating_sub(self.column - 1);
            for _ in 0..self.underline_width(source).min(remaining.max(1)) {
                caret.push('^');
            }
            let _ = write!(out, "\n    {text}\n    {caret}");
        }
        // Elide the middle of a long path rather than the data itself: `trace`
        // stays complete for anything that wants to inspect it, and only what
        // is printed is shortened. The threshold is one past what elision could
        // fit, so it never replaces a single frame with a line saying one frame
        // was replaced, which also keeps the count plural.
        if self.trace.len() > TRACE_HEAD + TRACE_TAIL + 1 {
            for entry in &self.trace[..TRACE_HEAD] {
                write_trace_entry(&mut out, entry);
            }
            let elided = self.trace.len() - TRACE_HEAD - TRACE_TAIL;
            let _ = write!(out, "\n  ... {elided} more frames");
            for entry in &self.trace[self.trace.len() - TRACE_TAIL..] {
                write_trace_entry(&mut out, entry);
            }
        } else {
            for entry in &self.trace {
                write_trace_entry(&mut out, entry);
            }
        }
        out
    }
}

impl fmt::Display for MiruError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let where_ = match (&self.file, self.line, self.column) {
            (None, 0, _) => String::new(),
            (None, line, 0) => format!(" (line {line})"),
            (None, line, column) => format!(" (line {line}, column {column})"),
            (Some(file), 0, _) => format!(" ({file})"),
            (Some(file), line, 0) => format!(" ({file}, line {line})"),
            (Some(file), line, column) => format!(" ({file}, line {line}, column {column})"),
        };
        write!(f, "error{where_}: {}", self.message)
    }
}

impl std::error::Error for MiruError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an error carrying `count` identical trace entries.
    fn traced(count: usize) -> MiruError {
        let mut error = MiruError::with_column(1, 1, "boom");
        error.trace = (0..count)
            .map(|_| TraceEntry {
                function: Some("r".to_string()),
                line: 1,
            })
            .collect();
        error
    }

    #[test]
    fn a_long_trace_is_elided_in_the_middle() {
        // The threshold is the last length elision cannot shorten. At it, every
        // frame prints; one past it, the middle collapses.
        let full = traced(TRACE_HEAD + TRACE_TAIL + 1).render("x");
        assert_eq!(full.matches("in r, called from line 1").count(), 11);
        assert!(!full.contains("more frames"), "{full}");

        let elided = traced(TRACE_HEAD + TRACE_TAIL + 2).render("x");
        assert_eq!(
            elided.matches("in r, called from line 1").count(),
            TRACE_HEAD + TRACE_TAIL
        );
        // Never "1 more frames": the threshold is set so at least two collapse.
        assert!(elided.contains("... 2 more frames"), "{elided}");

        let deep = traced(10_000).render("x");
        assert!(deep.contains("... 9990 more frames"), "{deep}");
        assert_eq!(deep.lines().count(), 1 + 2 + TRACE_HEAD + 1 + TRACE_TAIL);
    }

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
    fn render_underlines_a_whole_token() {
        // The point of the change: a name is marked over its whole length
        // rather than at its first character.
        let err = MiruError::with_column(1, 1, "boom");
        assert_eq!(
            err.render("total = 1"),
            "error (line 1, column 1): boom\n    total = 1\n    ^^^^^"
        );
    }

    #[test]
    fn render_underlines_one_character_when_the_token_is_one_character() {
        // The common case stays exactly as it was, which is why so few existing
        // expectations moved: most errors point at an operator.
        let err = MiruError::with_column(1, 3, "boom");
        assert_eq!(
            err.render("1 + 2"),
            "error (line 1, column 3): boom\n    1 + 2\n      ^"
        );
    }

    #[test]
    fn render_falls_back_to_one_caret_at_end_of_input() {
        // Eof covers no text, so its recorded span is genuinely zero long. Every
        // "found end of input" error would underline nothing without the floor.
        let err = MiruError::with_column(1, 8, "boom");
        assert_eq!(
            err.render("let x ="),
            "error (line 1, column 8): boom\n    let x =\n           ^"
        );
    }

    #[test]
    fn render_falls_back_to_one_caret_when_the_source_does_not_lex() {
        // A lexer error points at a character rather than a token, so there is
        // no token to measure and one caret is the right answer.
        let err = MiruError::with_column(1, 1, "unexpected character '@'");
        assert_eq!(
            err.render("@"),
            "error (line 1, column 1): unexpected character '@'\n    @\n    ^"
        );
    }

    #[test]
    fn render_falls_back_to_one_caret_inside_a_token() {
        // Column 3 is the middle of `total`, not the start of anything, so
        // nothing matches and the underline stays a single caret.
        let err = MiruError::with_column(1, 3, "boom");
        assert_eq!(
            err.render("total"),
            "error (line 1, column 3): boom\n    total\n      ^"
        );
    }

    #[test]
    fn render_measures_a_token_in_characters_rather_than_bytes() {
        // Three multi-byte characters in quotes are five characters and eleven
        // bytes. Measuring in bytes would run the underline off the line.
        let err = MiruError::with_column(1, 1, "boom");
        assert_eq!(
            err.render("\"\u{4e2d}\u{4e2d}\u{4e2d}\" + 1"),
            "error (line 1, column 1): boom\n    \"\u{4e2d}\u{4e2d}\u{4e2d}\" + 1\n    ^^^^^"
        );
    }

    #[test]
    fn an_error_from_another_file_names_it_and_draws_no_caret() {
        // The position belongs to the named file, and `render` is holding the
        // importing file's text. Drawing a line from it would point at
        // something unrelated with total confidence, so it does not.
        let mut err = MiruError::with_column(3, 5, "undefined variable 'x'");
        err.file = Some("./math.miru".to_string());
        assert_eq!(
            err.render("let a = 1\nlet b = 2\nlet c = 3\n"),
            "error (./math.miru, line 3, column 5): undefined variable 'x'"
        );
    }

    #[test]
    fn a_file_is_named_whatever_position_is_known() {
        let mut err = MiruError::new(7, "boom");
        err.file = Some("./m.miru".to_string());
        assert_eq!(err.to_string(), "error (./m.miru, line 7): boom");

        let mut bare = MiruError::with_column(0, 0, "boom");
        bare.file = Some("./m.miru".to_string());
        assert_eq!(bare.to_string(), "error (./m.miru): boom");
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
    fn format_source_preserves_comments() {
        let source = include_str!("../examples/contacts.miru");
        let formatted = format_source(source).expect("formats");
        assert!(formatted.contains("// Build a small phone book"));
        assert!(formatted.contains("// Add a new entry and update an existing one."));
        assert!(formatted.contains("// Look up a name, guarding against a missing one."));
    }
}
