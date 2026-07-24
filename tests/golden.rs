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
fn arithmetic_and_numeric_promotion() {
    check_all(&[
        ("1 + 2 * 3", "ok 7"),
        ("(1 + 2) * 3", "ok 9"),
        ("10 - 4 - 3", "ok 3"),
        ("2 * (3 + 4)", "ok 14"),
        ("20 / 4 / 5", "ok 1"),
        // Integer division and modulo truncate toward zero.
        ("7 / 2", "ok 3"),
        ("7 % 3", "ok 1"),
        ("-7 % 3", "ok -1"),
        // A float operand promotes the whole expression.
        ("7.0 / 2.0", "ok 3.5"),
        ("1 + 2.5", "ok 3.5"),
        ("2.5 + 1", "ok 3.5"),
        ("1.5 * 2", "ok 3.0"),
        ("-5 + 3", "ok -2"),
        ("- -8", "ok 8"),
        ("-(2 + 3)", "ok -5"),
    ]);
}

#[test]
fn comparison_and_equality() {
    check_all(&[
        ("1 < 2", "ok true"),
        ("2 < 1", "ok false"),
        ("2 <= 2", "ok true"),
        ("3 > 4", "ok false"),
        ("5 >= 5", "ok true"),
        ("1 == 1", "ok true"),
        // Equality promotes across int and float.
        ("1 == 1.0", "ok true"),
        ("2 != 3", "ok true"),
        ("\"a\" == \"a\"", "ok true"),
        ("\"a\" != \"b\"", "ok true"),
        ("nil == nil", "ok true"),
        ("true == true", "ok true"),
        // Strings order lexicographically.
        ("\"a\" < \"b\"", "ok true"),
        ("\"b\" >= \"a\"", "ok true"),
    ]);
}

#[test]
fn logical_operators_yield_bools_and_short_circuit() {
    check_all(&[
        ("true && true", "ok true"),
        ("true && false", "ok false"),
        ("false && true", "ok false"),
        ("false || false", "ok false"),
        ("true || false", "ok true"),
        // Only false and nil are falsy, and the result is always a bool
        // rather than the operand itself.
        ("1 && 2", "ok true"),
        ("0 && 1", "ok true"),
        ("nil || 5", "ok true"),
        ("1 < 2 && 3 < 4", "ok true"),
        ("!(true && false)", "ok true"),
        // The skipped side is never evaluated, so its error never happens.
        ("false && (1 / 0 == 0)", "ok false"),
        ("true || (1 / 0 == 0)", "ok true"),
        // The taken side is evaluated, so its error does.
        ("true && (1 / 0 == 0)", "err division by zero @ 1:12"),
    ]);
}

#[test]
fn strings_concatenate_with_plus() {
    check_all(&[("\"a\" + \"b\"", "ok \"ab\""), ("\"\" + \"x\"", "ok \"x\"")]);
}

#[test]
fn arithmetic_errors_report_their_position() {
    check_all(&[
        ("1 / 0", "err division by zero @ 1:3"),
        ("1 % 0", "err modulo by zero @ 1:3"),
        ("1.0 / 0.0", "err division by zero @ 1:5"),
        (
            "9223372036854775807 + 1",
            "err integer overflow in addition @ 1:21",
        ),
        (
            "-9223372036854775807 - 2",
            "err integer overflow in subtraction @ 1:22",
        ),
        (
            "9223372036854775807 * 2",
            "err integer overflow in multiplication @ 1:21",
        ),
    ]);
}

#[test]
fn type_errors_name_both_types() {
    check_all(&[
        ("1 + true", "err cannot add a int and a bool @ 1:3"),
        ("1 + nil", "err cannot add a int and a nil @ 1:3"),
        ("\"a\" + 1", "err cannot add a string and a int @ 1:5"),
        ("-nil", "err cannot negate a nil @ 1:1"),
        ("-\"a\"", "err cannot negate a string @ 1:1"),
        ("1 < \"a\"", "err cannot compare a int and a string @ 1:3"),
        ("nil < 1", "err cannot compare a nil and a int @ 1:5"),
        ("true > false", "err cannot compare a bool and a bool @ 1:6"),
    ]);
}

#[test]
fn globals_are_declared_read_and_assigned() {
    check_all(&[
        ("let x = 5\nx + 1", "ok 6"),
        ("let x = 5\nlet y = 10\nx * y", "ok 50"),
        ("let x = 1\nx = x + 1\nx", "ok 2"),
        ("let name = \"Aiko\"\n\"Hi \" + name", "ok \"Hi Aiko\""),
        // A let statement is not an expression, so a trailing one yields nil.
        ("let x = 5", "ok nil"),
        // Re-declaring overwrites.
        ("let a = 2\nlet a = 3\na", "ok 3"),
        ("missing", "err undefined variable 'missing' @ 1:1"),
        (
            "missing = 5",
            "err cannot assign to undefined variable 'missing' @ 1:1",
        ),
    ]);
}

#[test]
fn if_else_chains_pick_one_branch() {
    check_all(&[
        ("let x = 0\nif true { x = 1 }\nx", "ok 1"),
        ("let x = 0\nif false { x = 1 }\nx", "ok 0"),
        ("let x = 0\nif false { x = 1 } else { x = 2 }\nx", "ok 2"),
        (
            "let x = 5\nlet r = 0\nif x > 10 { r = 1 } else if x > 3 { r = 2 } else { r = 3 }\nr",
            "ok 2",
        ),
        ("let x = 3\nif x > 0 { if x > 2 { x = 100 } }\nx", "ok 100"),
        // Truthiness: nil is falsy, but 0 is not.
        ("let r = 0\nif nil { r = 1 } else { r = 2 }\nr", "ok 2"),
        ("let r = 0\nif 0 { r = 1 } else { r = 2 }\nr", "ok 1"),
    ]);
}

#[test]
fn locals_are_scoped_to_their_block() {
    check_all(&[
        // A block-local declaration does not leak out.
        ("let x = 1\nif true { let x = 2 }\nx", "ok 1"),
        (
            "let result = 0\nif true {\n  let a = 10\n  let b = 20\n  result = a + b\n}\nresult",
            "ok 30",
        ),
        // A local shadows an outer name inside the block only.
        (
            "let n = 5\nlet out = 0\nif n > 0 {\n  let n = 100\n  out = n\n}\nout",
            "ok 100",
        ),
        // A right-hand reference resolves to the outer binding.
        (
            "let out = 0\nif true {\n  let a = 3\n  let a = a + 1\n  out = a\n}\nout",
            "ok 4",
        ),
        (
            "let total = 0\nif true {\n  let a = 1\n  if true {\n    let b = 2\n    total = a + b\n  }\n}\ntotal",
            "ok 3",
        ),
        (
            "let out = 0\nif true {\n  let c = 1\n  c = c + 5\n  out = c\n}\nout",
            "ok 6",
        ),
    ]);
}

#[test]
fn while_loops_and_loop_control() {
    check_all(&[
        (
            "let i = 0\nlet sum = 0\nwhile i < 5 { sum = sum + i\ni = i + 1 }\nsum",
            "ok 10",
        ),
        ("let i = 0\nwhile i < 3 { i = i + 1 }\ni", "ok 3"),
        ("let i = 0\nwhile false { i = 1 }\ni", "ok 0"),
        // A local declared fresh on each iteration.
        (
            "let i = 0\nlet sum = 0\nwhile i < 4 {\n  let step = i * 2\n  sum = sum + step\n  i = i + 1\n}\nsum",
            "ok 12",
        ),
        (
            "let i = 0\nwhile true {\n  if i == 3 { break }\n  i = i + 1\n}\ni",
            "ok 3",
        ),
        (
            "let i = 0\nlet sum = 0\nwhile i < 6 {\n  i = i + 1\n  if i % 2 == 0 { continue }\n  sum = sum + i\n}\nsum",
            "ok 9",
        ),
        // break out of a loop that declared a local, leaving the stack balanced.
        (
            "let i = 0\nlet last = 0\nwhile i < 10 {\n  let doubled = i * 2\n  last = doubled\n  if i == 4 { break }\n  i = i + 1\n}\nlast",
            "ok 8",
        ),
    ]);
}

#[test]
fn for_in_loops_over_arrays() {
    check_all(&[
        ("let sum = 0\nfor x in [1, 2, 3, 4] { sum = sum + x }\nsum", "ok 10"),
        (
            "let s = \"\"\nfor c in [\"a\", \"b\", \"c\"] { s = s + c }\ns",
            "ok \"abc\"",
        ),
        // The loop variable is fresh per iteration and does not leak out.
        ("let i = 99\nfor i in [1, 2, 3] { }\ni", "ok 99"),
        ("let sum = 0\nfor x in [] { sum = 1 }\nsum", "ok 0"),
        (
            "let sum = 0\nfor x in [1, 2, 3, 4, 5] {\n  if x == 4 { break }\n  sum = sum + x\n}\nsum",
            "ok 6",
        ),
        (
            "let sum = 0\nfor x in [1, 2, 3, 4] {\n  if x % 2 == 0 { continue }\n  sum = sum + x\n}\nsum",
            "ok 4",
        ),
        (
            "let sum = 0\nfor x in [1, 2, 3] {\n  let sq = x * x\n  sum = sum + sq\n}\nsum",
            "ok 14",
        ),
        ("let sum = 0\nfor x in range(4) { sum = sum + x }\nsum", "ok 6"),
        (
            "let n = 0\nfor i in [1,2] { for j in [1,2,3] { n = n + 1 } }\nn",
            "ok 6",
        ),
        // Only arrays are iterable, including not strings.
        ("for x in 5 { }", "err cannot iterate over a int @ 1:10"),
        ("for x in \"ab\" { }", "err cannot iterate over a string @ 1:10"),
    ]);
}

#[test]
fn a_program_evaluates_to_its_last_expression() {
    check_all(&[("1\n2\n3", "ok 3"), ("1 + 1\n2 + 2", "ok 4")]);
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
