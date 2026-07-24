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

/// Check the fully rendered form of an error: the header line, the offending
/// source line, and the caret beneath it. This is what a user actually sees on
/// standard error, so it is worth pinning exactly rather than only checking the
/// message and position separately.
fn check_rendered(corpus: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in corpus {
        let actual = match miruscriptx::parse_program(source) {
            Err(error) => error.render(source),
            Ok(_) => match miruscriptx::eval_source_vm(source) {
                Ok(_) => "<no error>".to_string(),
                Err(error) => error.render(source),
            },
        };
        if actual != *expected {
            failures.push(format!(
                "  source:   {source:?}\n  expected: {expected:?}\n  actual:   {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} rendered-error cases failed:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n\n")
    );
}

#[test]
fn runtime_errors_render_with_a_caret() {
    check_rendered(&[
        (
            "let a = 1\nprint(b)",
            "error (line 2, column 7): undefined variable 'b'\n    print(b)\n          ^",
        ),
        (
            "1 / 0",
            "error (line 1, column 3): division by zero\n    1 / 0\n      ^",
        ),
        (
            "let x = [1, 2]\nx[9]",
            "error (line 2, column 3): index 9 is out of range for an array of length 2\n    x[9]\n      ^",
        ),
        // The caret sits under the target, which is the part at fault here.
        (
            "5[0]",
            "error (line 1, column 1): cannot index a int\n    5[0]\n    ^",
        ),
        (
            "fn f(a) { return a }\nf(1, 2)",
            "error (line 2, column 1): function f expects 1 argument(s) but received 2\n    f(1, 2)\n    ^",
        ),
        // A leading tab is reproduced in the caret indent, so the caret stays
        // aligned however the line is displayed.
        (
            "\tlet y = nil + 1",
            "error (line 1, column 14): cannot add a nil and a int\n    \tlet y = nil + 1\n    \t            ^",
        ),
    ]);
}

#[test]
fn syntax_errors_render_with_a_caret() {
    check_rendered(&[
        (
            "let = 1",
            "error (line 1, column 5): expected an identifier after 'let' but found '='\n    let = 1\n        ^",
        ),
        (
            "let x =",
            "error (line 1, column 8): expected an expression but found end of input\n    let x =\n           ^",
        ),
        (
            "print(",
            "error (line 1, column 7): expected an expression but found end of input\n    print(\n          ^",
        ),
        (
            "1 +",
            "error (line 1, column 4): expected an expression but found end of input\n    1 +\n       ^",
        ),
        (
            "if true {",
            "error (line 1, column 10): expected '}' to close a block but found end of input\n    if true {\n             ^",
        ),
        // Loop control outside a loop is caught while parsing, not at runtime.
        (
            "break",
            "error (line 1, column 1): break outside of a loop\n    break\n    ^",
        ),
        (
            "continue",
            "error (line 1, column 1): continue outside of a loop\n    continue\n    ^",
        ),
        (
            "@",
            "error (line 1, column 1): unexpected character '@'\n    @\n    ^",
        ),
        (
            "\"unterminated",
            "error (line 1, column 1): unterminated string literal\n    \"unterminated\n    ^",
        ),
        (
            "1 & 2",
            "error (line 1, column 3): unexpected '&' (did you mean '&&'?)\n    1 & 2\n      ^",
        ),
    ]);
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
fn functions_calls_and_recursion() {
    check_all(&[
        ("fn add(a, b) { return a + b }\nadd(2, 3)", "ok 5"),
        ("fn square(x) { return x * x }\nsquare(9)", "ok 81"),
        // A function is a first-class value with an inspect form.
        ("fn greet() { return \"hi\" }\ngreet", "ok <fn greet>"),
        // Falling off the end, and a bare return, both yield nil.
        ("fn nothing() { }\nnothing()", "ok nil"),
        ("fn early() { return }\nearly()", "ok nil"),
        (
            "fn fib(n) {\n  if n < 2 { return n }\n  return fib(n - 1) + fib(n - 2)\n}\nfib(10)",
            "ok 55",
        ),
        (
            "fn fact(n) {\n  if n < 2 { return 1 }\n  return n * fact(n - 1)\n}\nfact(6)",
            "ok 720",
        ),
        (
            "fn double(x) { return x * 2 }\nlet sum = 0\nfor x in [1, 2, 3] { sum = sum + double(x) }\nsum",
            "ok 12",
        ),
        ("let inc = fn(x) { return x + 1 }\ninc(41)", "ok 42"),
        ("(fn(x) { return x * 3 })(5)", "ok 15"),
        (
            "fn sign(n) {\n  if n > 0 { return 1 }\n  if n < 0 { return -1 }\n  return 0\n}\nsign(-8)",
            "ok -1",
        ),
        ("fn use_it(p) { return p * 10 }\nlet r = use_it(4)\nr", "ok 40"),
        // A function may call one declared after it, since both are globals.
        (
            "fn outer() { return inner() }\nfn inner() { return 7 }\nouter()",
            "ok 7",
        ),
        // Functions passed as arguments.
        (
            "fn apply(f, x) { return f(x) }\napply(fn(n) { return n + 100 }, 5)",
            "ok 105",
        ),
    ]);
}

#[test]
fn call_errors_report_the_call_site() {
    check_all(&[
        (
            "fn one(a) { return a }\none(1, 2)",
            "err function one expects 1 argument(s) but received 2 @ 2:1",
        ),
        (
            "fn one(a) { return a }\none()",
            "err function one expects 1 argument(s) but received 0 @ 2:1",
        ),
        ("let x = 5\nx(1)", "err a int is not callable @ 2:1"),
        ("nil()", "err a nil is not callable @ 1:1"),
    ]);
}

#[test]
fn closures_capture_by_reference_and_outlive_their_scope() {
    check_all(&[
        // Captured parameter, called after the enclosing function returned.
        (
            "fn make_adder(n) { return fn(x) { return x + n } }\nlet add5 = make_adder(5)\nadd5(10)",
            "ok 15",
        ),
        // A closed-over variable persists and mutates across calls (1 + 2).
        (
            "fn make_counter() {\n  let count = 0\n  return fn() { count = count + 1\nreturn count }\n}\nlet c = make_counter()\nlet a = c()\nlet b = c()\na + b",
            "ok 3",
        ),
        // Each closure instance captures its own variable (1 + 2 + 1).
        (
            "fn make_counter() {\n  let count = 0\n  return fn() { count = count + 1\nreturn count }\n}\nlet c1 = make_counter()\nlet c2 = make_counter()\nlet a = c1()\nlet b = c1()\nlet d = c2()\na + b + d",
            "ok 4",
        ),
        // Capturing an outer local while it is still live.
        (
            "fn outer() {\n  let base = 100\n  fn inner() { return base + 1 }\n  return inner()\n}\nouter()",
            "ok 101",
        ),
        // Capture threaded through two levels of nesting.
        (
            "fn a() {\n  let x = 10\n  fn b() {\n    fn c() { return x }\n    return c()\n  }\n  return b()\n}\na()",
            "ok 10",
        ),
        // Capture is by reference, so a later write is visible.
        (
            "fn f() {\n  let x = 1\n  let g = fn() { return x }\n  x = 99\n  return g()\n}\nf()",
            "ok 99",
        ),
        // And a write through the closure is visible outside it.
        (
            "fn f() {\n  let x = 1\n  let bump = fn() { x = x + 10 }\n  bump()\n  return x\n}\nf()",
            "ok 11",
        ),
        // Each iteration's loop variable is captured separately, so the three
        // closures add 1, 2, and 3 rather than all sharing the last value.
        (
            "fn adders() {\n  let fs = []\n  for i in [1, 2, 3] { push(fs, fn(x) { return x + i }) }\n  return fs\n}\nlet fs = adders()\nfs[0](10) + fs[1](10) + fs[2](10)",
            "ok 36",
        ),
    ]);
}

#[test]
fn arrays_maps_and_indexing() {
    check_all(&[
        ("[1, 2, 3]", "ok [1, 2, 3]"),
        ("[]", "ok []"),
        ("[1 + 1, 2 * 2, \"a\" + \"b\"]", "ok [2, 4, \"ab\"]"),
        ("[1, 2] == [1, 2]", "ok true"),
        ("[[1, 2], [3, 4]]", "ok [[1, 2], [3, 4]]"),
        ("[10, 20, 30][1]", "ok 20"),
        ("let a = [1, 2, 3]\na[0] + a[2]", "ok 4"),
        ("[[1, 2], [3, 4]][1][0]", "ok 3"),
        ("let a = [1, 2, 3]\na[1] = 99\na", "ok [1, 99, 3]"),
        ("{}", "ok {}"),
        ("{\"a\": 1, \"b\": 2}", "ok {\"a\": 1, \"b\": 2}"),
        ("{\"b\": 2, \"a\": 1}", "ok {\"a\": 1, \"b\": 2}"),
        ("{\"a\": 1, \"b\": 2}[\"a\"]", "ok 1"),
        ("let m = {\"x\": 10}\nm[\"x\"]", "ok 10"),
        ("{\"a\": 1}[\"missing\"]", "ok nil"),
        (
            "let k = \"name\"\nlet m = {k: \"Aiko\"}\nm[\"name\"]",
            "ok \"Aiko\"",
        ),
        (
            "let m = {\"a\": 1}\nm[\"b\"] = 2\nm",
            "ok {\"a\": 1, \"b\": 2}",
        ),
        ("{\"a\": 1} == {\"a\": 1}", "ok true"),
        (
            "[1, 2, 3][5]",
            "err index 5 is out of range for an array of length 3 @ 1:11",
        ),
        (
            "[1, 2, 3][-1]",
            "err index -1 is out of range (negative) @ 1:11",
        ),
        (
            "[1, 2][\"x\"]",
            "err array index must be an int, not a string @ 1:8",
        ),
        (
            "{\"a\": 1}[5]",
            "err map key must be a string, not a int @ 1:10",
        ),
        (
            "let a = [1, 2, 3]\na[9] = 0",
            "err index 9 is out of range for an array of length 3 @ 2:3",
        ),
        ("5[0]", "err cannot index a int @ 1:1"),
        ("nil[0]", "err cannot index a nil @ 1:1"),
        (
            "let x = 5\nx[0] = 1",
            "err cannot index-assign to a int @ 2:1",
        ),
    ]);
}

#[test]
fn builtins_and_their_errors() {
    check_all(&[
        ("len([1, 2, 3])", "ok 3"),
        ("len(\"hello\")", "ok 5"),
        ("len({\"a\": 1})", "ok 1"),
        ("len([])", "ok 0"),
        ("type(1)", "ok \"int\""),
        ("type(1.5)", "ok \"float\""),
        ("type(\"a\")", "ok \"string\""),
        ("type([])", "ok \"array\""),
        ("type({})", "ok \"map\""),
        ("type(nil)", "ok \"nil\""),
        ("type(true)", "ok \"bool\""),
        ("type(len)", "ok \"function\""),
        ("str(42) + \"!\"", "ok \"42!\""),
        ("str(nil)", "ok \"nil\""),
        ("str([1, 2])", "ok \"[1, 2]\""),
        ("range(4)", "ok [0, 1, 2, 3]"),
        ("range(2, 5)", "ok [2, 3, 4]"),
        ("range(0)", "ok []"),
        ("let a = [1]\npush(a, 2)\na", "ok [1, 2]"),
        ("upper(\"abc\")", "ok \"ABC\""),
        ("lower(\"DEF\")", "ok \"def\""),
        ("trim(\"  hi  \")", "ok \"hi\""),
        ("replace(\"a.b\", \".\", \"-\")", "ok \"a-b\""),
        ("split(\"a,b,c\", \",\")", "ok [\"a\", \"b\", \"c\"]"),
        ("split(\"hi\", \"\")", "ok [\"h\", \"i\"]"),
        ("join([1, 2, 3], \"-\")", "ok \"1-2-3\""),
        ("contains(\"hello\", \"ell\")", "ok true"),
        ("contains([1, 2], 2)", "ok true"),
        ("contains([1, 2], 9)", "ok false"),
        ("find(\"hello\", \"l\")", "ok 2"),
        ("find(\"hello\", \"z\")", "ok -1"),
        ("pop([1, 2, 3])", "ok 3"),
        ("index_of([10, 20], 20)", "ok 1"),
        ("index_of([1], 9)", "ok -1"),
        ("slice([1,2,3,4], 1, 3)", "ok [2, 3]"),
        ("slice(\"hello\", 1, 4)", "ok \"ell\""),
        ("slice([1, 2], 0, 99)", "ok [1, 2]"),
        ("sort([3, 1, 2])", "ok [1, 2, 3]"),
        ("sort([\"c\", \"a\"])", "ok [\"a\", \"c\"]"),
        ("reverse([1, 2, 3])", "ok [3, 2, 1]"),
        ("reverse(\"abc\")", "ok \"cba\""),
        ("abs(-5)", "ok 5"),
        ("abs(-2.5)", "ok 2.5"),
        ("min(3, 1, 2)", "ok 1"),
        ("max(3, 1, 2)", "ok 3"),
        ("min(2, 1.5)", "ok 1.5"),
        ("floor(2.7)", "ok 2"),
        ("ceil(2.1)", "ok 3"),
        ("round(2.5)", "ok 3"),
        ("round(2.4)", "ok 2"),
        ("sqrt(16)", "ok 4.0"),
        ("sqrt(9)", "ok 3.0"),
        ("pow(2, 10)", "ok 1024"),
        ("pow(2, -1)", "ok 0.5"),
        ("pow(2.0, 3)", "ok 8.0"),
        ("int(\"42\")", "ok 42"),
        ("int(2.9)", "ok 2"),
        ("int(7)", "ok 7"),
        ("float(3)", "ok 3.0"),
        ("float(\"1.5\")", "ok 1.5"),
        ("keys({\"b\": 2, \"a\": 1})", "ok [\"a\", \"b\"]"),
        ("values({\"b\": 2, \"a\": 1})", "ok [1, 2]"),
        ("has({\"a\": 1}, \"a\")", "ok true"),
        ("has({\"a\": 1}, \"z\")", "ok false"),
        (
            "len(1)",
            "err len expects a string, array, or map but got a int @ 1:1",
        ),
        ("upper(5)", "err upper expects a string but got a int @ 1:1"),
        ("sqrt(-1)", "err sqrt of a negative number @ 1:1"),
        ("pop([])", "err pop from an empty array @ 1:1"),
        ("int(\"abc\")", "err cannot convert \"abc\" to an int @ 1:1"),
        (
            "sort([1, \"a\"])",
            "err sort expects an array of all numbers or all strings @ 1:1",
        ),
        ("len()", "err len expects 1 argument(s) but got 0 @ 1:1"),
        ("keys([])", "err keys expects a map but got a array @ 1:1"),
        (
            "has([], \"a\")",
            "err has expects a map but got a array @ 1:1",
        ),
        (
            "push(5, 1)",
            "err push expects an array as its first argument but got a int @ 1:1",
        ),
        (
            "abs(\"a\")",
            "err abs expects a number but got a string @ 1:1",
        ),
        ("min()", "err min expects at least one argument @ 1:1"),
        (
            "float(\"zz\")",
            "err cannot convert \"zz\" to a float @ 1:1",
        ),
        (
            "join(5, \"-\")",
            "err join expects an array and a string separator @ 1:1",
        ),
        ("range(\"a\")", "err range expects integer arguments @ 1:1"),
    ]);
}

#[test]
fn higher_order_builtins() {
    check_all(&[
        ("map([1, 2, 3], fn(x) { return x * 2 })", "ok [2, 4, 6]"),
        ("filter([1, 2, 3, 4], fn(x) { return x % 2 == 0 })", "ok [2, 4]"),
        ("reduce([1, 2, 3, 4], fn(acc, x) { return acc + x }, 0)", "ok 10"),
        ("map([], fn(x) { return x })", "ok []"),
        ("reduce([], fn(a, b) { return a + b }, 42)", "ok 42"),
        ("fn double(x) { return x * 2 }\nmap([1, 2, 3], double)", "ok [2, 4, 6]"),
        ("map([-1, -2, 3], abs)", "ok [1, 2, 3]"),
        ("let n = 10\nlet add = fn(x) { return x + n }\nmap([1, 2, 3], add)", "ok [11, 12, 13]"),
        ("reduce(map(filter([1,2,3,4,5], fn(x) { return x % 2 == 1 }), fn(x) { return x * x }), fn(a, b) { return a + b }, 0)", "ok 35"),
        ("fn sum(xs) { return reduce(xs, fn(a, b) { return a + b }, 0) }\nsum([4, 5, 6])", "ok 15"),
        ("map([1, 0], fn(x) { return 1 / x })", "err division by zero @ 1:30"),
        ("map(5, fn(x) { return x })", "err map expects an array but got a int @ 1:1"),
        ("map([1, 2], 3)", "err a int is not callable @ 1:1"),
        ("map([1, 2])", "err map expects 2 argument(s) but got 1 @ 1:1"),
        ("filter(5, fn(x) { return true })", "err filter expects an array but got a int @ 1:1"),
        ("reduce([1, 2], fn(a, b) { return a })", "err reduce expects 3 argument(s) but got 2 @ 1:1"),
        ("filter([1,2,3], fn(x) { return nil })", "ok []"),
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
