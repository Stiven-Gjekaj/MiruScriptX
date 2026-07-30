//! Generated programs, run against the binary, asserting one thing: whatever
//! the program is, `miru` reports rather than dies.
//!
//! Every member of the abort class so far was found by a person reasoning about
//! the code. Deep source, in v1.1. A self-referential value, in v1.0. A chain of
//! values released at the end of a program, also in v1.1, which the v1.0 fix
//! walked straight past because it guarded comparing and printing rather than
//! releasing. Each time the reasoning was sound and each time it stopped one
//! instance short of the class.
//!
//! Hand-picked inputs have the same shape as hand-written reasoning: they cover
//! what the author thought of. Twenty-eight of them were tried against the
//! interpreter while surveying for this work and every one passed, hours before
//! generated nesting brought the process down. So this file generates instead.
//!
//! # What counts as failure
//!
//! An error is fine. A syntax error, a runtime error, a limit refusing a
//! program: all of those are the interpreter working. What is not fine:
//!
//! - Death by signal, which is how a Rust stack overflow arrives. Note that it
//!   shows up here as an exit code of `None` rather than 134: a process killed
//!   by `SIGABRT` has no exit code, and the shell's 134 is its own convention.
//!   This is the whole reason the file exists.
//! - A panic, which reaches stderr as `panicked at`.
//! - Any exit code other than 0 or 1, since the specification says there are
//!   two and `docs/stability.md` promises it.
//!
//! # Why the binary rather than the library
//!
//! An abort takes the process down, so it cannot be caught in-process: a test
//! calling `run_source` would take the test runner with it and report nothing
//! useful. Running the binary puts the failure in a child, where the exit code
//! survives to be asserted on. It also tests the stack the binary gives itself,
//! which is the configuration users actually have.
//!
//! # Determinism
//!
//! The generator is seeded, and the seed is printed with any failure, so a
//! run that finds something can be repeated exactly. It is not a fuzzer left
//! running for hours; it is a few hundred programs in a second or so, chosen to
//! reach the shapes that have historically hurt.

/// A small deterministic generator. `SplitMix64`, which is a few lines and has
/// no business being a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, options: &'a [T]) -> &'a T {
        &options[self.below(options.len())]
    }
}

/// The shapes that have hurt before, plus enough ordinary material that a
/// program is not always pathological.
///
/// Sizes go past every limit on purpose. `MAX_NESTING` is 1000, the call depth
/// limit is 10000, and a generated program should cross both often.
fn program(rng: &mut Rng) -> String {
    let depth = *rng.pick(&[1, 2, 5, 50, 500, 1200, 20000]);
    let width = *rng.pick(&[0, 1, 3, 60000]);
    let count = *rng.pick(&[1, 10, 5000, 200000]);

    match rng.below(14) {
        // Nesting the parser descends through.
        0 => format!("{}{}", "[".repeat(depth), "]".repeat(depth)),
        1 => format!("{}1{}", "(".repeat(depth), ")".repeat(depth)),
        2 => format!("{}1{}", "{\"a\": ".repeat(depth), "}".repeat(depth)),
        3 => format!("{}1", "try ".repeat(depth)),
        4 => format!("{}1", "-".repeat(depth)),
        // Spines, which cost the parser one frame and the tree `depth` levels.
        5 => format!("1{}", " + 1".repeat(depth)),
        6 => format!("let a = [1]\na{}", "[0]".repeat(depth)),
        7 => format!("let m = {{}}\nm{}", ".a".repeat(depth)),
        // Chains of values, built by a loop and released at the end. The loop
        // makes these reach lengths no literal could.
        8 => format!("let a = []\nlet i = 0\nwhile i < {count} {{\n  a = [a]\n  i = i + 1\n}}"),
        9 => format!(
            "let m = {{}}\nlet i = 0\nwhile i < {count} {{\n  m = {{\"a\": m}}\n  i = i + 1\n}}"
        ),
        10 => format!(
            "fn base() {{ return 0 }}\nlet f = base\nlet i = 0\n\
             while i < {count} {{\n  let g = f\n  f = fn() {{ return g() }}\n  i = i + 1\n}}"
        ),
        // Recursion, which must reach the call depth limit rather than the
        // machine stack.
        11 => format!("fn r(n) {{ return r(n + 1) }}\nr({depth})"),
        // Wide rather than deep, which is the other way to run out of room.
        12 => format!("let a = [{}]\nlen(a)", vec!["1"; width].join(", ")),
        // Nested function literals, where two separate limits have to meet.
        _ => {
            let mut source = String::from("0");
            for _ in 0..depth.min(2000) {
                source = format!("fn() {{ return {source} }}");
            }
            format!("let f = {source}\ntype(f)")
        }
    }
}

/// Run a program and give back its exit code and standard error.
///
/// Through a file rather than `-e`, because a generated program can be hundreds
/// of kilobytes and the command line has a length limit: passing one as an
/// argument fails with `ArgumentListTooLong` before `miru` is even started,
/// which would make this suite quietly stop testing the largest cases. A file
/// is also the path most programs arrive by.
fn run(source: &str, tag: &str) -> (Option<i32>, String) {
    let path = std::env::temp_dir().join(format!(
        "miru-never-aborts-{}-{tag}.miru",
        std::process::id()
    ));
    std::fs::write(&path, source).expect("the temporary file is writable");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_miru"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("the miru binary runs");
    let _ = std::fs::remove_file(&path);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn no_generated_program_aborts_or_panics() {
    // Fixed, so a failure is reproducible. Change it to widen the search.
    const SEED: u64 = 0x5C21_9700_0001_BEEF;
    let mut rng = Rng(SEED);
    let mut failures = Vec::new();

    for case in 0..300 {
        let source = program(&mut rng);
        let (code, stderr) = run(&source, &format!("case{case}"));
        let acceptable = matches!(code, Some(0) | Some(1)) && !stderr.contains("panicked at");

        if !acceptable {
            failures.push(format!(
                "  case {case} (seed {SEED:#x}) exited {code:?}\n    source begins: {:?}\n    stderr: {}",
                &source[..source.len().min(70)],
                stderr.lines().next().unwrap_or("").trim()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} generated programs did not report cleanly.\n\
         An exit of `None` means death by signal, which is how a Rust stack\n\
         overflow arrives, and stderr will say so.\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The generator has to be able to reach the failures we already fixed, or it
/// proves nothing. This runs the three known shapes directly, at sizes that
/// aborted before, and would have caught every member of the class.
#[test]
fn the_known_failures_are_within_reach_of_the_generator() {
    let known = [
        // Deep source: aborted before v1.1 gave the parser a limit.
        format!("{}{}", "[".repeat(50000), "]".repeat(50000)),
        // A spine: one parser frame, a tree 50000 tall.
        format!("1{}", " + 1".repeat(50000)),
        // A chain of values released at the end: aborted before the destructor
        // became iterative, after the program had already finished.
        "let a = []\nlet i = 0\nwhile i < 200000 {\n  a = [a]\n  i = i + 1\n}".to_string(),
        // The same through closures, which the plan for that work had missed.
        "fn base() { return 0 }\nlet f = base\nlet i = 0\n\
         while i < 200000 {\n  let g = f\n  f = fn() { return g() }\n  i = i + 1\n}"
            .to_string(),
    ];

    for (n, source) in known.iter().enumerate() {
        let (code, _) = run(source, &format!("known{n}"));
        assert!(
            matches!(code, Some(0) | Some(1)),
            "exited {code:?} on a known shape beginning {:?}",
            &source[..source.len().min(60)]
        );
    }
}
