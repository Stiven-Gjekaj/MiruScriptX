//! The interactive read-eval-print loop for the `miru` binary.

use std::process::ExitCode;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use miruscriptx::interpreter::Interpreter;
use miruscriptx::value::Value;

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

    let mut interpreter = Interpreter::new();
    let mut buffer = String::new();

    loop {
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
                // Record the input so the up arrow recalls it this session.
                let _ = editor.add_history_entry(source.trim_end());
                evaluate(&mut interpreter, &source);
            }
            // Ctrl-C discards the current (possibly multi-line) input.
            Err(ReadlineError::Interrupted) => buffer.clear(),
            // Ctrl-D exits.
            Err(ReadlineError::Eof) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("miru: input error: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
}

/// Parse and run one complete input, printing the result value or the error.
fn evaluate(interpreter: &mut Interpreter, source: &str) {
    match miruscriptx::parse_program(source) {
        Ok(program) => match interpreter.run_program(&program) {
            Ok(value) => {
                interpreter.flush();
                if !matches!(value, Value::Nil) {
                    println!("{}", value.repr());
                }
            }
            Err(err) => eprintln!("{}", err.render(source)),
        },
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
