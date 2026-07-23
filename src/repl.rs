//! The interactive read-eval-print loop for the `miru` binary.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use miruscriptx::interpreter::Interpreter;
use miruscriptx::value::Value;

/// Start the REPL. It reads a line (or a multi-line block) at a time, runs it,
/// and echoes the value of each expression. State persists across inputs, so
/// variables and functions you define stay available.
pub fn run() -> ExitCode {
    println!(
        "MiruScriptX {} REPL. Press Ctrl-D to exit.",
        miruscriptx::VERSION
    );
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut interpreter = Interpreter::new();
    let mut buffer = String::new();

    loop {
        let prompt = if buffer.is_empty() {
            "miru> "
        } else {
            "...   "
        };
        print!("{prompt}");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(err) => {
                eprintln!("miru: input error: {err}");
                return ExitCode::FAILURE;
            }
        }

        buffer.push_str(&line);
        if is_incomplete(&buffer) {
            continue; // wait for the block to be closed
        }

        let source = std::mem::take(&mut buffer);
        if source.trim().is_empty() {
            continue;
        }

        match miruscriptx::parse_program(&source) {
            Ok(program) => match interpreter.run_program(&program) {
                Ok(value) => {
                    interpreter.flush();
                    if !matches!(value, Value::Nil) {
                        println!("{}", value.repr());
                    }
                }
                Err(err) => eprintln!("{}", err.render(&source)),
            },
            Err(err) => eprintln!("{}", err.render(&source)),
        }
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
