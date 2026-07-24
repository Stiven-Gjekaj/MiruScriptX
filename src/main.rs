//! The `miru` command line interface: run a file or start the REPL.

mod repl;

use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  miru run <file.miru>      Run a MiruScriptX program from a file
  miru fmt <file.miru>      Format a program and print it to standard output
  miru fmt -w <file.miru>   Format a program and rewrite the file in place
  miru                      Start the interactive REPL
  miru repl                 Start the interactive REPL
  miru --version            Print the version and exit
  miru --help               Show this help and exit";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        None => repl::run(),
        Some((command, rest)) => match command.as_str() {
            "run" => run_file(rest),
            "fmt" => fmt_file(rest),
            "repl" => {
                if rest.is_empty() {
                    repl::run()
                } else {
                    usage_error("the 'repl' command takes no arguments")
                }
            }
            "--version" | "-v" => {
                println!("miru {}", miruscriptx::VERSION);
                ExitCode::SUCCESS
            }
            "--help" | "-h" => {
                println!("MiruScriptX {}", miruscriptx::VERSION);
                println!();
                println!("{USAGE}");
                ExitCode::SUCCESS
            }
            other => usage_error(&format!("unknown command '{other}'")),
        },
    }
}

fn run_file(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    for arg in args {
        match arg.as_str() {
            // Accepted and ignored: v0.4 offered --vm to opt into the bytecode
            // engine, which is now the only one, so commands written then keep
            // working rather than failing on an unknown option.
            "--vm" => {}
            other if other.starts_with("--") => {
                return usage_error(&format!("unknown option '{other}' for 'run'"));
            }
            other => {
                if path.is_some() {
                    return usage_error("the 'run' command takes a single file path");
                }
                path = Some(other);
            }
        }
    }

    let Some(path) = path else {
        return usage_error("the 'run' command needs a file path");
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("miru: cannot read '{path}': {err}");
            return ExitCode::FAILURE;
        }
    };

    let out = Box::new(std::io::stdout());
    match miruscriptx::run_source(&source, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("miru: {}", err.render(&source));
            ExitCode::FAILURE
        }
    }
}

fn fmt_file(args: &[String]) -> ExitCode {
    let mut write = false;
    let mut path: Option<&str> = None;
    for arg in args {
        match arg.as_str() {
            "--write" | "-w" => write = true,
            other if other.starts_with('-') => {
                return usage_error(&format!("unknown option '{other}' for 'fmt'"));
            }
            other => {
                if path.is_some() {
                    return usage_error("the 'fmt' command takes a single file path");
                }
                path = Some(other);
            }
        }
    }

    let Some(path) = path else {
        return usage_error("the 'fmt' command needs a file path");
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("miru: cannot read '{path}': {err}");
            return ExitCode::FAILURE;
        }
    };

    let formatted = match miruscriptx::format_source(&source) {
        Ok(formatted) => formatted,
        Err(err) => {
            eprintln!("miru: {}", err.render(&source));
            return ExitCode::FAILURE;
        }
    };

    if write {
        match std::fs::write(path, &formatted) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("miru: cannot write '{path}': {err}");
                ExitCode::FAILURE
            }
        }
    } else {
        print!("{formatted}");
        ExitCode::SUCCESS
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("miru: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}
