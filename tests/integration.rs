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

fn run_eval(option: &str, source: &str) -> std::process::Output {
    miru()
        .arg(option)
        .arg(source)
        .output()
        .expect("failed to launch miru")
}

#[test]
fn eval_option_runs_a_program_without_a_file() {
    for option in ["-e", "--eval"] {
        let output = run_eval(option, "print(6 * 7)");
        assert!(output.status.success(), "stderr was {:?}", output.stderr);
        assert_eq!(output.stdout, b"42\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn eval_option_reports_errors_without_inventing_a_file() {
    let output = run_eval("-e", "print(missing)");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("undefined variable 'missing'"),
        "stderr was {stderr}"
    );
    assert!(
        !stderr.contains(".miru"),
        "inline programs have no file: {stderr}"
    );
}

#[test]
fn eval_option_rejects_missing_program_and_imports() {
    let missing = miru().arg("-e").output().expect("failed to launch miru");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("the '-e' command needs a program"));

    let import = run_eval("-e", "import \"./module.miru\" as module");
    assert!(!import.status.success());
    assert!(String::from_utf8_lossy(&import.stderr)
        .contains("cannot import: this program was not loaded from a file"));
}

/// Write a set of `(name, source)` files into a fresh directory and run the
/// first through the binary, returning `(stdout, stderr)`.
///
/// Imports resolve against real paths, so these need real files. A directory
/// per test keeps them from colliding when the suite runs in parallel.
fn run_module_set(dir: &str, files: &[(&str, &str)]) -> (String, String) {
    let root = std::env::temp_dir().join(format!("miru_modules_{dir}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("create temp dir");
    for (name, source) in files {
        std::fs::write(root.join(name), source).expect("write module");
    }
    let output = miru()
        .arg("run")
        .arg(root.join(files[0].0))
        .output()
        .expect("failed to launch the miru binary");
    let _ = std::fs::remove_dir_all(&root);
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn an_import_binds_a_module_and_each_file_keeps_its_own_names() {
    // `total` is defined in both files. Before v0.8 that was impossible to
    // write, because every name shared one table.
    let (out, err) = run_module_set(
        "basic",
        &[
            (
                "main.miru",
                "import \"./math.miru\" as math\nlet total = 1\nprint(math.add(2, 3))\nprint(math.total)\nprint(total)\n",
            ),
            (
                "math.miru",
                "let total = 100\nfn add(a, b) {\n  return a + b\n}\n",
            ),
        ],
    );
    assert_eq!(err, "", "stderr was: {err}");
    assert_eq!(out, "5\n100\n1\n");
}

#[test]
fn a_module_imported_twice_runs_once() {
    // Reached through two different files, and by two spellings of one path, so
    // this checks the cache and the canonicalising together. A module with a
    // side effect running twice is the failure nobody can explain later.
    let (out, err) = run_module_set(
        "diamond",
        &[
            (
                "diamond.miru",
                "import \"./a.miru\" as a\nimport \"./b.miru\" as b\nprint(a.via + b.via)\n",
            ),
            ("a.miru", "import \"./shared.miru\" as s\nlet via = s.n\n"),
            (
                "b.miru",
                "import \"./sub/../shared.miru\" as s\nlet via = s.n\n",
            ),
            ("shared.miru", "print(\"shared ran\")\nlet n = 7\n"),
        ],
    );
    assert_eq!(err, "", "stderr was: {err}");
    assert_eq!(out, "shared ran\n14\n");
}

#[test]
fn an_import_cycle_reports_its_chain() {
    let (_, err) = run_module_set(
        "cycle",
        &[
            ("x.miru", "import \"./y.miru\" as y\nlet a = 1\n"),
            ("y.miru", "import \"./x.miru\" as x\nlet b = 2\n"),
        ],
    );
    assert!(
        err.contains("import cycle: ./y.miru -> ./x.miru -> ./y.miru"),
        "stderr was: {err}"
    );
}

#[test]
fn a_module_error_names_the_module_and_not_the_importing_file() {
    // Three deep, so the file named has to be the one the error is actually in
    // rather than the one at the top of the chain. And no caret: the position
    // belongs to c3.miru, and what the binary holds is a3.miru's text.
    let (_, err) = run_module_set(
        "nested",
        &[
            ("a3.miru", "import \"./b3.miru\" as b\n"),
            ("b3.miru", "import \"./c3.miru\" as c\n"),
            ("c3.miru", "let z = nope\n"),
        ],
    );
    assert!(
        err.contains("error (./c3.miru, line 1, column 9): undefined variable 'nope'"),
        "stderr was: {err}"
    );
    assert!(
        !err.contains('^'),
        "drew a caret on the wrong source: {err}"
    );
}

#[test]
fn a_runtime_error_inside_a_module_names_it_and_draws_no_caret() {
    // v0.8 got this right for errors raised while a module *loads*. An error
    // raised later, when a function the module defined is finally called, was
    // still reported against the importing file: the position belonged to the
    // module and the source did not, so the caret landed on whatever happened
    // to be on that line. Here that would be `let x = 1`, which is innocent.
    let (_, err) = run_module_set(
        "runtime_in_module",
        &[
            (
                "main.miru",
                "import \"./m.miru\" as m\nlet x = 1\nlet y = 2\nprint(m.boom(4))\n",
            ),
            ("m.miru", "fn boom(n) {\n  return n / 0\n}\n"),
        ],
    );
    assert!(
        err.contains("error (./m.miru, line 2, column 12): division by zero"),
        "stderr was: {err}"
    );
    assert!(
        !err.contains("let x = 1"),
        "drew a caret on the importing file: {err}"
    );
    // The trace still works across the boundary.
    assert!(
        err.contains("in boom, called from line 4"),
        "stderr was: {err}"
    );
}

#[test]
fn a_caught_error_from_a_module_knows_which_file_it_came_from() {
    let (out, err) = run_module_set(
        "caught_from_module",
        &[
            (
                "main.miru",
                "import \"./m.miru\" as m\nlet r = try m.boom(4)\nprint(r.file)\nprint(r.line)\n",
            ),
            ("m.miru", "fn boom(n) {\n  return n / 0\n}\n"),
        ],
    );
    assert_eq!(err, "", "stderr was: {err}");
    assert_eq!(out, "./m.miru\n2\n");
}

#[test]
fn shadowing_a_builtin_in_one_file_does_not_reach_into_another() {
    // The failure this replaced: `let print = 1` in main.miru wrote over the
    // builtin's slot, builtin slots are shared by every module, and so the
    // module's own `print` call raised "a int is not callable" -- pointing at
    // a line in a file that had done nothing wrong.
    let (out, err) = run_module_set(
        "shadow_builtin",
        &[
            (
                "main.miru",
                "import \"./lib.miru\" as lib\nlet print = 1\nlib.greet(\"world\")\n",
            ),
            (
                "lib.miru",
                "fn greet(name) {\n  print(\"hello \" + name)\n  return 1\n}\n",
            ),
        ],
    );
    assert_eq!(err, "", "stderr was: {err}");
    assert_eq!(out, "hello world\n");
}

#[test]
fn assigning_to_a_builtin_name_cannot_reach_into_another_file() {
    // The half `let` shadowing did not cover. A bare assignment resolved to the
    // builtin's slot, and builtin slots are shared by every module, so this
    // still broke `print` inside the imported file.
    let (_, err) = run_module_set(
        "assign_builtin",
        &[
            (
                "main.miru",
                "import \"./lib.miru\" as lib\nprint = 1\nlib.greet(\"world\")\n",
            ),
            (
                "lib.miru",
                "fn greet(name) {\n  print(\"hello \" + name)\n  return 1\n}\n",
            ),
        ],
    );
    assert!(
        err.contains("cannot assign to undefined variable 'print'"),
        "stderr was: {err}"
    );
}

#[test]
fn a_missing_module_says_so_rather_than_failing_to_canonicalise() {
    let (_, err) = run_module_set("missing", &[("m.miru", "import \"./nope.miru\" as n\n")]);
    assert!(
        err.contains("cannot import './nope.miru': no such file"),
        "stderr was: {err}"
    );
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
fn recover_example_output() {
    // The row that fails is the second of four, and the two after it still run.
    // A program that stopped at the first failure would print one line.
    let expected = "average: 6\nskipping a row: division by zero\naverage: 15\naverage: 7\nrows handled: 3 of 4\n";
    assert_eq!(run_example("recover.miru"), expected);
}

#[test]
fn shop_example_output() {
    let expected = "notebook x 2\npen x 5\nsubtotal: 1300\ntax: 8 percent\ntotal: 1404\n";
    assert_eq!(run_example("shop.miru"), expected);
}

#[test]
fn the_prices_module_run_on_its_own_is_an_ordinary_program() {
    // A module is not a special kind of file. It defines things and prints
    // nothing, and running it directly has to work like any other program.
    assert_eq!(run_example("prices.miru"), "");
}

#[test]
fn a_module_resolves_against_the_importing_file_not_the_working_directory() {
    // The same example, launched from a directory with no examples/ in it.
    // `import "./prices.miru"` has to mean the file beside shop.miru rather
    // than one beside the shell.
    let program = std::fs::canonicalize("examples/shop.miru").expect("the example is there");
    let output = miru()
        .arg("run")
        .arg(program)
        .current_dir(std::env::temp_dir())
        .output()
        .expect("failed to launch the miru binary");
    assert!(
        output.status.success(),
        "running from elsewhere failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("total: 1404"),
        "stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
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
fn disasm_prints_bytecode_for_a_program() {
    let output = miru()
        .arg("disasm")
        .arg("examples/greet.miru")
        .output()
        .expect("runs");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");

    // The top-level script, then each function nested inside it.
    assert!(stdout.contains("== script =="), "stdout was:\n{stdout}");
    assert!(stdout.contains("== fn greet =="), "stdout was:\n{stdout}");
    // Instructions carry the source line they came from.
    assert!(stdout.contains("CLOSURE"), "stdout was:\n{stdout}");
    assert!(stdout.contains("GET_LOCAL"), "stdout was:\n{stdout}");
    // A constant shows the value it holds, not just its index.
    assert!(stdout.contains("(\"Hello, \")"), "stdout was:\n{stdout}");
}

#[test]
fn disasm_reports_a_syntax_error_and_fails() {
    let path = std::env::temp_dir().join("miru_integration_disasm_bad.miru");
    std::fs::write(&path, "let = 1\n").expect("write temp file");
    let output = miru().arg("disasm").arg(&path).output().expect("runs");
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

#[test]
fn a_runtime_error_underlines_the_token_it_blames() {
    let path = std::env::temp_dir().join("miru_integration_underline.miru");
    std::fs::write(&path, "let total = 1\nprint(missing)\n").expect("write temp file");
    let output = miru().arg("run").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    // Seven carets for `missing`, indented to sit underneath it. Asserting the
    // source line and the underline together is what would catch an underline
    // of the right width in the wrong place, which checking only the run of
    // carets would let through.
    assert!(
        stderr.contains("\n    print(missing)\n          ^^^^^^^\n"),
        "stderr was:\n{stderr}"
    );
}

#[test]
fn a_runtime_error_prints_the_call_path() {
    let path = std::env::temp_dir().join("miru_integration_trace.miru");
    std::fs::write(
        &path,
        "fn add(a) {\n  return a + 1\n}\nfn total() {\n  return add(nil)\n}\ntotal()\n",
    )
    .expect("write temp file");
    let output = miru().arg("run").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    // The caret still marks where it broke.
    assert!(
        stderr.contains("line 2, column 12"),
        "stderr was:\n{stderr}"
    );
    // And the trace says how it was reached, innermost first.
    let add = stderr
        .find("in add, called from line 5")
        .unwrap_or_else(|| panic!("no add frame in:\n{stderr}"));
    let total = stderr
        .find("in total, called from line 7")
        .unwrap_or_else(|| panic!("no total frame in:\n{stderr}"));
    assert!(add < total, "frames out of order in:\n{stderr}");
}

/// A directory of its own, so a test that writes cannot disturb another.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("miru-files-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the temporary directory is creatable");
    dir
}

/// Reading and writing work through the binary, which is the only thing that
/// grants a file system.
///
/// These belong here rather than in `tests/golden.rs` because a golden test
/// runs through `eval_source`, which deliberately grants nothing. That split is
/// the design: golden tests pin the refusal, and these pin the real thing.
#[test]
fn a_program_reads_and_writes_files() {
    let dir = scratch("roundtrip");
    let data = dir.join("data.txt");
    std::fs::write(&data, "hello from a file\n").expect("writable");

    let program = dir.join("prog.miru");
    std::fs::write(
        &program,
        "let text = read_file(\"data.txt\")\n\
         print(trim(text))\n\
         write_file(\"out.txt\", upper(trim(text)))\n\
         print(read_file(\"out.txt\"))\n\
         print(file_exists(\"out.txt\"))\n\
         print(file_exists(\"absent.txt\"))\n",
    )
    .expect("writable");

    // Run from inside the directory, because a relative path resolves against
    // the working directory. That is the rule this test exists to pin.
    let output = miru()
        .arg("run")
        .arg("prog.miru")
        .current_dir(&dir)
        .output()
        .expect("failed to launch miru");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr was: {err}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hello from a file\nHELLO FROM A FILE\ntrue\nfalse\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A path resolves against the working directory, not against the script.
///
/// This is the one rule that differs from `import`, so it gets a test of its
/// own rather than riding along inside another. The script sits in one
/// directory and the data in another, and the program finds the data only if
/// the working directory is what counts.
#[test]
fn a_file_path_resolves_against_the_working_directory_not_the_script() {
    let dir = scratch("cwd");
    let scripts = dir.join("scripts");
    std::fs::create_dir_all(&scripts).expect("creatable");

    // The data sits beside nothing: it is in `dir`, and the script is in
    // `dir/scripts`. A script-relative rule would look in `dir/scripts` and
    // fail.
    std::fs::write(dir.join("data.txt"), "found").expect("writable");
    std::fs::write(
        scripts.join("tool.miru"),
        "print(read_file(\"data.txt\"))\n",
    )
    .expect("writable");

    let output = miru()
        .arg("run")
        .arg("scripts/tool.miru")
        .current_dir(&dir)
        .output()
        .expect("failed to launch miru");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr was: {err}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "found\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A missing file is an ordinary error: it names the path, points a caret at
/// the call, and `try` can catch it.
#[test]
fn reading_a_missing_file_is_a_catchable_error() {
    let output = run_eval("-e", "print(read_file(\"definitely-not-here.txt\"))");
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("cannot read 'definitely-not-here.txt'"),
        "stderr was: {err}"
    );
    assert!(err.contains('^'), "the caret is missing: {err}");

    let caught = run_eval(
        "-e",
        "let r = try read_file(\"definitely-not-here.txt\")\nprint(is_error(r))",
    );
    assert!(caught.status.success());
    assert_eq!(caught.stdout, b"true\n");
}
