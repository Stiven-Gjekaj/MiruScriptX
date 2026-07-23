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
    assert_eq!(run_example("greet.msx"), "Hello, Aiko!\nHello, Ken!\n");
}

#[test]
fn fib_example_output() {
    assert_eq!(run_example("fib.msx"), "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n");
}

#[test]
fn fizzbuzz_example_output() {
    let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
    assert_eq!(run_example("fizzbuzz.msx"), expected);
}

#[test]
fn contacts_example_output() {
    let expected =
        "names:\n  Aiko: 555-0100\n  Ken: 555-0199\n  Mai: 555-0177\nKen is 555-0199\nentries: 3\n";
    assert_eq!(run_example("contacts.msx"), expected);
}

#[test]
fn greeter_example_reads_stdin() {
    use std::io::Write;

    let mut child = miru()
        .arg("run")
        .arg("examples/greeter.msx")
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
        .arg("examples/does_not_exist.msx")
        .output()
        .expect("runs");
    assert!(!output.status.success());
}

#[test]
fn runtime_error_reports_line_and_fails() {
    let path = std::env::temp_dir().join("miru_integration_bad.msx");
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
