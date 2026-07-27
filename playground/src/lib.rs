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

use miruscriptx::globals::Globals;
use miruscriptx::lexer::Lexer;
use miruscriptx::token::TokenKind;
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

/// The example programs shipped with the language, as `(name, source)` pairs.
///
/// Inlined at build time with `include_str!` rather than fetched, so the page
/// makes no network request after the module loads and cannot show an example
/// that has drifted from the one in the repository. These are the same files
/// `tests/integration.rs` runs through the real binary.
///
/// `greeter.miru` is deliberately absent: it calls `input()`, and the
/// playground has nowhere to read a line from.
const EXAMPLES: &[(&str, &str)] = &[
    ("greet", include_str!("../../examples/greet.miru")),
    ("fib", include_str!("../../examples/fib.miru")),
    ("fizzbuzz", include_str!("../../examples/fizzbuzz.miru")),
    ("contacts", include_str!("../../examples/contacts.miru")),
    ("transform", include_str!("../../examples/transform.miru")),
];

/// The names of the bundled examples, in the order they should be offered.
#[wasm_bindgen]
pub fn example_names() -> Vec<String> {
    EXAMPLES.iter().map(|(name, _)| name.to_string()).collect()
}

/// The source of one bundled example, or an empty string if there is no such
/// example.
#[wasm_bindgen]
pub fn example_source(name: &str) -> String {
    EXAMPLES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, source)| source.to_string())
        .unwrap_or_default()
}

/// A run of source text to colour: where it starts, how long it is, and what
/// kind of thing it is.
///
/// Offsets are **char** indices, not UTF-16 units, because that is what the
/// lexer counts in. JavaScript must walk code points (`Array.from`) rather than
/// index the string directly, or every span after a multi-byte character lands
/// in the wrong place.
#[wasm_bindgen]
pub struct Highlight {
    start: usize,
    len: usize,
    class: String,
}

#[wasm_bindgen]
impl Highlight {
    #[wasm_bindgen(getter)]
    pub fn start(&self) -> usize {
        self.start
    }

    /// How many characters the span covers.
    ///
    /// Named `length` rather than `len` because it is a width, not the size of
    /// a collection, so the pairing with `is_empty` that `len` implies would be
    /// meaningless here. It also reads as ordinary JavaScript on the other side.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.len
    }

    #[wasm_bindgen(getter)]
    pub fn class(&self) -> String {
        self.class.clone()
    }
}

/// Whether a name is one of the language's builtins.
///
/// Answered by populating a real global table with the real registration
/// function and asking it, so the set can never drift from the one a program
/// actually sees. Building the table costs a few dozen map inserts, which is
/// nothing against repainting an editor.
fn is_builtin(name: &str) -> bool {
    let mut globals = Globals::new();
    miruscriptx::builtins::register(&mut globals);
    globals.contains(name)
}

/// Classify a token for colouring.
///
/// This lives here rather than in the language crate because which things share
/// a colour is a presentation question, not a property of the grammar. What the
/// language owns is the token kinds; the playground decides that `fn` and `if`
/// look alike and that `+` and `,` do not.
fn class_of(kind: &TokenKind) -> Option<&'static str> {
    let class = match kind {
        TokenKind::Int(_) | TokenKind::Float(_) => "number",
        TokenKind::Str(_) => "string",
        TokenKind::Fn
        | TokenKind::Let
        | TokenKind::Return
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::While
        | TokenKind::For
        | TokenKind::In
        | TokenKind::Break
        | TokenKind::Continue => "keyword",
        TokenKind::True | TokenKind::False | TokenKind::Nil => "literal",
        TokenKind::Ident(name) if is_builtin(name) => "builtin",
        // Identifiers, operators, delimiters, newlines, and end of input are
        // left as ordinary text. Colouring punctuation makes code busier to
        // read rather than clearer.
        _ => return None,
    };
    Some(class)
}

/// The spans to colour in a source string, in order.
///
/// A program that does not lex yields nothing rather than an error: someone
/// typing has an unterminated string most of the time, and the editor should
/// simply stop colouring until it is closed rather than flash a complaint.
#[wasm_bindgen]
pub fn highlight(source: &str) -> Vec<Highlight> {
    let Ok((tokens, spans)) = Lexer::tokenize_with_spans(source) else {
        return Vec::new();
    };

    // A name after a dot is a field, not a variable, so it must not be coloured
    // as a builtin however it is spelled. `c.len` names a field of `c`; `len(c)`
    // calls the builtin. `class_of` sees one token at a time and cannot tell
    // them apart, so the dot is tracked here.
    let mut after_dot = false;

    let mut out: Vec<Highlight> = tokens
        .iter()
        .zip(spans.tokens.iter())
        .filter(|(_, (_, len))| *len > 0)
        .filter_map(|(token, (start, len))| {
            let is_field = after_dot;
            after_dot = token.kind == TokenKind::Dot;
            if is_field {
                return None;
            }
            class_of(&token.kind).map(|class| Highlight {
                start: *start,
                len: *len,
                class: class.to_string(),
            })
        })
        .chain(spans.comments.iter().map(|(start, len)| Highlight {
            start: *start,
            len: *len,
            class: "comment".to_string(),
        }))
        .collect();

    // Comments are appended after the tokens, so the whole list needs sorting
    // before a consumer can walk it and the source together.
    out.sort_by_key(|span| span.start);
    out
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
    fn an_underline_survives_the_boundary() {
        // The page shows whatever the terminal shows because the error crosses
        // as rendered text, so a widened underline needs no work on this side.
        // That is worth an assertion rather than an assumption: it is exactly
        // the kind of thing a future envelope of code and message would drop
        // without any test noticing.
        let outcome = run("let total = 1\nprint(missing)");
        assert!(!outcome.ok);
        assert!(
            outcome
                .text
                .contains("\n    print(missing)\n          ^^^^^^^"),
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
    fn every_bundled_example_runs() {
        // The dropdown must not be able to offer a program that fails, and an
        // example that calls input() would, since the page has no stdin.
        for name in example_names() {
            let source = example_source(&name);
            assert!(!source.is_empty(), "{name} has no source");
            let outcome = run(&source);
            assert!(outcome.ok, "{name} failed:\n{}", outcome.text);
        }
    }

    #[test]
    fn an_unknown_example_yields_nothing_rather_than_panicking() {
        assert_eq!(example_source("nope"), "");
    }

    #[test]
    fn highlighting_classifies_the_parts_of_a_program() {
        let source = "// note\nlet n = 1\nprint(\"hi\")\nif true { n = n + 1 }";
        let chars: Vec<char> = source.chars().collect();
        let spans = highlight(source);

        let classified: Vec<(String, String)> = spans
            .iter()
            .map(|s| {
                (
                    s.class.clone(),
                    chars[s.start..s.start + s.len].iter().collect::<String>(),
                )
            })
            .collect();

        for expected in [
            ("comment", "// note"),
            ("keyword", "let"),
            ("number", "1"),
            ("builtin", "print"),
            ("string", "\"hi\""),
            ("keyword", "if"),
            ("literal", "true"),
        ] {
            let pair = (expected.0.to_string(), expected.1.to_string());
            assert!(
                classified.contains(&pair),
                "missing {expected:?} in {classified:?}"
            );
        }

        // A user's own name is left as ordinary text, and so is punctuation.
        assert!(
            !classified
                .iter()
                .any(|(_, text)| text == "n" || text == "+"),
            "{classified:?}"
        );
    }

    #[test]
    fn a_field_named_like_a_builtin_is_not_coloured_as_one() {
        // `class_of` sees one token at a time, so a field spelled like a builtin
        // would be coloured as one. Field access made that reachable: `c.len` is
        // a field of `c` and `len(c)` is the builtin, and only the dot tells
        // them apart.
        let spans = highlight("let c = {}\nc.len\nlen(c)");
        let builtins: Vec<usize> = spans
            .iter()
            .filter(|h| h.class == "builtin")
            .map(|h| h.start)
            .collect();
        // One "builtin", and it is the call on the third line rather than the
        // field on the second.
        assert_eq!(builtins.len(), 1);
        assert_eq!(
            "let c = {}\nc.len\nlen(c)"
                .chars()
                .skip(builtins[0])
                .take(3)
                .collect::<String>(),
            "len"
        );
        assert!(builtins[0] > "let c = {}\nc.len".chars().count());
    }

    #[test]
    fn highlighting_is_ordered_and_never_overlaps() {
        // The page walks the spans and the source together, so out-of-order or
        // overlapping spans would corrupt the rendered text rather than just
        // mis-colour it.
        let source = "fn f(a) {\n  // c\n  return \"x\" + a\n}\n";
        let mut end = 0;
        for span in highlight(source) {
            assert!(span.start >= end, "span at {} overlaps {end}", span.start);
            end = span.start + span.len;
        }
        assert!(end <= source.chars().count());
    }

    #[test]
    fn a_program_that_does_not_lex_highlights_as_nothing() {
        // Half-typed input is the common case in an editor, not an error worth
        // reporting.
        assert!(highlight("let s = \"unterminated").is_empty());
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
