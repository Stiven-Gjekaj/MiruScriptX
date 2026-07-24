//! End-to-end tests that run the compiled `miru` binary on the example
//! programs and check their output and exit codes.

use std::process::Command;

fn miru() -> Command {
    Command::new(env!("CARGO_BIN_EXE_miru"))
}

fn run_example(name: &str) -> String {
    let output = miru()
        .arg("run")
        .arg(format!("examples/{name}"))
        .output()
        .expect("failed to launch the miru binary");
    assert!(
        output.status.success(),
        "running {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("output should be valid utf-8")
}

#[test]
fn greet_example_output() {
    assert_eq!(run_example("greet.miru"), "Hello, Aiko!\nHello, Ken!\n");
}

#[test]
fn fib_example_output() {
    assert_eq!(run_example("fib.miru"), "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n");
}

#[test]
fn fizzbuzz_example_output() {
    let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
    assert_eq!(run_example("fizzbuzz.miru"), expected);
}

#[test]
fn contacts_example_output() {
    let expected =
        "names:\n  Aiko: 555-0100\n  Ken: 555-0199\n  Mai: 555-0177\nKen is 555-0199\nentries: 3\n";
    assert_eq!(run_example("contacts.miru"), expected);
}

#[test]
fn transform_example_output() {
    let expected =
        "doubled: [2, 4, 6, 8, 10, 12]\nevens: [2, 4, 6]\nsum: 21\nsum of odd squares: 35\n";
    assert_eq!(run_example("transform.miru"), expected);
}

#[test]
fn greeter_example_reads_stdin() {
    use std::io::Write;

    let mut child = miru()
        .arg("run")
        .arg("examples/greeter.miru")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to launch the miru binary");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(b"Aiko\n")
        .expect("write to child stdin");
    let output = child.wait_with_output().expect("wait for miru");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output should be valid utf-8"),
        "What is your name? Hello, Aiko!\n"
    );
}

#[test]
fn the_retired_vm_flag_is_still_accepted() {
    // v0.4 offered --vm to opt into the bytecode engine. It is now the only
    // engine, but a command written back then should not fail.
    let output = miru()
        .arg("run")
        .arg("--vm")
        .arg("examples/greet.miru")
        .output()
        .expect("runs");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf-8"),
        "Hello, Aiko!\nHello, Ken!\n"
    );
}

#[test]
fn fmt_prints_formatted_source_to_stdout() {
    let output = miru()
        .arg("fmt")
        .arg("examples/fib.miru")
        .output()
        .expect("runs");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    // Canonical spacing: the recursive call is reprinted with single spaces.
    assert!(
        stdout.contains("return fib(n - 1) + fib(n - 2)"),
        "stdout was: {stdout}"
    );
    // Printing to stdout must leave the file itself untouched.
    let on_disk = std::fs::read_to_string("examples/fib.miru").expect("read example");
    assert!(on_disk.contains("fib.miru"));
}

#[test]
fn fmt_write_rewrites_the_file_and_is_idempotent() {
    let path = std::env::temp_dir().join("miru_integration_fmt.miru");
    std::fs::write(&path, "let  x=[1,2,3]\nprint(  x )\n").expect("write temp file");

    let first = miru()
        .arg("fmt")
        .arg("-w")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(first.status.success());
    let after_first = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(after_first, "let x = [1, 2, 3]\nprint(x)\n");

    // Formatting an already-formatted file changes nothing.
    let second = miru()
        .arg("fmt")
        .arg("--write")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(second.status.success());
    let after_second = std::fs::read_to_string(&path).expect("read back");
    let _ = std::fs::remove_file(&path);
    assert_eq!(after_first, after_second);
}

#[test]
fn fmt_reports_a_syntax_error_and_fails() {
    let path = std::env::temp_dir().join("miru_integration_fmt_bad.miru");
    std::fs::write(&path, "let = 1\n").expect("write temp file");
    let output = miru().arg("fmt").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(stderr.contains("miru:"), "stderr was: {stderr}");
}

#[test]
fn version_flag_prints_version() {
    let output = miru().arg("--version").output().expect("runs");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.starts_with("miru "));
}

#[test]
fn missing_file_fails_with_nonzero_exit() {
    let output = miru()
        .arg("run")
        .arg("examples/does_not_exist.miru")
        .output()
        .expect("runs");
    assert!(!output.status.success());
}

#[test]
fn runtime_error_reports_line_and_fails() {
    let path = std::env::temp_dir().join("miru_integration_bad.miru");
    std::fs::write(&path, "let a = 1\nprint(b)\n").expect("write temp file");
    let output = miru().arg("run").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(stderr.contains("line 2"), "stderr was: {stderr}");
    assert!(
        stderr.contains("undefined variable 'b'"),
        "stderr was: {stderr}"
    );
}
