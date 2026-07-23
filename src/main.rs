//! The `miru` command line interface: run a file or start the REPL.

mod repl;

use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  miru run <file.miru>   Run a MiruScriptX program from a file
  miru                  Start the interactive REPL
  miru repl             Start the interactive REPL
  miru --version        Print the version and exit
  miru --help           Show this help and exit";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        None => repl::run(),
        Some((command, rest)) => match command.as_str() {
            "run" => run_file(rest),
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
    let path = match args {
        [path] => path,
        [] => return usage_error("the 'run' command needs a file path"),
        _ => return usage_error("the 'run' command takes a single file path"),
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("miru: cannot read '{path}': {err}");
            return ExitCode::FAILURE;
        }
    };

    match miruscriptx::run_source(&source, Box::new(std::io::stdout())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("miru: {}", err.render(&source));
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("miru: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}
