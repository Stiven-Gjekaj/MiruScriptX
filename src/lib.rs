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
pub mod random;
pub mod suggest;
pub mod token;
pub mod value;
pub mod vm;

/// The MiruScriptX version, taken from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lex and parse a source string into a program, giving **every** syntax error
/// rather than only the first.
///
/// The list is never empty on the failing side, and holds at most
/// [`parser::Parser::MAX_ERRORS`].
///
/// **A failure to lex is still one error.** The lexer is a stage before this
/// one, and a program that does not lex never reaches the parser, so an
/// unterminated string or a bad `\u{...}` stops where it always did. Section 6
/// of the specification says so.
pub fn parse_program_all(source: &str) -> Result<Vec<ast::Stmt>, Vec<MiruError>> {
    let tokens = lexer::Lexer::tokenize(source).map_err(|error| vec![error])?;
    parser::Parser::parse(tokens)
}

/// Lex and parse a source string into a program, giving the first error only.
///
/// This is the spelling almost everything wants: the REPL reports one error and
/// asks for the next line, a file that does not parse cannot be formatted or
/// disassembled, and each golden case pins one message.
///
/// A wrapper over [`parse_program_all`] rather than a second implementation.
/// One that is literally "take the first" cannot come to disagree with what it
/// wraps, which is the argument [`run_source`] makes for delegating to
/// [`run_source_from`].
pub fn parse_program(source: &str) -> Result<Vec<ast::Stmt>, MiruError> {
    parse_program_all(source).map_err(|errors| {
        errors
            .into_iter()
            .next()
            .expect("a failed parse has at least one error")
    })
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
    // The first error only. A file that does not parse cannot be formatted at
    // all, so listing every fault would be a longer way of saying the same no.
    let program = parser::Parser::parse(tokens).map_err(|errors| {
        errors
            .into_iter()
            .next()
            .expect("a failed parse has at least one error")
    })?;
    Ok(formatter::format_program(&program, &trivia))
}

/// Lex, parse, compile, and run a source string, sending `print` output to
/// `out` and reading `input()` from standard input.
///
/// Gives the code the program stopped with: `0` unless it called `exit`.
/// Diagnostics go to standard error unless the caller redirects them with
/// [`vm::Vm::set_error_output`].
///
/// The failing side is a list because a program can hold more than one syntax
/// error and all of them are worth reporting at once. It holds exactly one for
/// every other kind of failure: a run stops at the first thing that goes wrong.
pub fn run_source(source: &str, out: Box<dyn Write>, host: Host) -> Result<i32, Vec<MiruError>> {
    run_source_from(source, None, out, host)
}

/// Everything a program is allowed to reach outside itself.
///
/// One struct rather than one parameter each, because there are three of them
/// now and a call site holding three positional boxes says nothing about which
/// is which. Each field is separate rather than one "allow everything" flag,
/// because a host really can have one and not the others: the browser
/// playground has a clock, no file system, and no keyboard.
pub struct Host {
    pub system: Box<dyn value::System>,
    pub clock: Box<dyn value::Clock>,
    pub keyboard: Box<dyn value::Keyboard>,
    pub screen: Box<dyn value::Screen>,
}

impl Host {
    /// A host that grants nothing. Every capability refuses.
    pub fn none() -> Host {
        Host {
            system: Box::new(value::NoSystem),
            clock: Box::new(value::NoClock),
            keyboard: Box::new(value::NoKeyboard),
            screen: Box::new(value::NoScreen),
        }
    }

    fn give_to(self, vm: &mut vm::Vm) {
        vm.set_system(self.system);
        vm.set_clock(self.clock);
        vm.set_keyboard(self.keyboard);
        vm.set_screen(self.screen);
    }
}

impl Default for Host {
    fn default() -> Host {
        Host::none()
    }
}

/// Like [`run_source`], but for a program that came from `path`, which is what
/// an `import` inside it resolves against.
///
/// The path-free spelling above delegates here with `None`. Keeping one
/// implementation and a one-line wrapper, rather than two entry points, is why
/// the forty-odd existing callers did not have to change: a wrapper that is
/// literally `f(source, None, out)` cannot drift from what it wraps.
///
/// `host` is what the program is allowed to reach outside itself, and it is a
/// parameter rather than a default so that granting any of it is a decision
/// somebody wrote down. [`Host::none`] grants nothing.
pub fn run_source_from(
    source: &str,
    path: Option<&std::path::Path>,
    out: Box<dyn Write>,
    host: Host,
) -> Result<i32, Vec<MiruError>> {
    let program = parse_program_all(source)?;
    let mut vm = vm::Vm::with_output(out);
    vm.set_input(Box::new(StdinInput));
    host.give_to(&mut vm);
    let result = vm.run_from(&program, path);
    // Flushed on both arms, not only on success. A program that failed still
    // printed whatever it printed before it failed, and this used to drop that
    // because the run was followed by `?`. An exit makes it matter more, since
    // an exit leaves the loop as an error and would otherwise lose the output
    // it was reporting.
    vm.flush();
    match (result, vm.exit_code()) {
        // A program that asked to stop has not failed, whatever error carried
        // it out of the dispatch loop. The code is read before the error, or a
        // plain `exit(0)` would be reported as a failure.
        (_, Some(code)) => Ok(code),
        (Ok(_), None) => Ok(0),
        (Err(error), None) => Err(vec![error]),
    }
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

    /// Give the session a clock, which `now()` reads.
    ///
    /// A setter rather than an argument to [`Session::new`], because the REPL
    /// is the only caller that has a real one and every other caller wants the
    /// default, which is no clock at all.
    pub fn set_clock(&mut self, clock: Box<dyn value::Clock>) {
        self.vm.set_clock(clock);
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

/// Everything a captured run produced.
///
/// Separate fields rather than one string, because the whole reason a program
/// has two streams is that they can be told apart. A capture that merged them
/// would make a test of `eprint` pass whether or not the builtin worked.
pub struct Capture {
    /// What `print` wrote.
    pub out: String,
    /// What `eprint` wrote.
    pub err: String,
    /// What the program stopped with: `0` unless it called `exit`.
    pub code: i32,
}

/// Run a source string and capture everything it printed. Handy for tests and
/// tooling that needs the output as a string rather than on a stream.
///
/// Gives standard output only. Use [`run_capture_all`] for the diagnostics and
/// the exit code as well. This spelling is kept because most callers want the
/// one string, and changing it would have churned every one of them for the
/// benefit of two.
pub fn run_capture(source: &str) -> Result<String, MiruError> {
    run_capture_with_input(source, &[])
}

/// Like [`run_capture`], but feeds the given lines to `input()` in order.
pub fn run_capture_with_input(source: &str, input: &[&str]) -> Result<String, MiruError> {
    run_capture_all_with_input(source, input).map(|captured| captured.out)
}

/// Run a source string and capture both streams and the exit code.
pub fn run_capture_all(source: &str) -> Result<Capture, MiruError> {
    run_capture_all_with_input(source, &[])
}

/// Like [`run_capture_all`], but feeds the given lines to `input()` in order.
pub fn run_capture_all_with_input(source: &str, input: &[&str]) -> Result<Capture, MiruError> {
    // The first error only, because every caller of this spelling asserts one
    // message. The collecting one below is what the playground uses.
    run_capture_all_with(source, input, Host::none()).map_err(|errors| {
        errors
            .into_iter()
            .next()
            .expect("a failed run has at least one error")
    })
}

/// Like [`run_capture_all_with_input`], and with a host behind it.
///
/// The spellings above keep granting nothing, which is what every golden case
/// wants: the ambient builtins are the ones whose results the source does not
/// determine, so a capture with a real clock or keyboard behind it could not
/// assert an answer. This entry point exists for the callers that do supply
/// one, which are the browser playground and the tests of the capabilities
/// themselves.
pub fn run_capture_all_with(
    source: &str,
    input: &[&str],
    host: Host,
) -> Result<Capture, Vec<MiruError>> {
    let program = parse_program_all(source)?;
    let out = Rc::new(RefCell::new(Vec::<u8>::new()));
    let err = Rc::new(RefCell::new(Vec::<u8>::new()));
    let mut vm = vm::Vm::with_output(Box::new(SharedBuffer(Rc::clone(&out))));
    vm.set_error_output(Box::new(SharedBuffer(Rc::clone(&err))));
    vm.set_input(Box::new(ScriptedInput::new(input)));
    host.give_to(&mut vm);
    let result = vm.run(&program);
    // Flushed before either arm returns, and the code read before the error is
    // reported, for the same reasons as `run_source_from`.
    vm.flush();
    let captured = |code: i32| Capture {
        out: String::from_utf8_lossy(out.borrow().as_slice()).into_owned(),
        err: String::from_utf8_lossy(err.borrow().as_slice()).into_owned(),
        code,
    };
    match (result, vm.exit_code()) {
        (_, Some(code)) => Ok(captured(code)),
        (Ok(_), None) => Ok(captured(0)),
        (Err(error), None) => Err(vec![error]),
    }
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

/// A [`value::Keyboard`] backed by a fixed list of key names.
///
/// The only way to test `read_key` without a terminal. Raw mode itself has no
/// automated test on any platform, so what this covers is everything above the
/// syscalls: that the builtin gives back what the host handed it, that it gives
/// `nil` when the keys run out, and that a program can loop on it.
pub struct ScriptedKeyboard {
    keys: std::vec::IntoIter<String>,
}

impl ScriptedKeyboard {
    pub fn new(keys: &[&str]) -> ScriptedKeyboard {
        let keys: Vec<String> = keys.iter().map(|key| key.to_string()).collect();
        ScriptedKeyboard {
            keys: keys.into_iter(),
        }
    }
}

impl value::Keyboard for ScriptedKeyboard {
    fn read_key(&mut self) -> Result<Option<String>, String> {
        Ok(self.keys.next())
    }

    /// Always ready, which is the honest answer for a scripted keyboard and not
    /// a shortcut.
    ///
    /// A read here never waits: either a key is left, or the script has run out
    /// and the read gives `None` at once. Both are "a read will not block", so
    /// this is `true` even when the script is empty — the same answer a real
    /// keyboard gives at end of input, and the reason a loop guarded by it ends
    /// instead of spinning.
    fn key_ready(&mut self) -> Result<bool, String> {
        Ok(true)
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
    /// Whether `try` is allowed to turn this into a value.
    ///
    /// Almost everything is catchable, because almost everything is a condition
    /// a program could reasonably handle. The call depth limit is not: it means
    /// recursion that does not terminate, which is a bug in the program rather
    /// than a situation to recover from, and letting a `try` swallow it would
    /// hide the one thing worth knowing.
    pub fatal: bool,
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
            fatal: false,
            file: None,
        }
    }

    /// Mark this error as one `try` may not catch. See [`MiruError::fatal`].
    pub fn as_fatal(mut self) -> MiruError {
        self.fatal = true;
        self
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

    /// A clock stopped at one instant, which is what makes a time assertable.
    ///
    /// The seam exists so the library never reads the machine's clock. That it
    /// also makes `now` testable to the millisecond is the second thing it
    /// buys, and the reason the tests of the clock are here rather than in the
    /// integration tests, which can only check a bound.
    struct FixedClock(i64);

    impl value::Clock for FixedClock {
        fn now_millis(&mut self) -> Result<i64, String> {
            Ok(self.0)
        }

        /// Moves the clock forward instead of really waiting.
        ///
        /// A fake that sleeps by advancing is better than one that records the
        /// request: a test can then assert that `now()` moved by exactly the
        /// amount asked for, which is the property that matters, and the suite
        /// stays instant.
        fn sleep_millis(&mut self, millis: u64) -> Result<(), String> {
            self.0 += millis as i64;
            Ok(())
        }
    }

    #[test]
    fn now_gives_back_exactly_what_the_host_clock_said() {
        let captured = run_capture_all_with(
            "print(now())",
            &[],
            Host {
                clock: Box::new(FixedClock(1_234_567)),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "1234567\n");
    }

    /// A clock is read at each call rather than once per run, so a program that
    /// measures a duration sees two different times. Reading it once and
    /// caching would make every interval zero.
    #[test]
    fn each_call_reads_the_clock_again() {
        struct Ticking(i64);

        impl value::Clock for Ticking {
            fn now_millis(&mut self) -> Result<i64, String> {
                self.0 += 5;
                Ok(self.0)
            }

            fn sleep_millis(&mut self, _millis: u64) -> Result<(), String> {
                Ok(())
            }
        }

        let captured = run_capture_all_with(
            "let a = now()\nlet b = now()\nprint(b - a)",
            &[],
            Host {
                clock: Box::new(Ticking(0)),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "5\n");
    }

    #[test]
    fn read_key_gives_back_what_the_host_handed_it() {
        let captured = run_capture_all_with(
            "print(read_key())\nprint(read_key())\nprint(read_key())",
            &[],
            Host {
                keyboard: Box::new(ScriptedKeyboard::new(&["up", "a"])),
                ..Host::none()
            },
        )
        .expect("the program runs");
        // The third call is past the end, which is `nil` rather than a refusal:
        // the host had a keyboard and there is nothing more to read from it.
        assert_eq!(captured.out, "up\na\nnil\n");
    }

    /// A loop over `read_key` ends at the end of input rather than spinning,
    /// which is the shape every program using it will have.
    #[test]
    fn a_loop_over_read_key_ends() {
        let captured = run_capture_all_with(
            "let seen = 0\nwhile true {\n  let k = read_key()\n  if k == nil { break }\n  \
             if k == \"ctrl+c\" { break }\n  seen = seen + 1\n}\nprint(seen)",
            &[],
            Host {
                keyboard: Box::new(ScriptedKeyboard::new(&["a", "b", "ctrl+c", "d"])),
                ..Host::none()
            },
        )
        .expect("the program runs");
        // Two, not three: `ctrl+c` breaks before the counter moves, and the
        // `d` after it is never read.
        assert_eq!(captured.out, "2\n");
    }

    /// A screen that records what it was asked to do, rather than doing it.
    struct ScriptedScreen {
        done: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl value::Screen for ScriptedScreen {
        fn clear(&mut self) -> Result<(), String> {
            self.done.borrow_mut().push("clear".to_string());
            Ok(())
        }

        fn move_to(&mut self, column: i64, row: i64) -> Result<(), String> {
            self.done
                .borrow_mut()
                .push(format!("move_to {column} {row}"));
            Ok(())
        }

        fn hide_cursor(&mut self) -> Result<(), String> {
            self.done.borrow_mut().push("hide".to_string());
            Ok(())
        }

        fn show_cursor(&mut self) -> Result<(), String> {
            self.done.borrow_mut().push("show".to_string());
            Ok(())
        }

        fn size(&mut self) -> Result<(i64, i64), String> {
            Ok((80, 24))
        }
    }

    /// Every screen builtin reaches the host, with the arguments it was given.
    #[test]
    fn the_screen_builtins_reach_the_host() {
        let done = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = run_capture_all_with(
            "hide_cursor()\nclear()\nmove_to(3, 7)\nprint(term_size())\nshow_cursor()",
            &[],
            Host {
                screen: Box::new(ScriptedScreen {
                    done: std::rc::Rc::clone(&done),
                }),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "[80, 24]\n");
        assert_eq!(
            done.borrow().as_slice(),
            ["hide", "clear", "move_to 3 7", "show"]
        );
    }

    /// **Drawing does not go into the program's output.**
    ///
    /// `clear` reaches the screen capability, never `Output::write`. Were the
    /// escape sequence written to the output stream instead, this capture would
    /// hold control codes, and so would every golden test of every program that
    /// happened to clear.
    #[test]
    fn clearing_the_screen_puts_nothing_in_the_programs_output() {
        let done = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = run_capture_all_with(
            "clear()\nprint(\"only this\")",
            &[],
            Host {
                screen: Box::new(ScriptedScreen {
                    done: std::rc::Rc::clone(&done),
                }),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "only this\n");
        assert_eq!(done.borrow().as_slice(), ["clear"]);
    }

    /// A host with no terminal refuses, rather than quietly doing nothing.
    ///
    /// The playground is that host. Doing nothing would let a program there
    /// draw every frame on top of the last one and never say why.
    #[test]
    fn the_screen_builtins_refuse_where_there_is_no_terminal() {
        for program in [
            "clear()",
            "move_to(0, 0)",
            "hide_cursor()",
            "show_cursor()",
            "term_size()",
        ] {
            let Err(error) = run_capture_all(program) else {
                panic!("{program} worked on a host with no terminal");
            };
            assert!(
                error.message.contains("there is no terminal"),
                "{program} gave {:?}",
                error.message
            );
        }
    }

    /// A drain loop takes everything pressed since the last frame, in one frame.
    ///
    /// This is the shape the wiki teaches, and the property it has that reading
    /// one key per turn does not: four keys pressed while a frame was being
    /// drawn are all handled on the next frame, rather than arriving one per
    /// frame over the four frames after that.
    #[test]
    fn a_frame_drains_every_key_pressed_since_the_last_one() {
        let captured = run_capture_all_with(
            "let seen = []\nwhile key_ready() {\n  let k = read_key()\n  \
             if k == nil { break }\n  push(seen, k)\n}\nprint(len(seen))",
            &[],
            Host {
                keyboard: Box::new(ScriptedKeyboard::new(&["a", "b", "c", "d"])),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "4\n");
    }

    /// **The property that keeps a drain loop from spinning forever.**
    ///
    /// `key_ready` reports that a read will not wait, not that a key exists, so
    /// it stays `true` once the keys have run out. The read then gives `nil`
    /// and the loop breaks. Were it `false` at the end instead, this program
    /// would still terminate — but a game's outer loop, which asks again every
    /// frame, would never learn that the input had closed.
    #[test]
    fn key_ready_stays_true_at_the_end_so_the_read_can_report_it() {
        let captured = run_capture_all_with(
            "print(key_ready())\nprint(read_key())\nprint(key_ready())\nprint(read_key())",
            &[],
            Host {
                keyboard: Box::new(ScriptedKeyboard::new(&["a"])),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "true\na\ntrue\nnil\n");
    }

    /// A host with no keyboard refuses rather than answering `false`.
    ///
    /// `false` reads as "nothing is pressed", which would send a game round its
    /// loop forever waiting for a keyboard that does not exist. The playground
    /// is that host.
    #[test]
    fn key_ready_refuses_where_there_is_no_keyboard() {
        let Err(error) = run_capture_all("key_ready()") else {
            panic!("a host with no keyboard answered whether a key was ready");
        };
        assert!(
            error.message.contains("there is no keyboard"),
            "the message was {:?}",
            error.message
        );
    }

    /// The captures that everything else uses grant no clock, which is what
    /// keeps the golden cases deterministic. Stated as a test because it is a
    /// property of the default rather than of any one case.
    #[test]
    fn the_ordinary_capture_grants_no_clock() {
        let Err(error) = run_capture_all("now()") else {
            panic!("the default capture granted a clock");
        };
        assert!(
            error.message.contains("there is no clock"),
            "the message was {:?}",
            error.message
        );
    }

    /// `sleep` reaches the host's clock with the number the program asked for.
    ///
    /// `FixedClock` sleeps by moving itself forward, so this asserts the exact
    /// duration rather than a bound, and does it without the suite waiting.
    #[test]
    fn sleep_asks_the_host_clock_to_wait_exactly_as_long() {
        let captured = run_capture_all_with(
            "let a = now()\nsleep(250)\nprint(now() - a)",
            &[],
            Host {
                clock: Box::new(FixedClock(1_000)),
                ..Host::none()
            },
        )
        .expect("the program runs");
        assert_eq!(captured.out, "250\n");
    }

    /// Sleeping where nothing can pause is a refusal, not a silent return.
    ///
    /// A page is the host this is about. Returning at once there would let a
    /// paced loop run flat out while looking as though it were paced.
    #[test]
    fn sleep_refuses_where_the_host_has_no_clock() {
        let Err(error) = run_capture_all("sleep(1)") else {
            panic!("a host with no clock agreed to wait");
        };
        assert!(
            error.message.contains("there is no clock"),
            "the message was {:?}",
            error.message
        );
    }

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
