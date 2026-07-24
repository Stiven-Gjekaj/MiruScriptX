//! The interactive read-eval-print loop for the `miru` binary.

use std::path::PathBuf;
use std::process::ExitCode;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use miruscriptx::value::Value;
use miruscriptx::Session;

/// Start the REPL. It reads a line (or a multi-line block) at a time, runs it,
/// and echoes the value of each expression. State persists across inputs, so
/// variables and functions you define stay available. Line editing and history
/// come from rustyline.
pub fn run() -> ExitCode {
    println!(
        "MiruScriptX {} REPL. Press Ctrl-D to exit.",
        miruscriptx::VERSION
    );

    let mut editor = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(err) => {
            eprintln!("miru: could not start the REPL: {err}");
            return ExitCode::FAILURE;
        }
    };

    let history = history_path();
    if let Some(path) = &history {
        // A missing history file on the first run is fine.
        let _ = editor.load_history(path);
    }

    let mut session = Session::new();
    let mut buffer = String::new();

    let exit_code = loop {
        let prompt = if buffer.is_empty() {
            "miru> "
        } else {
            "...   "
        };
        match editor.readline(prompt) {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');
                if is_incomplete(&buffer) {
                    continue; // wait for the block to be closed
                }
                let source = std::mem::take(&mut buffer);
                if source.trim().is_empty() {
                    continue;
                }
                // Record the input so the up arrow recalls it, this session and
                // future ones.
                let _ = editor.add_history_entry(source.trim_end());
                evaluate(&mut session, &source);
            }
            // Ctrl-C discards the current (possibly multi-line) input.
            Err(ReadlineError::Interrupted) => buffer.clear(),
            // Ctrl-D exits.
            Err(ReadlineError::Eof) => break ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("miru: input error: {err}");
                break ExitCode::FAILURE;
            }
        }
    };

    if let Some(path) = &history {
        let _ = editor.save_history(path);
    }
    exit_code
}

/// The path to the REPL history file, `~/.miru_history` (or the Windows home
/// equivalent). `None` when no home directory is known.
fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".miru_history"))
}

/// Run one complete input, echoing the value it produced or reporting the error
/// it raised. Either way the session stays usable for the next input.
fn evaluate(session: &mut Session, source: &str) {
    match session.eval(source) {
        Ok(value) => {
            // A statement such as `let x = 1` yields nil, and echoing that on
            // every input would be noise.
            if !matches!(value, Value::Nil) {
                println!("{}", value.repr());
            }
        }
        Err(err) => eprintln!("{}", err.render(source)),
    }
}

/// Returns true when the buffered input still has unclosed brackets, so the
/// REPL should keep reading. Brackets inside strings and line comments do not
/// count.
fn is_incomplete(source: &str) -> bool {
    let mut depth: i32 = 0;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                while let Some(string_char) = chars.next() {
                    match string_char {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

#[cfg(test)]
mod tests {
    use super::is_incomplete;

    #[test]
    fn complete_input_is_not_incomplete() {
        assert!(!is_incomplete("let x = 1\n"));
        assert!(!is_incomplete("print(1)\n"));
    }

    #[test]
    fn unclosed_brackets_are_incomplete() {
        assert!(is_incomplete("fn f() {\n"));
        assert!(is_incomplete("[1, 2,\n"));
    }

    #[test]
    fn brackets_in_strings_and_comments_do_not_count() {
        assert!(!is_incomplete("let s = \"{[(\"\n"));
        assert!(!is_incomplete("print(1) // a ( comment\n"));
    }
}
