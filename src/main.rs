//! The `miru` command line interface: run a file or start the REPL.

mod repl;

use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  miru run <file.miru>      Run a MiruScriptX program from a file
  miru -e <program>         Evaluate a program supplied on the command line
  miru --eval <program>     Evaluate a program supplied on the command line
  miru fmt <file.miru>      Format a program and print it to standard output
  miru fmt -w <file.miru>   Format a program and rewrite the file in place
  miru disasm <file.miru>   Show the bytecode a program compiles to
  miru                      Start the interactive REPL
  miru repl                 Start the interactive REPL
  miru --version            Print the version and exit
  miru --help               Show this help and exit";

/// The stack the interpreter runs on.
///
/// The interpreter walks its own structures by recursion. The parser descends
/// through nested source, and the compiler, the formatter, and the destructor
/// each walk the tree it built. So how deeply a program may nest is decided by
/// how much stack this program has, and the default is not the same everywhere:
/// a main thread usually gets 8 MiB, a spawned thread 2 MiB, and neither number
/// is promised.
///
/// Choosing the stack here turns that around. The limit in [`miruscriptx::
/// parser::Parser::MAX_NESTING`] is then set from a figure this program
/// controls, rather than inherited from whatever the operating system happened
/// to hand out. A language should not have a different grammar on a thread.
const STACK_SIZE: usize = 64 * 1024 * 1024;

fn main() -> ExitCode {
    match std::thread::Builder::new()
        .name("miru".to_string())
        .stack_size(STACK_SIZE)
        .spawn(dispatch)
    {
        Ok(handle) => match handle.join() {
            Ok(code) => code,
            // The thread panicked. Its message has already gone to stderr
            // through the default hook, so there is nothing to add here.
            Err(_) => ExitCode::FAILURE,
        },
        // A system that refuses to start a thread can still run a program, with
        // whatever stack the main thread has. Less room is a better answer than
        // not running at all.
        Err(_) => dispatch(),
    }
}

fn dispatch() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        None => repl::run(),
        Some((command, rest)) => match command.as_str() {
            "run" => run_file(rest),
            "-e" | "--eval" => eval_source(rest),
            "fmt" => fmt_file(rest),
            "disasm" => disasm_file(rest),
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

/// The real file system, which only this program hands out.
///
/// The library grants nothing by default, so a program run through
/// `miruscriptx` as a crate, or in the playground, cannot touch a file unless
/// its host says so. This is that host saying so.
///
/// A relative path resolves against the working directory, which is what
/// `std::fs` already does and what every other command line tool does.
/// `import` is the exception rather than this: a module is part of the program
/// and is found beside it, while a data file is wherever the person running the
/// program is standing. Section 8 of the specification states both rules
/// together, because meeting the second one by surprise is the way to be
/// confused by it.
struct RealSystem;

impl miruscriptx::value::System for RealSystem {
    fn read_file(&mut self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|err| format!("cannot read '{path}': {err}"))
    }

    fn write_file(&mut self, path: &str, contents: &str) -> Result<(), String> {
        std::fs::write(path, contents).map_err(|err| format!("cannot write '{path}': {err}"))
    }

    fn file_exists(&mut self, path: &str) -> bool {
        std::path::Path::new(path).is_file()
    }

    fn arguments(&self) -> Vec<String> {
        Vec::new()
    }
}

fn eval_source(args: &[String]) -> ExitCode {
    let source = match args {
        [source] => source,
        [] => return usage_error("the '-e' command needs a program"),
        _ => return usage_error("the '-e' command takes a single program"),
    };

    match miruscriptx::run_source(source, Box::new(std::io::stdout()), Box::new(RealSystem)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("miru: {}", err.render(source));
            ExitCode::FAILURE
        }
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
    let system = Box::new(RealSystem);
    match miruscriptx::run_source_from(&source, Some(std::path::Path::new(path)), out, system) {
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

fn disasm_file(args: &[String]) -> ExitCode {
    let path = match args {
        [path] if !path.starts_with('-') => path,
        [] => return usage_error("the 'disasm' command needs a file path"),
        [other] => return usage_error(&format!("unknown option '{other}' for 'disasm'")),
        _ => return usage_error("the 'disasm' command takes a single file path"),
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("miru: cannot read '{path}': {err}");
            return ExitCode::FAILURE;
        }
    };

    match miruscriptx::disassemble_source(&source) {
        Ok(listing) => {
            print!("{listing}");
            ExitCode::SUCCESS
        }
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
