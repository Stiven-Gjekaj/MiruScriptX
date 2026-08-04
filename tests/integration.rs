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

/// The word counter reads a file, so its test is the one that proves a path
/// resolves against the working directory. `run_example` launches the binary
/// from the repository root, which is where the example's own comment says to
/// run it from.
#[test]
fn words_example_output() {
    let expected = "different words: 23\nmost common, at 7 each:\n  the\n";
    assert_eq!(run_example("words.miru"), expected);
}

/// Run from anywhere else, it says so and stops with 1 rather than failing
/// inside `read_file` with a message about a path the reader did not write.
#[test]
fn words_example_says_where_to_run_it_from() {
    let output = miru()
        .arg("run")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/words.miru"))
        .current_dir(std::env::temp_dir())
        .output()
        .expect("failed to launch the miru binary");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).expect("output should be valid utf-8"),
        "no file at examples/words.txt\nrun this from the repository root\n"
    );
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

/// The guessing game plays out the same way every time, because it seeds the
/// generator with a literal. That is the whole reason an example can use chance
/// and still be asserted against exact output.
///
/// The guesses below are a binary search for 52, which is what `seed(2026)`
/// gives for `random_int(1, 100)` in this release. A change of generator moves
/// the secret and fails this test, which is correct: the example's own comment
/// says the seed decides the game.
#[test]
fn guess_example_plays_a_seeded_game() {
    use std::io::Write;

    let mut child = miru()
        .arg("run")
        .arg("examples/guess.miru")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to launch the miru binary");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(b"50\nnope\n75\n60\n52\n")
        .expect("write to child stdin");
    let output = child.wait_with_output().expect("wait for miru");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output should be valid utf-8"),
        "I am thinking of a number from 1 to 100.\n\
         Your guess: Higher.\n\
         Your guess: That is not a number.\n\
         Your guess: Lower.\n\
         Your guess: Lower.\n\
         Your guess: That is it, in 4 guesses.\n"
    );
}

/// With nothing on standard input the game says the answer and stops, rather
/// than looping forever on a `nil` it did not check for.
#[test]
fn guess_example_ends_at_end_of_input() {
    let output = miru()
        .arg("run")
        .arg("examples/guess.miru")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to launch the miru binary");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("output should be valid utf-8"),
        "I am thinking of a number from 1 to 100.\n\
         Your guess: The number was 52\n"
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
fn a_program_that_holds_a_unicode_escape_runs_and_formats() {
    // The whole path for `\u{...}`, end to end: it runs, it formats, and what
    // it prints is the same before and after. The escape is a way to write a
    // character, so formatting writes the character and the spelling is not
    // what survives. The value is.
    let path = std::env::temp_dir().join("miru_integration_unicode.miru");
    std::fs::write(&path, "let e = \"\\u{1F600}\"\nprint(e)\nprint(len(e))\n").expect("write");

    let before = miru().arg("run").arg(&path).output().expect("runs");
    assert!(before.status.success());
    assert_eq!(
        String::from_utf8(before.stdout).expect("utf-8"),
        "\u{1F600}\n1\n"
    );

    let first = miru()
        .arg("fmt")
        .arg("-w")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(first.status.success());
    let after_first = std::fs::read_to_string(&path).expect("read back");
    assert!(
        after_first.contains("let e = \"\u{1F600}\""),
        "file was: {after_first}"
    );

    // Formatting again changes nothing, and running the formatted file prints
    // what the original printed.
    let second = miru()
        .arg("fmt")
        .arg("-w")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(second.status.success());
    assert_eq!(after_first, std::fs::read_to_string(&path).expect("read"));

    let after = miru().arg("run").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);
    assert!(after.status.success());
    assert_eq!(
        String::from_utf8(after.stdout).expect("utf-8"),
        "\u{1F600}\n1\n"
    );
}

#[test]
fn fmt_never_writes_a_control_character_into_the_source() {
    // The defect this closes, checked on the bytes rather than on the text.
    // `miru fmt` used to write the character itself, so a file holding one
    // came back holding a byte no editor shows and a copy and paste loses.
    let path = std::env::temp_dir().join("miru_integration_control.miru");
    std::fs::write(
        &path,
        "let bell = \"\\u{7}\"\nlet nul = \"\\0\"\nprint(len(bell) + len(nul))\n",
    )
    .expect("write temp file");

    let first = miru()
        .arg("fmt")
        .arg("-w")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(first.status.success());

    let bytes = std::fs::read(&path).expect("read back");
    let stray: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|b| (*b < 0x20 && *b != b'\n') || *b == 0x7F)
        .collect();
    assert!(
        stray.is_empty(),
        "the formatted file holds control bytes {stray:?}: {}",
        String::from_utf8_lossy(&bytes)
    );

    // Formatting again changes nothing, which says the escapes it writes are
    // ones it also reads back.
    let after_first = std::fs::read_to_string(&path).expect("read back");
    let second = miru()
        .arg("fmt")
        .arg("-w")
        .arg(&path)
        .output()
        .expect("runs");
    assert!(second.status.success());
    assert_eq!(after_first, std::fs::read_to_string(&path).expect("read"));

    // And the program still does what it did, so nothing was lost on the way.
    let run = miru().arg("run").arg(&path).output().expect("runs");
    let _ = std::fs::remove_file(&path);
    assert!(run.status.success());
    assert_eq!(String::from_utf8(run.stdout).expect("utf-8"), "2\n");
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

/// Everything after the file path belongs to the program.
#[test]
fn a_program_sees_the_arguments_it_was_given() {
    let dir = scratch("args");
    std::fs::write(
        dir.join("tool.miru"),
        "let a = args()\nprint(len(a))\nprint(join(a, \"|\"))\n",
    )
    .expect("writable");

    let output = miru()
        .arg("run")
        .arg("tool.miru")
        // The second of these looks like an option. It is not one: it is after
        // the path, so it belongs to the program, and `miru` must not try to
        // interpret it.
        .args(["alpha", "--verbose", "beta"])
        .current_dir(&dir)
        .output()
        .expect("failed to launch miru");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr was: {err}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "3\nalpha|--verbose|beta\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_program_given_no_arguments_sees_an_empty_array() {
    let output = run_eval("-e", "print(len(args()))");
    assert!(output.status.success(), "stderr was {:?}", output.stderr);
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn the_eval_option_passes_its_trailing_arguments_to_the_program() {
    let output = miru()
        .arg("-e")
        .arg("print(join(args(), \",\"))")
        .args(["one", "two"])
        .output()
        .expect("failed to launch miru");
    assert!(output.status.success(), "stderr was {:?}", output.stderr);
    assert_eq!(output.stdout, b"one,two\n");
}

/// An option before the path is still `miru`'s, and an unknown one is refused.
#[test]
fn an_unknown_option_before_the_path_is_still_refused() {
    let output = miru()
        .arg("run")
        .arg("--nonsense")
        .arg("whatever.miru")
        .output()
        .expect("failed to launch miru");
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("unknown option '--nonsense'"), "stderr: {err}");
}

/// An exit code is a property of the process, so these need the real binary.
/// `run_capture` cannot see one and neither can a golden test.
#[test]
fn exit_sets_the_process_exit_code() {
    for code in [0, 1, 2, 42, 255] {
        let output = run_eval("-e", &format!("exit({code})"));
        assert_eq!(
            output.status.code(),
            Some(code),
            "exit({code}) gave {:?}",
            output.status
        );
    }
}

/// A program that never asks still ends with 0, and one that fails still ends
/// with 1. Those two were the whole set before `exit` existed, and the
/// guarantee says they keep their meanings.
#[test]
fn the_two_original_exit_codes_still_mean_what_they_meant() {
    assert_eq!(run_eval("-e", "print(1)").status.code(), Some(0));
    assert_eq!(run_eval("-e", "undefined_name").status.code(), Some(1));
}

/// Stopping must not throw away what the program already said.
///
/// An exit leaves the dispatch loop as an error, and the error path did not
/// flush before this release, so a program that printed and then exited lost
/// its output. That is the same defect 1.1 fixed for an abort.
#[test]
fn output_survives_an_exit() {
    let output = run_eval("-e", "print(\"kept\")\nexit(3)");
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "kept\n");
}

/// The two streams are two streams, checked where it actually matters: at the
/// file descriptors a shell would redirect.
#[test]
fn eprint_goes_to_standard_error_and_print_to_standard_output() {
    let output = run_eval("-e", "print(\"result\")\neprint(\"diagnostic\")");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "result\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "diagnostic\n");
}

/// `miru run` carries a code the same way `miru -e` does. Two entry points,
/// one behaviour, and nothing in between them that could differ.
#[test]
fn a_file_carries_an_exit_code_too() {
    let dir = std::env::temp_dir().join("miru-exit-code-test");
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let path = dir.join("stop.miru");
    std::fs::write(&path, "print(\"working\")\nexit(9)\n").expect("write the program");

    let output = miru()
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to launch miru");

    assert_eq!(output.status.code(), Some(9));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "working\n");
    std::fs::remove_dir_all(&dir).ok();
}

/// `miru` gives a program a clock, so `now` answers rather than refusing.
///
/// The bound is the only assertion a test of a real clock can make. It is the
/// millisecond value of 2025-06-15, which is before this test was written and
/// after every machine that could plausibly run it was built, so the test says
/// the number came from a clock and not from a zero.
#[test]
fn the_binary_gives_a_program_a_clock() {
    let output = run_eval("-e", "print(now() > 1750000000000)");
    assert!(output.status.success(), "stderr was {:?}", output.stderr);
    assert_eq!(output.stdout, b"true\n");
}

/// The clock is a whole capability rather than one builtin, so the two entry
/// points that run a program both supply it. A `run` that had no clock would be
/// a difference between `miru run` and `miru -e` that nothing else has.
#[test]
fn running_a_file_gives_a_clock_too() {
    let dir = std::env::temp_dir().join("miru-clock-test");
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let path = dir.join("when.miru");
    std::fs::write(&path, "print(type(now()))\n").expect("write the program");

    let output = miru()
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to launch miru");

    assert!(output.status.success(), "stderr was {:?}", output.stderr);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "int\n");
    std::fs::remove_dir_all(&dir).ok();
}
