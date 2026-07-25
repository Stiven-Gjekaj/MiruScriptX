//! Language behavior tests exercised through the public API.
//!
//! These began as the tree-walking interpreter's own unit tests. The engine they
//! were written against has been retired, but what they check is the language,
//! not the engine, so they moved here rather than being deleted with it. They
//! complement the golden corpus: these read as prose about one behavior each,
//! while the golden tests pin exact outcomes in bulk.

use miruscriptx::value::Value;
use miruscriptx::MiruError;

/// Run a program and return its final value.
fn run(source: &str) -> Value {
    miruscriptx::eval_source(source).expect("program should run")
}

fn repr(source: &str) -> String {
    run(source).repr()
}

/// Run a program expected to fail, returning the error it raised.
fn error(source: &str) -> MiruError {
    match miruscriptx::eval_source(source) {
        Ok(value) => panic!(
            "expected an error but the program returned {}",
            value.repr()
        ),
        Err(err) => err,
    }
}

#[test]
fn runtime_error_points_at_the_operator() {
    // The '/' sits at column 3, where the division by zero happens.
    let err = error("1 / 0");
    assert_eq!(err.line, 1);
    assert_eq!(err.column, 3);
}

#[test]
fn undefined_variable_points_at_the_name() {
    let err = error("  nope");
    assert_eq!(err.column, 3);
}

#[test]
fn out_of_range_index_points_at_the_index() {
    // The index expression 5 sits at column 7, where the lookup fails.
    let err = error("[1][  5]");
    assert_eq!(err.column, 7);
}

#[test]
fn arithmetic_precedence() {
    assert_eq!(repr("1 + 2 * 3"), "7");
    assert_eq!(repr("(1 + 2) * 3"), "9");
}

#[test]
fn integer_division_truncates_but_floats_do_not() {
    assert_eq!(repr("10 / 3"), "3");
    assert_eq!(repr("10.0 / 4"), "2.5");
}

#[test]
fn numeric_promotion_produces_floats() {
    assert_eq!(repr("2 + 3.0"), "5.0");
    assert_eq!(repr("2.0"), "2.0");
}

#[test]
fn string_concatenation() {
    assert_eq!(repr("\"Hello, \" + \"world\""), "\"Hello, world\"");
}

#[test]
fn comparisons_and_logic() {
    assert_eq!(repr("1 < 2"), "true");
    assert_eq!(repr("1 == 1.0"), "true");
    assert_eq!(repr("true && false"), "false");
    assert_eq!(repr("false || true"), "true");
    assert_eq!(repr("!nil"), "true");
}

#[test]
fn variables_and_reassignment() {
    assert_eq!(repr("let x = 5\nx = x + 1\nx"), "6");
}

#[test]
fn arrays_index_and_assign() {
    assert_eq!(repr("let a = [1, 2, 3]\na[1]"), "2");
    assert_eq!(repr("let a = [1, 2, 3]\na[0] = 9\na"), "[9, 2, 3]");
}

#[test]
fn for_loop_sums_an_array() {
    assert_eq!(
        repr("let total = 0\nfor x in [1, 2, 3, 4] {\n  total = total + x\n}\ntotal"),
        "10"
    );
}

#[test]
fn while_loop_counts() {
    assert_eq!(
        repr("let i = 0\nlet sum = 0\nwhile i < 5 {\n  sum = sum + i\n  i = i + 1\n}\nsum"),
        "10"
    );
}

#[test]
fn if_else_selects_a_branch() {
    let source =
        "let out = \"\"\nlet x = 7\nif x % 2 == 0 {\n  out = \"even\"\n} else {\n  out = \"odd\"\n}\nout";
    assert_eq!(repr(source), "\"odd\"");
}

#[test]
fn recursive_fibonacci() {
    let source = "fn fib(n) {\n  if n < 2 {\n    return n\n  }\n  return fib(n - 1) + fib(n - 2)\n}\nfib(10)";
    assert_eq!(repr(source), "55");
}

#[test]
fn closures_capture_their_environment() {
    let source =
        "fn make_adder(x) {\n  return fn(y) {\n    return x + y\n  }\n}\nlet add5 = make_adder(5)\nadd5(3)";
    assert_eq!(repr(source), "8");
}

#[test]
fn a_function_without_return_yields_nil() {
    assert_eq!(repr("fn noop() {}\nnoop()"), "nil");
}

#[test]
fn reports_undefined_variable_with_line() {
    let err = error("let a = 1\nb");
    assert_eq!(err.line, 2);
    assert!(err.message.contains("undefined variable 'b'"));
}

#[test]
fn reports_division_by_zero() {
    assert!(error("1 / 0").message.contains("division by zero"));
}

#[test]
fn reports_type_errors() {
    assert!(error("\"a\" - 1").message.contains("cannot subtract"));
}

#[test]
fn reports_index_out_of_range() {
    assert!(error("let a = [1]\na[5]").message.contains("out of range"));
}

#[test]
fn reports_wrong_argument_count() {
    let err = error("fn f(a) {\n  return a\n}\nf(1, 2)");
    assert!(err.message.contains("expects 1 argument"));
}

#[test]
fn reports_calling_a_non_function() {
    assert!(error("let x = 5\nx()").message.contains("not callable"));
}

#[test]
fn break_stops_a_loop() {
    let source =
        "let last = 0\nfor n in range(1, 10) {\n  if n == 5 { break }\n  last = n\n}\nlast";
    assert_eq!(repr(source), "4");
}

#[test]
fn continue_skips_to_the_next_iteration() {
    let source =
        "let sum = 0\nfor n in range(1, 6) {\n  if n % 2 == 0 { continue }\n  sum = sum + n\n}\nsum";
    assert_eq!(repr(source), "9");
}

#[test]
fn break_works_in_a_while_loop() {
    let source = "let i = 0\nwhile true {\n  i = i + 1\n  if i == 3 { break }\n}\ni";
    assert_eq!(repr(source), "3");
}

#[test]
fn break_only_exits_the_inner_loop() {
    let source = "let count = 0\nfor a in range(0, 3) {\n  for b in range(0, 3) {\n    if b == 1 { break }\n    count = count + 1\n  }\n}\ncount";
    assert_eq!(repr(source), "3");
}

#[test]
fn evaluates_map_literals_in_sorted_order() {
    assert_eq!(
        repr("{\"name\": \"Aiko\", \"age\": 3}"),
        "{\"age\": 3, \"name\": \"Aiko\"}"
    );
}

#[test]
fn evaluates_an_empty_map() {
    assert_eq!(repr("{}"), "{}");
}

#[test]
fn map_key_must_be_a_string() {
    assert!(error("{1: 2}").message.contains("map key must be a string"));
}

#[test]
fn reads_and_writes_map_values() {
    let source = "let m = {\"a\": 1}\nm[\"b\"] = 2\nm[\"a\"] = 10\nm";
    assert_eq!(repr(source), "{\"a\": 10, \"b\": 2}");
}

#[test]
fn missing_map_key_reads_as_nil() {
    assert_eq!(repr("let m = {\"a\": 1}\nm[\"z\"]"), "nil");
}

#[test]
fn map_index_key_must_be_a_string() {
    assert!(error("let m = {\"a\": 1}\nm[3]")
        .message
        .contains("map key must be a string"));
}

#[test]
fn a_literal_repeated_past_the_constant_limit_still_compiles() {
    // A `Constant` operand is one byte, so a chunk holds 256 of them. Each
    // occurrence of a literal used to take its own slot, which made this
    // perfectly ordinary program fail to compile at line 257.
    let mut source = String::from("let n = 0\n");
    for _ in 0..300 {
        source.push_str("n = n + 1\n");
    }
    source.push('n');
    assert_eq!(repr(&source), "300");
}

#[test]
fn a_chunk_may_hold_more_than_two_hundred_and_fifty_six_distinct_constants() {
    // v0.5 made the constant pool count distinct values rather than every
    // occurrence, which was the bug, but left the cap itself at what one operand
    // byte could address. ConstantLong retires it: the pool is addressed by two
    // bytes when it outgrows one.
    let mut source = String::from("let n = 0\n");
    for i in 0..300 {
        source.push_str(&format!("n = n + {i}\n"));
    }
    source.push('n');
    // The sum of 0 through 299.
    assert_eq!(repr(&source), "44850");
}

#[test]
fn a_map_literal_may_hold_more_than_two_hundred_and_fifty_five_entries() {
    // Three hundred entries need three hundred distinct key strings, so this
    // exercises the entry count and the constant pool at once. It is the shape
    // a generated lookup table takes.
    let entries: Vec<String> = (0..300).map(|i| format!("\"k{i}\": {i}")).collect();
    let source = format!("let m = {{{}}}\nlen(m) + m[\"k299\"]", entries.join(", "));
    assert_eq!(repr(&source), "599");
}

#[test]
fn a_file_may_hold_more_than_two_hundred_and_fifty_six_functions() {
    // Closure's function index was one byte, so a library of three hundred
    // small functions failed to compile at the two hundred and fifty seventh.
    let mut source = String::new();
    for i in 0..300 {
        source.push_str(&format!("fn f{i}(x) {{ return x + {i} }}\n"));
    }
    source.push_str("f299(1)");
    assert_eq!(repr(&source), "300");
}

#[test]
fn an_array_literal_may_hold_more_than_two_hundred_and_fifty_five_elements() {
    // Array's element count was one byte. The values here repeat so that the
    // constant pool is not what is being tested.
    let source = format!("let a = [{}]\nlen(a)", vec!["7"; 300].join(", "));
    assert_eq!(repr(&source), "300");
}
