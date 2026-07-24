//! Golden tests: a corpus of programs, each paired with the exact result it must
//! produce.
//!
//! Through v0.4 the language was verified by running every program on both
//! engines and requiring them to agree. That check is only as permanent as the
//! second engine, and v0.5 retires the tree walker. These tests take its place:
//! the behavior the two engines agreed on is written down here as literal
//! expectations, so it keeps being enforced against the one remaining engine,
//! including while its hot paths are rewritten for speed.
//!
//! Expectations are literals on purpose. A test that regenerates its own
//! expected value cannot fail, and so cannot catch a regression. When a change
//! is meant to alter behavior, the literal is edited by hand and the diff shows
//! exactly what changed.

/// The outcome of running a program, as a single comparable line: either the
/// value it evaluated to (in inspect form) or the error it raised, with the
/// position that error points at.
fn outcome(source: &str) -> String {
    match miruscriptx::eval_source_vm(source) {
        Ok(value) => format!("ok {}", value.repr()),
        Err(error) => format!("err {} @ {}:{}", error.message, error.line, error.column),
    }
}

/// Check a corpus of `(source, expected outcome)` pairs. Every mismatch is
/// collected before failing, so one run reports every regression rather than
/// stopping at the first.
fn check_all(corpus: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in corpus {
        let actual = outcome(source);
        if actual != *expected {
            failures.push(format!(
                "  source:   {source:?}\n  expected: {expected}\n  actual:   {actual}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} golden cases failed:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n\n")
    );
}

/// Check a corpus of `(source, expected printed output)` pairs, for programs
/// whose behavior is what they print rather than what they evaluate to.
fn check_output(corpus: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in corpus {
        let actual = match miruscriptx::run_capture_vm(source) {
            Ok(output) => output,
            Err(error) => format!("<error: {}>", error.message),
        };
        if actual != *expected {
            failures.push(format!(
                "  source:   {source:?}\n  expected: {expected:?}\n  actual:   {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} golden output cases failed:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n\n")
    );
}

#[test]
fn literals_and_their_inspect_forms() {
    check_all(&[
        ("1", "ok 1"),
        ("0", "ok 0"),
        ("-7", "ok -7"),
        ("3.5", "ok 3.5"),
        ("2.0", "ok 2.0"),
        ("true", "ok true"),
        ("false", "ok false"),
        ("nil", "ok nil"),
        ("\"hi\"", "ok \"hi\""),
        ("\"\"", "ok \"\""),
        ("\"a\\nb\"", "ok \"a\\nb\""),
        ("\"tab\\there\"", "ok \"tab\\there\""),
        ("\"quote\\\"inside\"", "ok \"quote\\\"inside\""),
        ("9223372036854775807", "ok 9223372036854775807"),
    ]);
}

#[test]
fn printing_writes_display_forms() {
    check_output(&[
        ("print(1)", "1\n"),
        ("print(\"hi\")", "hi\n"),
        ("print(1, \"two\", true)", "1 two true\n"),
        ("print(nil)", "nil\n"),
        ("print(2.0)", "2.0\n"),
        ("print([1, 2])", "[1, 2]\n"),
        ("print({\"a\": 1})", "{\"a\": 1}\n"),
        ("print()", "\n"),
        ("print(1)\nprint(2)", "1\n2\n"),
    ]);
}
