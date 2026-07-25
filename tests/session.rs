//! Tests for [`miruscriptx::Session`], the accumulating state behind the REPL.
//!
//! The REPL itself needs a terminal, so it is checked by hand. Everything it
//! relies on lives in `Session`, which is exercised here: state carried from one
//! input to the next, and staying usable after an input fails.

use miruscriptx::Session;

/// Evaluate one input, describing the outcome the way the REPL would show it.
fn eval(session: &mut Session, source: &str) -> String {
    match session.eval(source) {
        Ok(value) => format!("ok {}", value.repr()),
        Err(error) => format!("err {}", error.message),
    }
}

#[test]
fn definitions_persist_across_inputs() {
    let mut session = Session::with_output(Box::new(std::io::sink()));
    assert_eq!(eval(&mut session, "let x = 1"), "ok nil");
    assert_eq!(eval(&mut session, "x"), "ok 1");
    assert_eq!(eval(&mut session, "x = x + 41"), "ok nil");
    assert_eq!(eval(&mut session, "x"), "ok 42");
    assert_eq!(
        eval(&mut session, "fn double(n) { return n * 2 }"),
        "ok nil"
    );
    assert_eq!(eval(&mut session, "double(21)"), "ok 42");
    // A later input can build on both.
    assert_eq!(eval(&mut session, "double(x)"), "ok 84");
}

#[test]
fn a_later_input_can_redefine_an_earlier_one() {
    let mut session = Session::with_output(Box::new(std::io::sink()));
    eval(&mut session, "let x = 1");
    eval(&mut session, "let x = 2");
    assert_eq!(eval(&mut session, "x"), "ok 2");
    eval(&mut session, "fn f() { return 1 }");
    eval(&mut session, "fn f() { return 2 }");
    assert_eq!(eval(&mut session, "f()"), "ok 2");
}

#[test]
fn the_session_survives_a_runtime_error() {
    let mut session = Session::with_output(Box::new(std::io::sink()));
    eval(&mut session, "let x = 42");
    eval(&mut session, "fn f(n) { return n * 2 }");

    // An error at the top level.
    assert_eq!(eval(&mut session, "1 / 0"), "err division by zero");
    assert_eq!(eval(&mut session, "x"), "ok 42");

    // An error raised inside a called function.
    assert_eq!(
        eval(&mut session, "fn bad() { return nil + 1 }\nbad()"),
        "err cannot add a nil and a int"
    );
    assert_eq!(eval(&mut session, "f(5)"), "ok 10");

    // An error raised inside a callback, several frames deep.
    assert_eq!(
        eval(&mut session, "map([1, 2], fn(v) { return v / 0 })"),
        "err division by zero"
    );
    assert_eq!(eval(&mut session, "f(x)"), "ok 84");
}

#[test]
fn the_session_survives_a_syntax_error() {
    let mut session = Session::with_output(Box::new(std::io::sink()));
    eval(&mut session, "let x = 7");
    assert!(eval(&mut session, "let = 1").starts_with("err "));
    assert!(eval(&mut session, "1 +").starts_with("err "));
    assert_eq!(eval(&mut session, "x"), "ok 7");
}

#[test]
fn a_closure_defined_later_sees_earlier_state() {
    let mut session = Session::with_output(Box::new(std::io::sink()));
    eval(&mut session, "let base = 100");
    eval(&mut session, "let add = fn(n) { return n + base }");
    assert_eq!(eval(&mut session, "add(5)"), "ok 105");
    // Reassigning the global is visible through the closure.
    eval(&mut session, "base = 200");
    assert_eq!(eval(&mut session, "add(5)"), "ok 205");
}

#[test]
fn printed_output_accumulates_across_inputs() {
    // A session shares one output sink, so what each input prints lands in order.
    let buffer = std::rc::Rc::new(std::cell::RefCell::new(Vec::<u8>::new()));
    struct Shared(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
    impl std::io::Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut session = Session::with_output(Box::new(Shared(std::rc::Rc::clone(&buffer))));
    eval(&mut session, "print(\"first\")");
    eval(&mut session, "let n = 2\nprint(\"second\", n)");
    session.flush();

    let printed = String::from_utf8(buffer.borrow().clone()).expect("utf-8");
    assert_eq!(printed, "first\nsecond 2\n");
}

#[test]
fn a_traced_error_does_not_leak_into_the_next_input() {
    // The frames an error's trace is built from are torn down as the failed
    // input unwinds. A session goes on running afterwards, so the next error
    // has to describe its own call path and not inherit the previous one.
    let mut session = Session::with_output(Box::new(std::io::sink()));

    session
        .eval("fn inner(a) { return a + 1 }\nfn outer() { return inner(nil) }")
        .expect("definitions run");

    let first = session.eval("outer()").err().expect("outer fails");
    assert_eq!(
        first
            .trace
            .iter()
            .map(|entry| entry.function.as_deref().unwrap_or("<anonymous>"))
            .collect::<Vec<_>>(),
        vec!["inner", "outer"]
    );

    // A shallower failure afterwards reports only its own frame.
    let second = session.eval("inner(nil)").err().expect("inner fails");
    assert_eq!(
        second
            .trace
            .iter()
            .map(|entry| entry.function.as_deref().unwrap_or("<anonymous>"))
            .collect::<Vec<_>>(),
        vec!["inner"]
    );

    // And a failure with no call at all carries no trace.
    let third = session.eval("nil + 1").err().expect("top level fails");
    assert!(third.trace.is_empty(), "trace was {:?}", third.trace);

    // The session is still usable.
    assert_eq!(eval(&mut session, "1 + 1"), "ok 2");
}
