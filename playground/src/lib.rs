//! WebAssembly bindings that run MiruScriptX in a browser.
//!
//! This crate is glue and nothing else. Every entry point forwards to a function
//! that already exists in the `miruscriptx` library, so the playground runs the
//! same lexer, compiler, and virtual machine as `miru` on a terminal, and cannot
//! drift from it.
//!
//! It is a separate crate on purpose. `wasm-bindgen` is a real dependency tree,
//! and keeping it here means the language itself still builds and ships with the
//! two direct dependencies its README advertises.
//!
//! # Errors
//!
//! A failing program is not an exception here, it is the output a user came to
//! see. Each entry point returns the rendered error, caret and call trace and
//! all, exactly as the binary prints it, paired with a flag saying which of the
//! two happened.

use miruscriptx::MiruError;
use wasm_bindgen::prelude::*;

/// The result of running, formatting, or disassembling a program.
///
/// `ok` distinguishes the two cases; `text` is the program's output when `ok`
/// and the rendered error otherwise. Keeping the error as rendered text rather
/// than a code and a message means the page shows the same caret and call trace
/// the terminal does, without reimplementing the layout in JavaScript.
#[wasm_bindgen]
pub struct Outcome {
    ok: bool,
    text: String,
}

#[wasm_bindgen]
impl Outcome {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }

    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }
}

impl Outcome {
    fn succeeded(text: String) -> Outcome {
        Outcome { ok: true, text }
    }

    fn failed(error: &MiruError, source: &str) -> Outcome {
        Outcome {
            ok: false,
            text: error.render(source),
        }
    }
}

/// Run a program and return everything it printed.
#[wasm_bindgen]
pub fn run(source: &str) -> Outcome {
    match miruscriptx::run_capture(source) {
        Ok(output) => Outcome::succeeded(output),
        Err(error) => Outcome::failed(&error, source),
    }
}

/// Reformat a program in the canonical `miru fmt` style.
#[wasm_bindgen]
pub fn format(source: &str) -> Outcome {
    match miruscriptx::format_source(source) {
        Ok(formatted) => Outcome::succeeded(formatted),
        Err(error) => Outcome::failed(&error, source),
    }
}

/// Compile a program and return its bytecode as readable assembly.
#[wasm_bindgen]
pub fn disassemble(source: &str) -> Outcome {
    match miruscriptx::disassemble_source(source) {
        Ok(listing) => Outcome::succeeded(listing),
        Err(error) => Outcome::failed(&error, source),
    }
}

/// The version of the language this playground was built from.
#[wasm_bindgen]
pub fn version() -> String {
    miruscriptx::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // These run on the host, not in a browser. What they check is that the glue
    // forwards correctly and reports failure as rendered text, which has nothing
    // to do with the target.

    #[test]
    fn running_a_program_returns_what_it_printed() {
        let outcome = run("print(\"hi\")\nprint(1 + 2)");
        assert!(outcome.ok);
        assert_eq!(outcome.text, "hi\n3\n");
    }

    #[test]
    fn a_failing_program_returns_the_rendered_error() {
        let outcome = run("fn add(a) {\n  return a + 1\n}\nadd(nil)");
        assert!(!outcome.ok);
        // The caret and the call trace both survive the boundary, which is the
        // reason the error crosses it as rendered text.
        assert!(
            outcome.text.contains("cannot add a nil and a int"),
            "{}",
            outcome.text
        );
        assert!(outcome.text.contains('^'), "{}", outcome.text);
        assert!(
            outcome.text.contains("in add, called from line 4"),
            "{}",
            outcome.text
        );
    }

    #[test]
    fn formatting_and_disassembling_forward_to_the_language() {
        let formatted = format("let  x=[1,2]");
        assert!(formatted.ok);
        assert_eq!(formatted.text, "let x = [1, 2]\n");

        let listing = disassemble("1 + 2");
        assert!(listing.ok);
        assert!(listing.text.contains("== script =="), "{}", listing.text);
    }

    #[test]
    fn a_syntax_error_is_reported_by_every_entry_point() {
        for outcome in [run("let = 1"), format("let = 1"), disassemble("let = 1")] {
            assert!(!outcome.ok);
            assert!(
                outcome.text.contains("expected an identifier"),
                "{}",
                outcome.text
            );
        }
    }
}
