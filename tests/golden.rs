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
    match miruscriptx::eval_source(source) {
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
        let actual = match miruscriptx::run_capture(source) {
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
            Ok(_) => match miruscriptx::eval_source(source) {
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
            "error (line 1, column 1): break outside of a loop\n    break\n    ^^^^^",
        ),
        (
            "continue",
            "error (line 1, column 1): continue outside of a loop\n    continue\n    ^^^^^^^^",
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
        // The caret sits at the start of the number rather than at the
        // separator, which is where every other error about a number sits. One
        // character wide, because a lexer error has no token to underline, the
        // same as the unterminated string above.
        (
            "let x = 1_",
            "error (line 1, column 9): a digit separator must be between two digits\n    let x = 1_\n            ^",
        ),
    ]);
}

#[test]
fn errors_underline_the_token_they_blame() {
    // The cases above mostly point at operators, which are one character wide
    // and render identically before and after underlining. These are the shape
    // that gains from it: an error blaming a *name*, where the underline shows
    // which name without the reader counting columns.
    check_rendered(&[
        (
            "let total = 1\nprint(totl)",
            "error (line 2, column 7): undefined variable 'totl'. Did you mean 'total'?\n    print(totl)\n          ^^^^",
        ),
        (
            "user_count = 1",
            "error (line 1, column 1): cannot assign to undefined variable 'user_count'\n    user_count = 1\n    ^^^^^^^^^^",
        ),
        // The call site is blamed, so the underline covers the callee's name.
        (
            "fn make_adder(x) {\n  return x\n}\nmake_adder(1, 2)",
            "error (line 4, column 1): function make_adder expects 1 argument(s) but received 2\n    make_adder(1, 2)\n    ^^^^^^^^^^",
        ),
        (
            "let counter = 5\ncounter()",
            "error (line 2, column 1): a int is not callable\n    counter()\n    ^^^^^^^",
        ),
        // Indexing blames the target, which here is a name rather than the
        // literal the earlier case used.
        (
            "let total = 5\ntotal[0]",
            "error (line 2, column 1): cannot index a int\n    total[0]\n    ^^^^^",
        ),
        // A keyword is a token like any other, so a syntax error naming one
        // underlines the whole keyword.
        (
            "let while = 1",
            "error (line 1, column 5): expected an identifier after 'let' but found 'while'\n    let while = 1\n        ^^^^^",
        ),
        // An underline and a call trace in one rendering, which is what a real
        // failure inside a function looks like.
        (
            "fn describe(value) {\n  return missing + value\n}\ndescribe(1)",
            "error (line 2, column 10): undefined variable 'missing'\n      return missing + value\n             ^^^^^^^\n  in describe, called from line 4",
        ),
    ]);
}

#[test]
fn runaway_recursion_is_an_error_rather_than_a_hang() {
    check_all(&[
        // Direct and mutual recursion both hit the frame cap.
        (
            "fn r(n) { return r(n + 1) }\nr(1)",
            "err call depth limit of 10000 exceeded @ 1:18",
        ),
        (
            "fn a(n) { return b(n) }\nfn b(n) { return a(n) }\na(1)",
            "err call depth limit of 10000 exceeded @ 1:18",
        ),
        // Recursing back through a builtin is now ordinary recursion: map calls
        // its callback by pushing a frame, not by entering a nested bytecode
        // loop, so there is one cap rather than two and this reaches the same
        // one direct recursion does.
        (
            "fn r(n) { return map([1], fn(x) { return r(n + 1) })[0] }\nr(1)",
            "err call depth limit of 10000 exceeded @ 1:18",
        ),
    ]);
}

#[test]
fn recursion_well_within_the_limit_still_works() {
    check_all(&[
        // A thousand frames is ordinary; the cap must not interfere.
        (
            "fn count(n) {\n  if n == 0 { return 0 }\n  return 1 + count(n - 1)\n}\ncount(1000)",
            "ok 1000",
        ),
        // Nested higher-order calls. Two hundred deep failed outright until the
        // builtin bridge went, because each level cost a nested bytecode loop
        // and the cap on those was sixty four. It is unremarkable now.
        (
            "fn depth(n) {\n  if n == 0 { return 0 }\n  return map([1], fn(x) { return 1 + depth(n - 1) })[0]\n}\ndepth(200)",
            "ok 200",
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
        // A digit separator groups the digits and is no part of the value, so
        // each of these is the same number written two ways.
        ("1_000", "ok 1000"),
        ("1_000_000", "ok 1000000"),
        ("1_000.5", "ok 1000.5"),
        ("1.000_5", "ok 1.0005"),
        ("9_223_372_036_854_775_807", "ok 9223372036854775807"),
        // A name that starts with an underscore is a name, not a number with a
        // separator in front of it. This is the one thing the separator could
        // have taken away.
        ("let _1 = 5\n_1", "ok 5"),
        ("let _ = 7\n_", "ok 7"),
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
        // Compound assignment, one case per target shape.
        ("let x = 1\nx += 5\nx", "ok 6"),
        ("let x = 20\nx -= 5\nx *= 3\nx /= 5\nx %= 4\nx", "ok 1"),
        ("let a = [1, 2, 3]\na[1] += 10\na", "ok [1, 12, 3]"),
        ("let m = {\"n\": 1}\nm.n += 41\nm", "ok {\"n\": 42}"),
        ("let s = \"a\"\ns += \"b\"\ns", "ok \"ab\""),
        // **The reason this is a statement of its own rather than a rewrite
        // into `a[next()] = a[next()] + 5`.** The rewrite calls `next` twice,
        // reads one element and writes another, and leaves the array wrong in
        // a way nothing would explain. One call, and `calls` says so.
        (
            "let calls = 0\nfn next() {\n  calls += 1\n  return 0\n}\n\
             let a = [10]\na[next()] += 5\n[a, calls]",
            "ok [[15], 1]",
        ),
        // Still a statement, so this is the error it always was.
        (
            "let x = 1\nlet y = (x += 1)",
            "err expected ')' to close a grouped expression but found '+=' @ 2:12",
        ),
        ("repeat(\"-\", 3)", "ok \"---\""),
        ("repeat(\"ab\", 3)", "ok \"ababab\""),
        // Zero gives the empty string. The loop this replaces did, and a
        // builtin that refused zero would be worse for a computed count.
        ("repeat(\"x\", 0)", "ok \"\""),
        (
            "repeat(\"x\", -1)",
            "err repeat expects a count of 0 or more but got -1 @ 1:1",
        ),
        // The fill goes where the name says, so `pad_left` lines text up on
        // the right. Both directions are pinned because a name that can be
        // read two ways should have the reading written down.
        ("pad_right(\"7\", 4)", "ok \"7   \""),
        ("pad_left(\"7\", 4)", "ok \"   7\""),
        ("pad_left(\"7\", 3, \"0\")", "ok \"007\""),
        // Already long enough: unchanged rather than cut, because losing text
        // to line a column up is worse than a ragged column.
        ("pad_left(\"hello\", 3)", "ok \"hello\""),
        ("pad_right(\"hello\", 5)", "ok \"hello\""),
        // Characters, not bytes. Measured in bytes the emoji is four, so this
        // would come back unchanged instead of gaining two spaces.
        ("len(pad_right(\"\\u{1F600}\", 3))", "ok 3"),
        (
            "pad_left(\"7\", 4, \"ab\")",
            "err pad_left expects a fill of one character but got one of 2 @ 1:1",
        ),
        (
            "pad_left(\"7\", -1)",
            "err pad_left expects a width of 0 or more but got -1 @ 1:1",
        ),
        ("ord(\"A\")", "ok 65"),
        ("chr(65)", "ok \"A\""),
        // Code points, not bytes. The emoji is four bytes and one character,
        // so a byte-oriented `ord` gives 240 here.
        ("ord(\"\\u{1F600}\")", "ok 128512"),
        ("chr(128512) == \"\\u{1F600}\"", "ok true"),
        // The property the pair exists to have.
        ("chr(ord(\"\\u{1F600}\")) == \"\\u{1F600}\"", "ok true"),
        // `chr` consults the same test `\u{...}` does, so the refusals are the
        // same set in the same words. Section 2.8 pins the escape's side.
        ("chr(55296)", "err '\\u{D800}' is not a character @ 1:1"),
        ("chr(1114112)", "err '\\u{110000}' is not a character @ 1:1"),
        // Not a code point at all, so reported as itself rather than dressed
        // up as an escape: -1 in hex is '\\u{FFFFFFFFFFFFFFFF}'.
        ("chr(-1)", "err -1 is not a character @ 1:1"),
        (
            "ord(\"hi\")",
            "err ord expects a string of one character but got one of 2 @ 1:1",
        ),
        (
            "ord(\"\")",
            "err ord expects a string of one character but got one of 0 @ 1:1",
        ),
        ("\"abc\"[0]", "ok \"a\""),
        ("\"abc\"[2]", "ok \"c\""),
        // Characters, not bytes. The emoji is four bytes and one character, so
        // a byte-indexed implementation gives `[1]` as a fragment or refuses,
        // and both disagree with `len` and with `split(s, "")`.
        ("len(\"a\\u{1F600}b\")", "ok 3"),
        ("\"a\\u{1F600}b\"[1]", "ok \"\u{1F600}\""),
        ("\"a\\u{1F600}b\"[2]", "ok \"b\""),
        ("\"hello\"[1] == split(\"hello\", \"\")[1]", "ok true"),
        (
            "\"abc\"[3]",
            "err index 3 is out of range for a string of length 3 @ 1:7",
        ),
        (
            "\"\"[0]",
            "err index 0 is out of range for a string of length 0 @ 1:4",
        ),
        // Refused rather than counted from the end, so that meaning stays
        // available to a later release.
        (
            "\"abc\"[-1]",
            "err index -1 is out of range (negative) @ 1:7",
        ),
        (
            "\"abc\"[\"x\"]",
            "err string index must be an int, not a string @ 1:7",
        ),
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
        ("starts_with(\"hello.miru\", \"hello\")", "ok true"),
        ("ends_with(\"hello.miru\", \".miru\")", "ok true"),
        ("starts_with(\"hello\", \"jelly\")", "ok false"),
        ("ends_with(\"hello\", \"jelly\")", "ok false"),
        // An empty needle is a prefix and a suffix of everything, and a needle
        // longer than the string is neither.
        ("starts_with(\"a\", \"\")", "ok true"),
        ("ends_with(\"a\", \"\")", "ok true"),
        ("starts_with(\"ab\", \"abc\")", "ok false"),
        // Characters, not bytes: `é` is two bytes and the emoji is four.
        ("starts_with(\"héllo\", \"hé\")", "ok true"),
        ("ends_with(\"a\\u{1F600}\", \"\\u{1F600}\")", "ok true"),
        ("sum([1, 2, 3])", "ok 6"),
        ("product([2, 3, 4])", "ok 24"),
        // The identity values, which are what let the two compose.
        ("sum([])", "ok 0"),
        ("product([])", "ok 1"),
        // One float anywhere makes the answer a float.
        ("sum([1, 2.5])", "ok 3.5"),
        ("product([2, 1.5])", "ok 3.0"),
        ("sum([2.0])", "ok 2.0"),
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
        ("remove({\"a\": 1, \"b\": 2}, \"a\")", "ok 1"),
        ("remove({\"a\": 1}, \"z\")", "ok nil"),
        ("remove({}, \"a\")", "ok nil"),
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
            "remove([], \"a\")",
            "err remove expects a map but got a array @ 1:1",
        ),
        (
            "remove({\"a\": 1}, 1)",
            "err remove expects a string key but got a int @ 1:1",
        ),
        (
            "remove({\"a\": 1})",
            "err remove expects 2 argument(s) but got 1 @ 1:1",
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

/// `sort(a, key)` orders by what the key function gives, not by the elements.
///
/// The one-argument form is pinned above, in the builtin block, and those cases
/// are unchanged by this: `sort` became a higher-order builtin in 1.6 and its
/// existing behaviour did not move.
#[test]
fn sort_takes_a_key_function() {
    check_all(&[
        (
            "sort([{\"a\": 2}, {\"a\": 1}], fn(x) { return x.a })",
            "ok [{\"a\": 1}, {\"a\": 2}]",
        ),
        // A builtin is a key function like any other, which also exercises the
        // path that drives a task without ever leaving `call_native`.
        ("sort([\"bbb\", \"a\", \"cc\"], len)", "ok [\"a\", \"cc\", \"bbb\"]"),
        // Decreasing order is `reverse`, which is why no second argument for it
        // was added.
        (
            "reverse(sort([3, 1, 2], fn(x) { return x }))",
            "ok [3, 2, 1]",
        ),
        // **Stability.** Two passes, least significant key first, is a two-key
        // sort. This case fails if the sort stops being stable, which section
        // 8.4 of the specification now promises it is.
        (
            "let p = [{\"a\": 1, \"n\": \"b\"}, {\"a\": 1, \"n\": \"a\"}, {\"a\": 0, \"n\": \"c\"}]\n\
             let by_name = sort(p, fn(x) { return x.n })\n\
             map(sort(by_name, fn(x) { return x.a }), fn(x) { return x.n })",
            "ok [\"c\", \"a\", \"b\"]",
        ),
        ("sort([], fn(x) { return x })", "ok []"),
        ("sort([5], fn(x) { return x })", "ok [5]"),
        // The keys go through the same ordering rules the elements do, so a key
        // function that gives something unorderable gives the existing error.
        // A caught error is refused here rather than by `Value::condition`: a
        // key is not a conditional, and it is not a number or a string either.
        (
            "sort([1, 2], fn(x) { return {} })",
            "err sort expects an array of all numbers or all strings @ 1:1",
        ),
        (
            "sort([1, 2], fn(x) { return try (1 / 0) })",
            "err sort expects an array of all numbers or all strings @ 1:1",
        ),
        // `nil` is refused rather than read as "no key function", which is how
        // the task tells the two apart internally.
        (
            "sort([1, 2], nil)",
            "err sort expects a function but got a nil @ 1:1",
        ),
        (
            "sort([1, 2], fn(x) { return x }, 3)",
            "err sort expects 1 or 2 argument(s) but got 3 @ 1:1",
        ),
        ("sort()", "err sort expects 1 or 2 argument(s) but got 0 @ 1:1"),
        // Since 1.6 `sort` is a higher-order builtin, so a caught error handed
        // to it is refused by its own type check rather than by the guard in
        // `call_native`, which higher-order builtins do not reach. Both stop
        // the program; only the wording differs, which section 3.1 of the
        // guarantee leaves free.
        (
            "let e = try (1 / 0)\nsort(e)",
            "err sort expects an array but got a error @ 2:1",
        ),
    ]);
}

#[test]
fn higher_order_builtins_iterate_a_snapshot_of_their_input() {
    // A callback that pushes to the array it is being applied to does not
    // extend the iteration: each builtin copies its input before it starts, so
    // it walks the array as it was when it was handed over.
    //
    // This is not an accident of how `map` happens to be written. `for x in xs`
    // does the same, through OpCode::IterSnapshot, so the two agree. Pinning
    // both together is the point: an optimization that drops the copy from the
    // builtins would make them disagree with the loop, and that is a language
    // change rather than a faster implementation.
    //
    // Nothing tested this before v0.7 went looking for the cost of that copy.
    // The callbacks are named rather than written inline because a newline is
    // insignificant inside parentheses, so a multi-line function body written
    // directly as a call argument does not parse. That is unrelated to what
    // these cases are about.
    check_all(&[
        (
            "let xs = [1, 2, 3]\nfn tap(x) {\n  push(xs, x)\n  return x\n}\nlet ys = map(xs, tap)\n[len(xs), len(ys)]",
            "ok [6, 3]",
        ),
        (
            "let xs = [1, 2, 3]\nfn keep(x) {\n  push(xs, x)\n  return true\n}\nlet ys = filter(xs, keep)\n[len(xs), len(ys)]",
            "ok [6, 3]",
        ),
        (
            "let xs = [1, 2, 3]\nfn add(a, b) {\n  push(xs, b)\n  return a + b\n}\nlet total = reduce(xs, add, 0)\n[len(xs), total]",
            "ok [6, 6]",
        ),
        // The loop, for comparison. Same shape, same answer.
        (
            "let xs = [1, 2, 3]\nlet n = 0\nfor x in xs {\n  push(xs, x)\n  n = n + 1\n}\n[len(xs), n]",
            "ok [6, 3]",
        ),
    ]);
}

#[test]
fn a_higher_order_builtin_can_be_another_one_s_callback() {
    // `reduce` applies its function to two arguments, which is exactly what
    // `map` and `filter` take, so one higher-order builtin can be handed to
    // another and the inner one's result becomes the accumulator. Strange code,
    // but it works, and nothing pinned it.
    //
    // `map` and `filter` pass one argument, which never matches the two or
    // three the host builtins need, so `reduce` is the only one that can host
    // another. Anything else is an arity error, and the last case pins that too.
    //
    // This matters for more than curiosity: it is the one shape where a
    // higher-order builtin's result belongs to another higher-order builtin
    // rather than to the expression that called it.
    check_all(&[
        // Both elements are `abs`, so this is map([1, -2], abs) twice over.
        ("reduce([abs, abs], map, [1, 0 - 2])", "ok [1, 2]"),
        // The same shape with `filter`, whose result is shorter than its input.
        (
            "fn keep(x) {\n  return x > 1\n}\nreduce([keep], filter, [1, 2, 3])",
            "ok [2, 3]",
        ),
        // One level, so the inner result is the final answer rather than an
        // accumulator that gets used again.
        ("reduce([abs], map, [0 - 5])", "ok [5]"),
        // `map` hands its callback a single argument, so a host builtin in that
        // position never has the arity it needs.
        (
            "map([[1], [2]], map)",
            "err map expects 2 argument(s) but got 1 @ 1:1",
        ),
    ]);
}

#[test]
fn a_newline_stays_insignificant_inside_parentheses_and_brackets() {
    // The lexer drops newline tokens while it is inside `(` or `[`, which is
    // what lets an expression span lines. These are the shapes that rely on it,
    // pinned before v0.8 changes how braces interact with that rule, so the
    // change has to prove it broke none of them.
    check_all(&[
        // A wrapped expression.
        ("(1 +\n2)", "ok 3"),
        // An argument list split over lines.
        ("max(1,\n2)", "ok 2"),
        // An array literal split over lines, with a trailing comma.
        ("len([1,\n2,\n3,\n])", "ok 3"),
        // A map literal as a call argument. This one is the interesting case:
        // braces do not suppress newlines, because blocks need them, so the map
        // parser skips newlines between entries itself.
        ("len({\"a\": 1,\n\"b\": 2})", "ok 2"),
        // The same, nested one deeper, so a brace sits between two groups.
        ("len({\"a\": [1,\n2]})", "ok 1"),
        // A block does need its newlines, and gets them at the top level today.
        ("fn f() {\n  let a = 1\n  return a\n}\nf()", "ok 1"),
    ]);
}

#[test]
fn a_multi_line_function_body_parses_inside_a_call_or_an_array() {
    // Newline suppression inside a group used to reach into a block nested
    // within it, so a function body of more than one statement lost the
    // separators its statements need. A multi-line callback handed straight to
    // `map` is the obvious thing to write and did not parse.
    //
    // A brace now restores newline significance whatever is open outside it.
    check_all(&[
        (
            "map([1, 2], fn(x) {\n  let d = x * 2\n  return d\n})",
            "ok [2, 4]",
        ),
        // Control flow inside the callback, which needs the separators most.
        (
            "filter([1, 2, 3], fn(x) {\n  if x == 2 {\n    return false\n  }\n  return true\n})",
            "ok [1, 3]",
        ),
        // Inside a bracket group rather than a call.
        ("[fn() {\n  let a = 1\n  return a\n}][0]()", "ok 1"),
        // A brace inside a brace inside a group: a map literal holding a
        // multi-line function, as a call argument.
        ("len({\"f\": fn() {\n  let a = 1\n  return a\n}})", "ok 1"),
    ]);
}

/// `try` cannot catch an exit, and that is the whole correctness of `exit`.
///
/// An exit stops the program by raising an error, so without the fatal marking
/// in `call_native` a `try` would turn it into a value and the program would
/// carry on with an exit code already recorded. Its caller would then be told a
/// result the program never reached, which is a lie no test of the exit code
/// alone would catch.
///
/// The error surfaces here because `eval_source` has no exit code to consult.
/// Through `miru run` the same program prints nothing and exits with the code;
/// `tests/integration.rs` pins that half.
#[test]
fn an_exit_is_not_catchable() {
    check_all(&[
        // Caught, the value would be an error and the program would continue.
        // Uncaught, it reaches the top as a failure.
        ("try exit(4)", "err the program called exit @ 1:5"),
        (
            "let r = try exit(0)\nprint(r)",
            "err the program called exit @ 1:13",
        ),
        // Inside a function inside a try, which is the shape that would most
        // plausibly slip past a guard placed at only one depth.
        (
            "fn stop() {\n  exit(7)\n}\ntry stop()",
            "err the program called exit @ 2:3",
        ),
        // A refused code never records one, so it stays an ordinary catchable
        // error. This is the case that proves the guard keys on the recorded
        // code and not on the name of the builtin.
        ("is_error(try exit(999))", "ok true"),
        ("is_error(try exit(\"x\"))", "ok true"),
    ]);
}

/// `remove` is the inverse of assignment, which a map had no way to undo.
///
/// Setting a key to `nil` does not remove it. That is the defect this closes,
/// and the first case pins it so nobody "simplifies" `remove` into an
/// assignment later.
#[test]
fn remove_takes_a_key_out_of_a_map() {
    check_all(&[
        // The old way, which does not work: the key stays and still counts.
        (
            "let m = {\"a\": 1, \"b\": 2}\nm[\"a\"] = nil\n[len(m), has(m, \"a\"), keys(m)]",
            "ok [2, true, [\"a\", \"b\"]]",
        ),
        // The new way.
        (
            "let m = {\"a\": 1, \"b\": 2}\nlet gone = remove(m, \"a\")\n[gone, len(m), has(m, \"a\"), keys(m)]",
            "ok [1, 1, false, [\"b\"]]",
        ),
        // An absent key is not an error, so "remove it if it is there" is one
        // call. This is the decision that cannot change without a 2.0.
        (
            "let m = {\"b\": 2}\n[remove(m, \"zz\"), len(m)]",
            "ok [nil, 1]",
        ),
        // The cost of that decision, stated as a test rather than left to be
        // discovered: a stored nil and an absent key give the same answer.
        // `has` before the removal is what tells them apart.
        (
            "let m = {\"a\": nil}\nlet before = has(m, \"a\")\nlet got = remove(m, \"a\")\n[before, got, has(m, \"a\"), len(m)]",
            "ok [true, nil, false, 0]",
        ),
        // A map is a reference, so a removal is seen through every name for it.
        (
            "let m = {\"a\": 1, \"b\": 2}\nlet same = m\nremove(m, \"a\")\n[len(same), keys(same)]",
            "ok [1, [\"b\"]]",
        ),
        // Removing every key leaves a map that still works.
        (
            "let m = {\"a\": 1}\nremove(m, \"a\")\nm[\"c\"] = 3\n[len(m), m[\"c\"]]",
            "ok [1, 3]",
        ),
        // The value comes back whole, not a copy: mutating it after removal is
        // visible through the reference the caller kept.
        (
            "let m = {\"a\": [1, 2]}\nlet taken = remove(m, \"a\")\npush(taken, 3)\n[len(m), taken]",
            "ok [0, [1, 2, 3]]",
        ),
    ]);
}

#[test]
fn a_field_reads_a_map_entry_and_insists_it_exists() {
    check_all(&[
        ("let m = {\"a\": 1, \"b\": 2}\nm.b", "ok 2"),
        // Chained, and through a call's result.
        ("let m = {\"a\": {\"b\": 7}}\nm.a.b", "ok 7"),
        ("fn f() {\n  return {\"x\": 1}\n}\nf().x", "ok 1"),
        // The difference from indexing, in one program: a missing key reads as
        // nil, a missing field does not get that far.
        ("let m = {\"a\": 1}\nm[\"nope\"]", "ok nil"),
        // The candidates are that map's own keys, so a near one is offered and
        // a name nowhere near any of them is not.
        ("let m = {\"a\": 1}\nm.nope", "err no field 'nope' @ 2:3"),
        (
            "let m = {\"name\": 1}\nm.nmae",
            "err no field 'nmae'. Did you mean 'name'? @ 2:3",
        ),
        // The target is blamed when it has no fields at all, so the two errors
        // point at different halves of the same expression.
        (
            "let n = 5\nn.field",
            "err cannot read a field of a int @ 2:1",
        ),
        (
            "let m = {}\nm.1",
            "err expected a field name after '.' but found integer '1' @ 2:3",
        ),
    ]);
}

#[test]
fn a_missing_field_underlines_the_name() {
    // The v0.7 underline earns its keep here: the field is the part at fault
    // and it is the part marked, without the reader counting columns.
    check_rendered(&[
        (
            "let cfg = {\"timeout\": 30}\ncfg.tiemout",
            "error (line 2, column 5): no field 'tiemout'. Did you mean 'timeout'?\n    cfg.tiemout\n        ^^^^^^^",
        ),
        (
            "let n = 5\nn.field",
            "error (line 2, column 1): cannot read a field of a int\n    n.field\n    ^",
        ),
    ]);
}

#[test]
fn import_parses_and_says_why_it_cannot_resolve_without_a_file() {
    // Resolving a path needs to know which file the import was written in, and
    // a program compiled from a string has none. That is the playground's
    // situation and eval_source's, so this message is the real answer rather
    // than a placeholder for one.
    check_all(&[
        (
            "import \"./math.miru\" as math",
            "err cannot import: this program was not loaded from a file @ 1:8",
        ),
        // The path is a quoted string rather than a bare word, so it can hold
        // any filename.
        (
            "import math",
            "err expected a quoted path after 'import' but found identifier 'math' @ 1:8",
        ),
        // The alias is required rather than inferred from the path: a reader
        // should be able to tell where a name came from without working out
        // what a filename would have been shortened to.
        (
            "import \"./a.miru\"",
            "err expected 'as' after an import path but found end of input @ 1:18",
        ),
        // The same import with a newline after it reports "end of line"
        // instead, because that is the token it actually met. Pinned because
        // the difference is easy to mistake for a wording inconsistency.
        (
            "import \"./a.miru\"\n",
            "err expected 'as' after an import path but found end of line @ 1:18",
        ),
        (
            "import \"./a.miru\" as 5",
            "err expected a name after 'as' but found integer '5' @ 1:22",
        ),
    ]);
}

#[test]
fn a_program_evaluates_to_its_last_expression() {
    check_all(&[("1\n2\n3", "ok 3"), ("1 + 1\n2 + 2", "ok 4")]);
}

#[test]
fn cases_inherited_from_the_differential_suite() {
    // v0.4 verified the VM by running these against the tree walker. They are
    // kept here so retiring that engine loses no coverage: every case its
    // corpus exercised is now pinned to a literal expectation.
    check_all(&[
        ("!!0", "ok true"),
        ("!nil", "ok true"),
        ("!true", "ok false"),
        ("\"x\" < \"y\"", "ok true"),
        ("false || true", "ok true"),
        ("abs(-5) + min(3, 1) + max(2, 7)", "ok 13"),
        ("floor(2.7) + ceil(2.1) + round(2.5)", "ok 8"),
        ("int(\"42\") + int(2.9)", "ok 44"),
        ("pow(2, 8)", "ok 256"),
        ("slice([1, 2, 3, 4], 1, 3)", "ok [2, 3]"),
        ("type(1) + type(\"a\")", "ok \"intstring\""),
        ("upper(\"abc\") + lower(\"DEF\")", "ok \"ABCdef\""),
        ("reduce([], fn(acc, x) { return acc + x }, 42)", "ok 42"),
        ("fn total(xs) {\n  let sum = 0\n  for x in xs { sum = sum + x }\n  return sum\n}\ntotal(range(5))", "ok 10"),
        ("let n = 7\nlet label = \"\"\nif n % 2 == 0 { label = \"even\" } else { label = \"odd\" }\nlabel", "ok \"odd\""),
        ("let sum = 0\nlet a = [5, 6, 7]\nfor x in a { sum = sum + x }\nsum", "ok 18"),
        ("map([1, 2], fn(x) { return reduce([1, 2, 3], fn(a, b) { return a + b }, x) })", "ok [7, 8]"),
        ("reduce(map(filter([1, 2, 3, 4, 5], fn(x) { return x % 2 == 1 }), fn(x) { return x * x }), fn(a, b) { return a + b }, 0)", "ok 35"),
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

#[test]
fn operators_with_a_constant_right_operand_keep_their_results_and_positions() {
    // These compile to the fused BinaryConst instruction rather than a constant
    // load followed by the operator. The caret must land on the operator, the
    // same place the unfused pair put it, and the fused form must decline the
    // same cases so the shared operator rules produce the message.
    check_all(&[
        ("let x = 1\nx / 0", "err division by zero @ 2:3"),
        ("let x = 1\nx % 0", "err modulo by zero @ 2:3"),
        (
            "let x = 9223372036854775807\nx + 1",
            "err integer overflow in addition @ 2:3",
        ),
        (
            "let x = -9223372036854775807\nx - 2",
            "err integer overflow in subtraction @ 2:3",
        ),
        (
            "let x = 9223372036854775807\nx * 2",
            "err integer overflow in multiplication @ 2:3",
        ),
        (
            "let s = \"a\"\ns - 1",
            "err cannot subtract a string and a int @ 2:3",
        ),
        // And the ordinary results, which have to match what the pair gave.
        ("let x = 10\nx / 4", "ok 2"),
        ("let x = 10\nx % 4", "ok 2"),
        ("let x = 1\nx + 2.5", "ok 3.5"),
        ("let s = \"a\"\ns + \"b\"", "ok \"ab\""),
        ("let x = 3\nx < 4", "ok true"),
        ("let x = 3\nx >= 4", "ok false"),
        // A right operand that folds to a constant fuses as that constant.
        ("let x = 1\nx + 2 * 3", "ok 7"),
        // A right operand that cannot fold stays an ordinary two-instruction
        // operator, and its errors are unchanged.
        ("let x = 1\nlet y = 0\nx / y", "err division by zero @ 3:3"),
        // true, false, and nil are deliberately not fused; they keep their
        // single-byte opcodes.
        ("let x = 1\nx == true", "ok false"),
        ("let x = 1\nx != nil", "ok true"),
    ]);
}

#[test]
fn runtime_errors_carry_the_call_path_they_came_through() {
    check_rendered(&[
        // One call deep. The caret says where it broke; the trace says how it
        // was reached.
        (
            "fn add(a) {\n  return a + 1\n}\nadd(nil)",
            "error (line 2, column 12): cannot add a nil and a int\n      return a + 1\n               ^\n  in add, called from line 4",
        ),
        // Two deep, innermost first.
        (
            "fn add(a) {\n  return a + 1\n}\nfn total(xs) {\n  let s = 0\n  s = add(nil)\n  return s\n}\ntotal([1])",
            "error (line 2, column 12): cannot add a nil and a int\n      return a + 1\n               ^\n  in add, called from line 6\n  in total, called from line 9",
        ),
        // At the top level there is no call path, so nothing is appended and the
        // rendering is what it always was.
        (
            "nil + 1",
            "error (line 1, column 5): cannot add a nil and a int\n    nil + 1\n        ^",
        ),
        // A closure called by a builtin. This is the case the capture has to get
        // right: `map` runs its callback on a nested bytecode loop, and the
        // frames are torn down as the error leaves it.
        (
            "fn double(x) {\n  return x * nil\n}\nmap([1], double)",
            "error (line 2, column 12): cannot multiply a int and a nil\n      return x * nil\n               ^\n  in double, called from line 4",
        ),
        // An anonymous function is named as such rather than left blank.
        (
            "map([1], fn(x) { return x * nil })",
            "error (line 1, column 27): cannot multiply a int and a nil\n    map([1], fn(x) { return x * nil })\n                              ^\n  in <anonymous>, called from line 1",
        ),
        // Runaway recursion would otherwise print ten thousand identical lines
        // and bury the error in its own trace. The two ends carry the
        // information: where it broke, and how the program entered the
        // recursion, which is the "line 2" entry at the bottom.
        (
            "fn r(n) { return r(n + 1) }\nr(1)",
            "error (line 1, column 18): call depth limit of 10000 exceeded\n    fn r(n) { return r(n + 1) }\n                     ^\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 1\n  ... 9989 more frames\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 1\n  in r, called from line 2",
        ),
        // A syntax error never has a call path: there is no call stack yet.
        (
            "let = 1",
            "error (line 1, column 5): expected an identifier after 'let' but found '='\n    let = 1\n        ^",
        ),
    ]);
}

#[test]
fn try_turns_a_failure_into_a_value_and_leaves_a_success_alone() {
    check_all(&[
        // The failure becomes the expression's value instead of stopping the
        // program.
        ("try 1 / 0", "ok <error: division by zero>"),
        // Nothing failed, so `try` is invisible.
        ("try 6 * 7", "ok 42"),
        // `try` takes the whole expression after it rather than binding like a
        // unary operator, so this covers the division and not just the 1.
        ("try 1 / 0 + 5", "ok <error: division by zero>"),
        // Parentheses narrow it. The division is outside the guard, so nothing
        // catches it.
        ("(try 1) / 0", "err division by zero @ 1:9"),
        // The check a program makes. `type` is the one builtin that may be
        // handed a failure, because this is how a program learns it has one.
        ("type(try 1 / 0)", "ok \"error\""),
        ("type(try 6 * 7)", "ok \"int\""),
        // Nested: the inner one catches, so the outer one sees an ordinary
        // value and does nothing.
        ("try (try 1 / 0)", "ok <error: division by zero>"),
    ]);
}

#[test]
fn a_failure_is_caught_from_any_depth_and_the_program_carries_on() {
    check_all(&[
        // Three frames deep. The failure is raised in c, and the try is in the
        // script frame, so two frames have to be unwound to reach it.
        (
            "fn c() { return nope }\nfn b() { return c() }\nfn a() { return b() }\ntype(try a())",
            "ok \"error\"",
        ),
        // And the VM keeps working afterwards, which is what says the unwinding
        // put the stack back rather than merely not crashing.
        ("fn c() { return nope }\nlet r = try c()\n1 + 1", "ok 2"),
        // A callback that fails inside a higher-order builtin. This is the case
        // that exercises the task stack: `map` is suspended waiting for the
        // call that fails, and that suspension has to be rewound too.
        (
            "type(try map([1, 2, 3], fn(x) { return x / 0 }))",
            "ok \"error\"",
        ),
        // Same program, then map again, so a rewound task stack is proven not
        // to have left anything behind.
        (
            "let r = try map([1], fn(x) { return x / 0 })\nmap([1, 2], fn(x) { return x * 2 })",
            "ok [2, 4]",
        ),
    ]);
}

#[test]
fn using_a_caught_failure_stops_the_program_where_it_was_used() {
    check_all(&[
        // Named at the operator, not at the division that failed, because the
        // mistake is here: the program had the failure and did something else
        // with it. The failure's own message rides along.
        (
            "let r = try 1 / 0\nr + 1",
            "err unhandled error: division by zero @ 2:3",
        ),
        // The silent one. Without a guard this reads as false and the program
        // takes the else branch as though nothing had gone wrong.
        (
            "let r = try 1 / 0\nif r { 1 } else { 2 }",
            // Under `r` rather than under `if`: the condition is the failure,
            // and it is what a reader needs to look at.
            "err unhandled error: division by zero @ 2:4",
        ),
        // Likewise equality, which answers for every pair of values.
        (
            "let r = try 1 / 0\nr == nil",
            "err unhandled error: division by zero @ 2:3",
        ),
        // And a builtin that takes anything.
        (
            "let r = try 1 / 0\nprint(r)",
            "err unhandled error: division by zero @ 2:1",
        ),
    ]);
}

#[test]
fn try_does_not_catch_the_call_depth_limit() {
    // Runaway recursion is a bug in the program rather than a condition to
    // handle, so this one failure refuses to become a value. A `try` that
    // swallowed it would hide the only thing worth knowing.
    let outcome = outcome("fn boom(n) { return boom(n + 1) }\nlet r = try boom(0)\nr");
    assert!(
        outcome.starts_with("err call depth limit of 10000 exceeded"),
        "outcome was: {outcome}"
    );
}

#[test]
fn a_caught_failure_can_be_asked_what_went_wrong() {
    check_all(&[
        // The check itself. `type(v) == "error"` says the same thing; this
        // cannot be misspelled without failing outright.
        ("is_error(try 1 / 0)", "ok true"),
        ("is_error(42)", "ok false"),
        // What went wrong, and where. The position is the failure's own, which
        // is why using one reports at the point of use instead: both are
        // available, each where it belongs.
        ("(try 1 / 0).message", "ok \"division by zero\""),
        ("(try 1 / 0).line", "ok 1"),
        ("(try 1 / 0).column", "ok 8"),
        // nil rather than "" when the failure came from the file being run, so
        // "no file" is distinguishable from a file with an empty name.
        ("(try 1 / 0).file", "ok nil"),
        // Nothing was called, so there is no path to report.
        ("(try 1 / 0).trace", "ok []"),
        // A misspelling fails rather than reading nil, the same bargain field
        // access makes everywhere else.
        (
            "(try 1 / 0).nope",
            "err an error has no field 'nope' @ 1:13",
        ),
    ]);
}

#[test]
fn the_call_path_survives_into_a_caught_caught_error() {
    // The v0.6 trace is captured before anything is unwound, so a failure
    // carries where it came from even after being caught. Knowing that
    // something failed is much less useful than knowing where.
    check_all(&[
        (
            "fn f() { return nope }\n(try f()).trace",
            "ok [\"in f, called from line 2\"]",
        ),
        (
            "fn c() { return nope }\nfn b() { return c() }\nfn a() { return b() }\nlen((try a()).trace)",
            "ok 3",
        ),
    ]);
}

#[test]
fn a_field_can_be_assigned_through_and_creates_what_is_not_there() {
    check_all(&[
        // The half v0.8 left out: reading a field worked, writing one did not
        // parse.
        ("let m = {\"a\": 1}\nm.a = 2\nm.a", "ok 2"),
        // Assigning to a field that is not there creates it, where *reading*
        // one that is not there is an error. The asymmetry is the point: a
        // misspelling on the way in is almost always a mistake, and on the way
        // out almost never is.
        ("let m = {}\nm.a = 1\nm", "ok {\"a\": 1}"),
        // Which is exactly what the bracket form has always done, so the two
        // spellings agree on the one thing they could have differed on.
        ("let m = {}\nm.a = 1\nm[\"a\"]", "ok 1"),
        ("let m = {}\nm[\"a\"] = 1\nm.a", "ok 1"),
        // Nested, so the target of an assignment can itself be a field read.
        (
            "let n = {\"deep\": {\"x\": 1}}\nn.deep.x = 9\nn.deep.x",
            "ok 9",
        ),
        // A target that has no fields is reported at the target, as index
        // assignment already reports one that cannot be indexed.
        ("let n = 5\nn.a = 1", "err cannot assign a field of a int @ 2:1"),
        // And what is still not a target.
        (
            "f() = 1",
            "err invalid assignment target (only variables, elements, and fields can be assigned to) @ 1:1",
        ),
    ]);
}

#[test]
fn a_higher_order_builtin_refuses_a_caught_failure_as_a_condition() {
    check_all(&[
        // filter asks its callback's answer whether it is true, which is the
        // same question `if` asks, and a caught failure has to refuse it in both
        // places. Until v1.0 this one read the failure as true and silently kept
        // every element, which is the one path the v0.9 guard missed.
        (
            "filter([1, 2, 3], fn(n) { return try 1 / 0 })",
            "err unhandled error: division by zero @ 1:1",
        ),
        // The position is the call that started the task, not wherever the
        // callback happened to be.
        (
            "let xs = [1]\nfilter(xs, fn(n) { return try nope })",
            "err unhandled error: undefined variable 'nope' @ 2:1",
        ),
        // Ordinary filtering is untouched.
        (
            "filter([1, 2, 3, 4], fn(n) { return n % 2 == 0 })",
            "ok [2, 4]",
        ),
        // map and reduce only ever store what they are handed, which is what
        // assigning a failure already does everywhere else, so they keep it.
        // Storing one is allowed; using it is not.
        (
            "let out = map([1], fn(n) { return try 1 / 0 })\nis_error(out[0])",
            "ok true",
        ),
    ]);
}

#[test]
fn iterating_a_caught_error_names_the_original_caught_error() {
    check_all(&[
        // The last consumer path v0.9 missed. The wildcard answered "cannot
        // iterate over a error", which is true, generic, and about the type
        // rather than about what went wrong.
        (
            "let r = try 1 / 0\nfor x in r { }",
            "err unhandled error: division by zero @ 2:10",
        ),
        // The generic message still belongs to everything that genuinely has
        // no elements.
        ("for x in 5 { }", "err cannot iterate over a int @ 1:10"),
    ]);
}

#[test]
fn a_value_that_contains_itself_is_survivable() {
    check_all(&[
        // Until v1.0 each of these aborted the process on a Rust stack
        // overflow: no caret, no trace, and uncatchable, which is not an
        // outcome a program should be able to cause.
        //
        // Printing shows the cycle where it closes, as Python does.
        ("let a = []\npush(a, a)\nstr(a)", "ok \"[[...]]\""),
        (
            "let m = {}\nm.self = m\nstr(m)",
            "ok \"{\\\"self\\\": {...}}\"",
        ),
        // Mutual recursion closes one level further down.
        (
            "let a = []\nlet b = []\npush(a, b)\npush(b, a)\nstr(a)",
            "ok \"[[[...]]]\"",
        ),
        // The same array twice is a shape, not a cycle, and still prints whole.
        // A depth counter alone would have got this wrong.
        ("let x = [1, 2]\nstr([x, x])", "ok \"[[1, 2], [1, 2]]\""),
        // Comparing a cyclic value with itself is answered by identity before
        // anything is walked, so it does not need the depth limit at all.
        ("let a = []\npush(a, a)\na == a", "ok true"),
        // Two different cyclic values have no answer, so comparing them refuses
        // rather than guessing. It is an ordinary error: `try` catches it.
        (
            "let a = []\npush(a, a)\nlet b = []\npush(b, b)\na == b",
            "err value is nested too deeply to compare @ 5:3",
        ),
        (
            "let a = []\npush(a, a)\nlet b = []\npush(b, b)\nis_error(try a == b)",
            "ok true",
        ),
    ]);
}

#[test]
fn a_misspelled_name_is_offered_the_one_it_is_nearest() {
    // The unit tests for the rule use three or four candidates. These run it
    // against the real table, where every one of the 44 builtins is a
    // candidate, which is the only place a threshold that is too generous
    // shows itself.
    check_all(&[
        // The example the issue was opened with.
        (
            "prnt(\"abc\")",
            "err undefined variable 'prnt'. Did you mean 'print'? @ 1:1",
        ),
        // A swap of two letters, which plain Levenshtein charges two edits for
        // and this length allows only one of.
        (
            "pirnt(\"abc\")",
            "err undefined variable 'pirnt'. Did you mean 'print'? @ 1:1",
        ),
        // Case is not part of the comparison.
        (
            "LEN(\"abc\")",
            "err undefined variable 'LEN'. Did you mean 'len'? @ 1:1",
        ),
        // Nothing is near this, and guessing anyway would send the reader off
        // before they had started looking.
        ("xyzzy", "err undefined variable 'xyzzy' @ 1:1"),
        // Three edits from `len`, because the builtin is not called `length`.
        // Declining is the right answer, and it is the case the threshold was
        // checked against before it was kept.
        ("lenght(\"abc\")", "err undefined variable 'lenght' @ 1:1"),
    ]);
}

#[test]
fn assignment_never_introduces_a_name() {
    check_all(&[
        // One rule for both: `let` introduces a name, assignment does not.
        // Until v1.0 assigning to a builtin's name succeeded and wrote over it
        // for every module in the program, while assigning to any other
        // undeclared name was already this error.
        //
        // No suggestion here, and that is the point of this case now as well:
        // `print` is spelled correctly, and `eprint` is one edit away, so a
        // message that guessed would answer a question nobody asked.
        (
            "print = 1",
            "err cannot assign to undefined variable 'print' @ 1:1",
        ),
        // A misspelled target is the other half. The name is genuinely not
        // there, so the near one is worth offering.
        (
            "let total = 1\ntotl = 2",
            "err cannot assign to undefined variable 'totl'. Did you mean 'total'? @ 2:1",
        ),
        ("x = 1", "err cannot assign to undefined variable 'x' @ 1:1"),
        // Declaring it first makes it the module's own name, and a name the
        // module owns can be assigned like any other.
        ("let print = 1\nprint = 2\nprint", "ok 2"),
        // Refusing the assignment leaves the builtin alone.
        ("len(\"abc\")", "ok 3"),
    ]);
}

#[test]
fn declaring_a_name_a_builtin_already_has_shadows_it() {
    check_all(&[
        // Until v1.0 this wrote over the builtin's slot instead of shadowing
        // it, and builtin slots are shared by every module, so one file could
        // break `print` inside a module it imported. See the integration test
        // for the cross-file half.
        ("let print = 1\nprint", "ok 1"),
        ("let len = 99\nlen", "ok 99"),
        // The builtin is untouched everywhere the name was not declared.
        ("len(\"abc\")", "ok 3"),
        // And a file that shadows one still has all the others.
        ("let len = 1\nupper(\"ab\")", "ok \"AB\""),
    ]);
}

/// Run a test body on the stack the interpreter is built to assume.
///
/// A libtest thread gets 2 MiB, which is less than `Parser::MAX_NESTING`
/// requires. `miru` starts a 64 MiB thread for exactly this reason (`STACK_SIZE`
/// in `src/main.rs`) and the WebAssembly build links a 16 MiB shadow stack.
/// Without the same here, a test that reaches the limit overflows before it gets
/// there, and measures libtest rather than the language.
fn with_interpreter_stack<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(body)
        .expect("the operating system can start a thread")
        .join()
        .expect("the test body did not panic")
}

/// Deep source used to abort the process with a Rust stack overflow: no
/// message, no caret, and nothing `try` could catch. Every one of these was
/// reproduced as an abort before the parser started counting.
///
/// The sources are generated rather than written out, and follow the two
/// constants, so raising either limit does not quietly stop these from reaching
/// it.
///
/// **The two halves get different messages, and the split is the point.**
/// Nesting costs a parser frame per level and a chain costs none, so they have
/// separate limits, and a chain that reported "nested too deeply" would send a
/// reader looking for brackets that are not in their program.
#[test]
fn source_that_nests_too_deeply_is_an_error_rather_than_an_abort() {
    with_interpreter_stack(|| {
        let over = miruscriptx::parser::Parser::MAX_NESTING + 1;
        // Nesting the parser descends through. These overflow on the way down,
        // before there is a tree to measure.
        let nested: Vec<String> = vec![
            format!("{}{}", "[".repeat(over), "]".repeat(over)),
            format!("({}1{})", "(".repeat(over), ")".repeat(over)),
            format!("{}1{}", "{\"a\": ".repeat(over), "}".repeat(over)),
            format!("{}1", "try ".repeat(over)),
            format!("{}1", "-".repeat(over)),
            format!("{}true", "!".repeat(over)),
        ];
        for source in &nested {
            let outcome = outcome(source);
            assert!(
                outcome.starts_with("err the program is nested too deeply"),
                "expected a nesting error, got: {outcome}\n  source begins: {:?}",
                &source[..source.len().min(60)]
            );
        }

        let tall = miruscriptx::parser::Parser::MAX_HEIGHT + 1;
        // The composition case grows as the product of its two dimensions, so
        // each side only needs to be the square root of the limit to pass it.
        // Using `tall` for both would build a source of many megabytes.
        let side = (tall as f64).sqrt() as usize + 2;
        let long: Vec<String> = vec![
            // Chains. These are one Rust frame however long they run, so
            // counting the parser's recursion sees nothing: the tree gets tall.
            format!("1{}", " + 1".repeat(tall)),
            format!("a{}", "[0]".repeat(tall)),
            format!("a{}", ".b".repeat(tall)),
            format!("a{}", "(1)".repeat(tall)),
            // A chain at every level of nesting. The tree grows as the product
            // of the two, so counting either alone lets this through. Neither
            // dimension on its own reaches either limit here.
            (0..side).fold(String::from("1"), |inner, _| {
                format!("({}{})", inner, " + 1".repeat(side))
            }),
        ];
        for source in &long {
            let outcome = outcome(source);
            assert!(
                outcome.starts_with("err the expression is too long"),
                "expected a length error, got: {outcome}\n  source begins: {:?}",
                &source[..source.len().min(60)]
            );
        }
    });
}

#[test]
fn nesting_well_within_the_limit_still_works() {
    with_interpreter_stack(|| {
        // The limits must not interfere with ordinary programs. The deepest
        // bracket nesting anywhere in this repository's own examples is 4.
        check_all(&[
            ("[[[[[[[[1]]]]]]]]", "ok [[[[[[[[1]]]]]]]]"),
            ("((((((((1))))))))", "ok 1"),
            ("1 + 1 + 1 + 1 + 1 + 1 + 1 + 1", "ok 8"),
        ]);
        // A chain half the length of its limit is fine, which is far more
        // arithmetic than anyone writes on one line.
        let long = miruscriptx::parser::Parser::MAX_HEIGHT / 2;
        let sum = format!("1{}", " + 1".repeat(long - 1));
        assert_eq!(outcome(&sum), format!("ok {long}"));
        // As is nesting to half of its own, which is the smaller figure.
        let deep = miruscriptx::parser::Parser::MAX_NESTING / 2;
        let nest = format!("{}1{}", "[".repeat(deep), "]".repeat(deep));
        assert!(outcome(&nest).starts_with("ok ["));
    });
}

/// The limits exist to keep a 1.0 program working, not only to stop an abort.
///
/// 1.0 had no limit, so anything that fitted the stack parsed. The first attempt
/// at this fix set the limit at 64, taken from a 2 MiB thread, which refused
/// programs 1.0 accepted and broke section 2.1 of the stability guarantee.
/// These depths are the ones a real program plausibly reaches.
#[test]
fn depths_a_1_0_program_could_reach_still_parse() {
    with_interpreter_stack(|| {
        // A generated expression summing several hundred terms.
        let sum = format!("1{}", " + 1".repeat(499));
        assert_eq!(outcome(&sum), "ok 500");
        // A generated table, nested well past anything written by hand.
        let nest = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        assert!(outcome(&nest).starts_with("ok ["));
        // A long chain of field accesses, as a generated accessor path.
        let fields = format!("let m = {{}}\nm{}", ".a".repeat(300));
        assert!(outcome(&fields).starts_with("err no field"));
    });
}

/// The exact ceilings 1.0.1 reached, so neither limit can drift back under them.
///
/// The second attempt broke 2.1 as well, more quietly than the first. One
/// constant governed both nesting and chain length, and 1000 was right for
/// nesting and far too strict for a chain: a sum of 2000 terms ran on *every*
/// 1.0 build, the browser included, and became a syntax error. The two are
/// separate constants now.
///
/// These numbers are measurements, not guesses. They come from running the
/// released 1.0.1 binary under `ulimit -s 1024`, which is the 1 MiB shadow
/// stack the playground had and so the least any 1.0 build could do. A program
/// that cleared these worked everywhere 1.0 ran, and has to keep working.
#[test]
fn the_ceilings_1_0_reached_on_its_smallest_stack_still_parse() {
    with_interpreter_stack(|| {
        // 1.0.1 on a 1 MiB stack summed 4959 terms. It managed 40255 on an
        // 8 MiB main thread, which is past `MAX_HEIGHT` and deliberately not
        // covered: that band ran on a large native stack and never in a
        // browser, so it was never a program 1.0 ran everywhere.
        let sum = format!("1{}", " + 1".repeat(4958));
        assert_eq!(outcome(&sum), "ok 4959");
        // The one that actually regressed, and the reason this test exists.
        let modest = format!("1{}", " + 1".repeat(1999));
        assert_eq!(outcome(&modest), "ok 2000");
        // 1.0.1 on the same 1 MiB stack reached 917 levels of bracket nesting.
        // `MAX_NESTING` is 1000, so nesting never regressed; this pins that.
        let nest = format!("{}1{}", "[".repeat(917), "]".repeat(917));
        assert!(outcome(&nest).starts_with("ok ["));
    });
}

/// A chain of values used to abort the process when it was released.
///
/// Building one is a loop, so it needs no stack. Releasing one walked the chain
/// by recursion, one Rust frame per link, and overflowed. It happened at the
/// assignment that dropped the last reference, or at the end of the program,
/// where it also lost whatever was still buffered on standard output.
///
/// Deliberately **not** wrapped in `with_interpreter_stack`. These run on a
/// 2 MiB libtest thread, which is the point: teardown must not depend on the
/// stack the `miru` binary gives itself. Every depth here aborted before.
#[test]
fn a_long_chain_of_values_is_released_without_recursion() {
    check_all(&[
        // An array holding an array holding an array, 50000 deep. This aborted
        // somewhere above 30000 in a debug build.
        (
            "let a = []\nlet i = 0\nwhile i < 50000 {\n  a = [a]\n  i = i + 1\n}\na = 0\n\"released\"",
            "ok \"released\"",
        ),
        // The same through maps.
        (
            "let m = {}\nlet i = 0\nwhile i < 50000 {\n  m = {\"a\": m}\n  i = i + 1\n}\nm = 0\n\"released\"",
            "ok \"released\"",
        ),
        // And through closures, which chain by capturing each other. This one
        // was not in the plan: the audit found it.
        (
            "fn base() { return 0 }\nlet f = base\nlet i = 0\nwhile i < 50000 {\n  let g = f\n  f = fn() { return g() }\n  i = i + 1\n}\nf = 0\n\"released\"",
            "ok \"released\"",
        ),
    ]);
}

/// Releasing one holder must not release what another still points at.
///
/// This matters more than the depth tests. Teardown descends only into a body
/// nobody else holds; getting that condition wrong frees live data, which is a
/// use-after-free rather than a crash, and no depth test would notice.
#[test]
fn releasing_one_holder_leaves_a_shared_value_alone() {
    check_all(&[
        // `b` still sees the shared array after `a` goes.
        (
            "let inner = [1, 2, 3]\nlet a = [inner]\nlet b = [inner]\na = 0\nb",
            "ok [[1, 2, 3]]",
        ),
        // And it is still the *same* array, not a copy: writing through one
        // name shows through the other.
        (
            "let inner = [1, 2]\nlet a = [inner]\nlet b = [inner]\na = 0\npush(inner, 3)\nb",
            "ok [[1, 2, 3]]",
        ),
        // The same for a map.
        (
            "let inner = {\"n\": 1}\nlet a = [inner]\nlet b = [inner]\na = 0\ninner[\"n\"] = 2\nb",
            "ok [{\"n\": 2}]",
        ),
        // A deep chain held twice: releasing one holder keeps the other whole.
        (
            "let s = []\nlet i = 0\nwhile i < 50000 {\n  s = [s]\n  i = i + 1\n}\nlet keep = [s]\ns = 0\nlen(keep)",
            "ok 1",
        ),
    ]);
}

/// v1.0 promised that a value containing itself does not stop the process.
/// Teardown must not change that: such a value is a cycle, so no reference
/// count reaches zero and nothing is released, which is what happened before.
#[test]
fn a_value_that_contains_itself_still_behaves_as_promised() {
    check_all(&[
        ("let a = []\npush(a, a)\na == a", "ok true"),
        ("let a = []\npush(a, a)\nstr(a)", "ok \"[[...]]\""),
    ]);
}

/// A function literal is a height leaf: `ExprKind::Function` holds statements,
/// not expressions, so `Expr::height` stops at one there. Nesting them is
/// bounded by the separate counter in `Parser::statement` instead.
///
/// That is a claim about two mechanisms meeting, which is the kind that is
/// usually assumed and occasionally wrong, so it gets a test. The compiler
/// walks a nested function by recursion when it compiles the inner chunks, so
/// the consequence of being wrong is the same abort as everywhere else.
#[test]
fn nested_function_literals_are_bounded_by_the_statement_counter() {
    with_interpreter_stack(|| {
        let nest = |levels: usize| {
            let mut source = String::from("0");
            for _ in 0..levels {
                source = format!("fn() {{\n  return {source}\n}}");
            }
            format!("let f = {source}\ntype(f)")
        };
        // Deep nesting is ordinary and compiles.
        assert_eq!(outcome(&nest(200)), "ok \"function\"");
        // Past the point where the statement counter adds up to the limit, it
        // reports rather than aborting. Each level costs more than one unit, so
        // this trips well below `MAX_NESTING` levels of function, which is the
        // conservative direction. It is the nesting limit and not the length
        // one, because `ExprKind::Function` is a leaf for height: every level
        // here is a statement the parser descends into, not a taller tree.
        let outcome = outcome(&nest(miruscriptx::parser::Parser::MAX_NESTING));
        assert!(
            outcome.starts_with("err the program is nested too deeply"),
            "expected a nesting error, got: {outcome}"
        );
    });
}

/// The file builtins refuse where the host has no file system.
///
/// Golden tests run through `eval_source`, which grants no `System` at all, so
/// this is exactly the state the playground and any embedder is in. The refusal
/// is a raised error rather than `nil`, because a program that reads a file and
/// silently gets nothing goes on to do the wrong thing with it.
#[test]
fn the_file_builtins_refuse_where_there_is_no_file_system() {
    check_all(&[
        (
            "read_file(\"data.txt\")",
            "err this program is running where there is no file system @ 1:1",
        ),
        (
            "write_file(\"data.txt\", \"x\")",
            "err this program is running where there is no file system @ 1:1",
        ),
        // Asking whether a file is there answers rather than refusing: with no
        // file system, the honest answer is no.
        ("file_exists(\"data.txt\")", "ok false"),
        // The refusal is catchable, like any other runtime error.
        ("is_error(try read_file(\"data.txt\"))", "ok true"),
        (
            "let r = try read_file(\"data.txt\")\nr.message",
            "ok \"this program is running where there is no file system\"",
        ),
    ]);
}

/// `now` refuses where the host has no clock.
///
/// Golden tests grant no clock, for the same reason they grant no file system:
/// what a host supplies is the host's decision, and the default is nothing.
/// That is also what keeps every case in this file deterministic. `now` is the
/// first builtin whose result the source does not determine, so a golden case
/// that called it with a real clock behind it could not assert an answer.
#[test]
fn now_refuses_where_there_is_no_clock() {
    check_all(&[
        (
            "now()",
            "err this program is running where there is no clock @ 1:1",
        ),
        // Catchable, like the file refusals.
        ("is_error(try now())", "ok true"),
        (
            "let r = try now()\nr.message",
            "ok \"this program is running where there is no clock\"",
        ),
        // The arity check runs before the clock is consulted, so the complaint
        // about the call is the same whether or not the host has one.
        ("now(1)", "err now expects 0 argument(s) but got 1 @ 1:1"),
    ]);
}

/// `read_key` refuses where the host has no keyboard.
///
/// Golden cases grant nothing, which is the state the playground and any
/// embedder is in. The refusal is an error rather than `nil`, because `nil`
/// means "no more keys" and a host with no keyboard has not run out of them.
/// What `read_key` gives when there *is* a keyboard is tested in `src/lib.rs`,
/// against a scripted one.
#[test]
fn read_key_refuses_where_there_is_no_keyboard() {
    check_all(&[
        (
            "read_key()",
            "err this program is running where there is no keyboard @ 1:1",
        ),
        ("is_error(try read_key())", "ok true"),
        (
            "let r = try read_key()\nr.message",
            "ok \"this program is running where there is no keyboard\"",
        ),
        (
            "read_key(1)",
            "err read_key expects 0 argument(s) but got 1 @ 1:1",
        ),
    ]);
}

/// `random` is asserted by its properties, never by its value.
///
/// A golden case that pinned a number would be pinning something the guarantee
/// deliberately leaves free: section 3.8 of `docs/stability.md` says a later 1.x
/// can change the generator. What a program can depend on is the range, and
/// these cases hold exactly that. The exact sequence is pinned once, as a unit
/// test in `src/random.rs`, where it is labelled a change detector.
///
/// These run under a capture with no clock, so the generator starts from its
/// fixed value and the cases are deterministic without setting a seed.
#[test]
fn random_stays_inside_its_range() {
    check_all(&[
        ("let r = random()\nr >= 0 && r < 1", "ok true"),
        ("type(random())", "ok \"float\""),
        // Two draws in one program are two draws, not one value read twice.
        ("random() == random()", "ok false"),
        (
            "random(1)",
            "err random expects 0 argument(s) but got 1 @ 1:1",
        ),
    ]);
}

/// `random_int` is held to its range and to what it refuses, for the same
/// reason as `random`: the numbers are free and the rules are not.
#[test]
fn random_int_stays_inside_its_range() {
    check_all(&[
        ("let n = random_int(1, 6)\nn >= 1 && n <= 6", "ok true"),
        ("type(random_int(1, 6))", "ok \"int\""),
        // A range of one value is that value, and does not spin in the
        // rejection loop looking for another.
        ("random_int(4, 4)", "ok 4"),
        // A range across zero is the case the unsigned arithmetic inside has to
        // get right.
        ("let n = random_int(-3, 3)\nn >= -3 && n <= 3", "ok true"),
        (
            "random_int(6, 1)",
            "err random_int expects a low bound not above the high bound but got 6 and 1 @ 1:1",
        ),
        (
            "random_int(1, 6.5)",
            "err random_int expects an int high bound but got a float @ 1:1",
        ),
        (
            "random_int(\"1\", 6)",
            "err random_int expects an int low bound but got a string @ 1:1",
        ),
        (
            "random_int(1)",
            "err random_int expects 2 argument(s) but got 1 @ 1:1",
        ),
    ]);
}

/// A seed makes the numbers repeat, and that is the only thing about them a
/// program may depend on.
///
/// Each case here asserts a comparison rather than a number, so all of them
/// stay true across a change of generator, which section 3.8 of the guarantee
/// allows. The one case that pins actual output lives in `src/random.rs` and
/// says there that it is a change detector.
#[test]
fn a_seed_makes_a_run_repeat() {
    check_all(&[
        (
            "seed(1)\nlet a = random()\nseed(1)\na == random()",
            "ok true",
        ),
        (
            "seed(7)\nlet a = [random_int(1, 1000), random_int(1, 1000)]\n\
             seed(7)\na == [random_int(1, 1000), random_int(1, 1000)]",
            "ok true",
        ),
        // A seed set part way through a program takes effect from there, so
        // this is not the same as seeding at the start.
        (
            "seed(1)\nlet a = random()\nlet b = random()\nseed(1)\nrandom() == b",
            "ok false",
        ),
        ("seed(3)", "ok nil"),
        ("seed(1.5)", "err seed expects an int but got a float @ 1:1"),
        ("seed()", "err seed expects 1 argument(s) but got 0 @ 1:1"),
    ]);
}

/// Argument checking happens before the host is consulted, so a program gets
/// the same complaint about a wrong type whether or not files are available.
#[test]
fn the_file_builtins_check_their_arguments() {
    check_all(&[
        (
            "read_file(1)",
            "err read_file expects a string path but got a int @ 1:1",
        ),
        (
            "write_file(1, \"x\")",
            "err write_file expects a string path but got a int @ 1:1",
        ),
        (
            "write_file(\"p\", 1)",
            "err write_file expects a string to write but got a int @ 1:1",
        ),
        (
            "file_exists([])",
            "err file_exists expects a string path but got a array @ 1:1",
        ),
        (
            "read_file()",
            "err read_file expects 1 argument(s) but got 0 @ 1:1",
        ),
        (
            "write_file(\"p\")",
            "err write_file expects 2 argument(s) but got 1 @ 1:1",
        ),
    ]);
}

/// `args()` gives an empty array where there is no command line, rather than
/// refusing. A program given no arguments and a program that could not have
/// been given any both have none, and the loop over them is the same.
#[test]
fn args_is_empty_where_there_is_no_command_line() {
    check_all(&[
        ("args()", "ok []"),
        ("len(args())", "ok 0"),
        ("args(1)", "err args expects 0 argument(s) but got 1 @ 1:1"),
    ]);
}
