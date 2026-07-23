<div align="center">
  <a href="README.md"><picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/miru-dark.svg">
    <img src="assets/miru-light.svg" alt="MiruScriptX" height="44">
  </picture></a>
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

    cargo run -- run examples/greet.msx

Start the REPL:

    cargo run

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

## Coding style

- Match the surrounding code. The project favors small, focused functions and
  clear names over cleverness.
- Keep the interpreter dependency-free. MiruScriptX uses only the Rust standard
  library, and pull requests that add dependencies are unlikely to be merged.
- Write documentation and comments in plain prose. Do not use em-dashes or
  emoji in source, docs, commit messages, or examples.

## Where things live

See `docs/architecture.md` for a tour of the pipeline (lexer, parser,
interpreter, builtins) and notes on how to add a builtin, an operator, or a
statement.

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
