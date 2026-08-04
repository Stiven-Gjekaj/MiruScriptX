<div align="center">
  <a href="README.md"><img src="assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Contributing to MiruScriptX

Thanks for your interest in MiruScriptX, a small, general-purpose scripting
language written in Rust. Contributions of all kinds are welcome: bug reports,
documentation fixes, new examples, and language features.

## Ways to contribute

- Report a bug or request a feature by opening an issue.
- Improve the documentation in `wiki/` or `docs/`.
- Add an example program in `examples/`.
- Implement a language feature or a builtin.

Before starting significant work, please open an issue to discuss it, so we can
agree on the approach before you spend time on a pull request.

## Development setup

You need a recent stable Rust toolchain (via rustup). Then:

    git clone https://github.com/stiven-gjekaj/miruscriptx
    cd miruscriptx
    cargo build

Run a program:

    cargo run -- run examples/greet.miru

Start the REPL:

    cargo run

## Where a change lives

For the change you want to make, these are the files to open. This is a map and
not a lesson: `wiki/` teaches the language, `docs/architecture.md` explains the
internals, and [`AGENTS.md`](AGENTS.md) lists the checks and the traps this
project has hit.

| Change | Files |
| ------ | ----- |
| A builtin | `src/builtins.rs` (the function, `register` with the right `define`, `BUILTIN_NAMES`), `docs/specification.md` section 8, `docs/stability.md` section 2.3, `wiki/13-builtins.md` |
| A host capability | the trait and its refusing default in `src/value.rs`, the field and setter on `Vm`, the parameter on `run_source_from` in `src/lib.rs`, the real one in `src/main.rs`, the browser's in `playground/src/lib.rs`, `docs/architecture.md` |
| Syntax | `src/lexer.rs` or `src/parser.rs`, `src/ast.rs`, `src/compiler.rs`, `src/formatter.rs`, `docs/specification.md` sections 2 and 3 |
| A new opcode | `src/chunk.rs` (the enum, the `OPCODES` table, the disassembler), `src/compiler.rs`, `src/vm.rs` |
| An error message | wherever it is raised, and `docs/specification.md` **only if that message is quoted there**. Section 3.1 of `docs/stability.md` leaves the words of a message free |
| How a value prints | `quoted_string` in `src/value.rs`, which both the formatter and printing use, and `docs/specification.md` section 4.4 |
| A command line option | `src/main.rs`, `docs/specification.md` section 10, `docs/stability.md` section 2.4, `wiki/02-getting-started.md`, `tests/integration.rs`, `README.md` |
| An example | `examples/`, `tests/integration.rs`, `README.md`, and `EXAMPLES` in `playground/src/lib.rs` unless it needs input, a file, or an import |
| A wiki lesson | `wiki/`, then `./scripts/build_reference.sh` |

### And for nearly every change

- **A change in behaviour needs a golden case** in `tests/golden.rs`, which
  holds source against output.
- **Add your entry to `CHANGELOG.md` under `## Unreleased`.** A commit carries
  no version prefix and changes no version. The version in `Cargo.toml` moves
  only when something is released.
- **Adding a test moves the badge in `README.md`.** `tests/documentation.rs`
  checks it, along with the line counts, the file count, and the playground's
  size. Its failure message gives the number to use.
- **Code and its tests go in one commit. Documentation goes in its own.**

### Four builtins, not one kind

`define` is the ordinary kind, and almost certainly the one you want.
`define_system` is for a builtin that needs the file system, and it is refused
by default so that the browser playground and any embedder get a sentence
rather than file access. `define_ambient` is for one that reads something the
program's own source does not determine, such as the clock; it is refused by
default for the same reason. `define_host` is for one that calls back into user
code, such as `map`. Copying the wrong one is a mistake you find out about
late.

Adding one also moves **counts that are different numbers**: how many take a
`BuiltinFn`, how many take an `AmbientFn`, how many reach `call_native`, and how
many exist. `builtin_kind_counts_match_the_comments_that_quote_them` in
`src/builtins.rs` holds all of them, and the prose that quotes each one is named
in its failure message. Do not assume they move together. Two correct sentences
were once lined up to be "corrected" into wrong ones, and 1.5 added a builtin
without changing the first of these numbers at all.

`tests/specification.rs` checks `BUILTIN_NAMES` against the specification in
both directions, so it names the builtin whose documentation you missed.

### Four rules that have caught somebody

- **`docs/language-reference.md` is generated.** Edit `wiki/` and run
  `./scripts/build_reference.sh`. Never edit it by hand.
- **`cargo test --workspace` is the build check, never `cargo build`.** `build`
  does not compile a `#[cfg(test)]` module, so a broken test helper passes it
  and fails later under clippy.
- **The WebAssembly check is part of the gate**, not an extra:
  `cargo clippy --target wasm32-unknown-unknown -p miruscriptx-playground -- -D warnings`.
  A pointer is four bytes there and eight natively, so code that assumes
  otherwise passes every native check and breaks the playground.
- **[`docs/stability.md`](docs/stability.md) says what is promised.** A 1.x
  release can add a builtin or add syntax. It cannot remove either, and it
  cannot change what one means.

## Before you open a pull request

Every change must keep the project green. Run these locally, exactly as CI does:

    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test

- `cargo fmt` keeps formatting consistent; CI fails on any diff.
- `cargo clippy` runs with warnings denied. Fix warnings rather than allowing
  them, unless there is a clear reason.
- `cargo test` runs the unit tests (next to each module) and the end-to-end
  tests in `tests/`. Add tests for anything you change.

If you change how programs are evaluated, add cases to `tests/golden.rs`, which
pairs each program with the exact outcome it must produce, including the line
and column an error points at. Write those expectations as literals: a test that
regenerates its expected value cannot fail, and so cannot catch a regression.

Benchmarks live in `benches/`; run them with `cargo bench`. Read the module docs
there before drawing a conclusion from one. The harness has a noise floor of
roughly four percent that looks exactly like a real result, so a change under
five percent is unmeasured rather than small.

## Coding style

- Match the surrounding code. The project favors small, focused functions and
  clear names over cleverness.
- Add dependencies sparingly. MiruScriptX keeps a small, curated dependency set:
  rustyline for REPL history at runtime, and criterion for benchmarks as a
  dev-dependency (it never ships to anyone running a MiruScriptX program). The
  README badge counts both, so a viewer sees the real total. A pull request that
  adds a dependency should justify the need and prefer the standard library
  where practical.
- Write documentation and comments in plain prose. Do not use em-dashes or
  emoji in source, docs, commit messages, or examples.

## Where things live

`docs/architecture.md` is the tour of the pipeline: lexer, parser, compiler,
virtual machine, builtins. Read it when you want to know how something works.
The section above answers the other question, which is where to start looking.

## Commit messages and pull requests

- Write clear, present-tense commit subjects that describe the change.
- Keep each commit focused on one logical change.
- In your pull request, describe what changed and why, and note how you tested
  it. If it changes the language, update the relevant `wiki/` lesson and
  regenerate the reference with `scripts/build_reference.sh`.

## Reporting security issues

Please do not open a public issue for a security problem. See
[SECURITY.md](SECURITY.md) for how to report it privately.

## Code of conduct

By taking part in this project you agree to abide by the
[Code of Conduct](CODE_OF_CONDUCT.md).
