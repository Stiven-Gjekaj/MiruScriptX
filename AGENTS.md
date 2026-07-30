# Working on MiruScriptX with an AI agent

MiruScriptX is a scripting language written from scratch in Rust: lexer, Pratt
parser, bytecode compiler, stack virtual machine. This file is what an agent
needs that is not obvious from reading the code.

`CONTRIBUTING.md` is for everybody and says how to set up and what is expected.
Read that first. This file adds only the things that have actually gone wrong
here.

## The gate

Every one of these, before every commit. Not a selection.

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-unknown-unknown -p miruscriptx-playground -- -D warnings
cargo test --workspace
```

**`cargo test --workspace` is the build check, never `cargo build`.** `build`
does not compile `#[cfg(test)]` modules, so a broken test helper passes it and
fails later under clippy. This has cost time twice.

**The wasm check is not optional.** It was skipped once, for a reason that
sounded good at the time, and `main` was red for five runs. The library compiles
for `wasm32-unknown-unknown`, where a pointer is four bytes rather than eight,
and code that assumes otherwise builds fine natively.

## Things that are generated or pinned

- **`docs/language-reference.md` is generated.** Edit `wiki/*.md`, then run
  `scripts/build_reference.sh`. Never edit the reference by hand.
- **Several numbers are pinned by tests** and will fail if you change one thing
  and not the other: the builtin list and count (`tests/specification.rs`), the
  nesting limit, `size_of::<Value>()`, and the README's line counts and test
  badge (`tests/documentation.rs`). When one fails, the document is out of date,
  not the test.
- Adding a builtin touches six places. `tests/specification.rs` will name
  whichever you missed.

## What you may not change

`docs/stability.md` is a promise, not a preference. A 1.x release may add a
builtin, add syntax that leaves every valid 1.0 program meaning the same thing,
reword an error, and change speed. It may not remove or alter a builtin, change
what any syntax means, or change evaluation order.

Check a change against it before writing code, not after. One change already
shipped that broke section 2.1 and had to be undone in the next release.

## Measurement

If you touch performance, read the header of `benches/vm.rs` first. It is a lab
notebook, and most of what looks promising has already been tried and rejected
with numbers.

Three rules earned the hard way:

1. **Run the control first.** Benchmark the same binary against itself, twice.
   Whatever that reports is your noise floor. On a shared cloud host it was
   measured at fifteen percent, which makes almost any single change
   unmeasurable there.
2. **A reproducible number is not a true one.** Two runs against one stale
   baseline agree with each other perfectly and are both wrong.
3. **Check the result against the mechanism.** If a change to map reads moves a
   benchmark that never reads a map, the number is lying, whatever its p value.

## Conventions

- **Commits carry no version prefix and change no version.** Add your entry to
  `CHANGELOG.md` under `## Unreleased`. Commits before 1.0 carry a `vX.Y.Z:`
  prefix; that convention is retired, do not copy it.
- **One logical change per commit.** Code and its tests together, documentation
  separately, and a wide mechanical rename on its own with no behaviour change
  inside it.
- **Branches**: `feat/`, `bugfix/`, `perf/`, `docs/`.
- **Commit as yourself.** See "Sign your own work" below.
- **Do not open a pull request unless asked.**
- **No em-dashes** in anything written for this repository.
- `docs/specification.md`, `docs/stability.md`, and
  `docs/language-reference.md` are written in ASD-STE100 Simplified Technical
  English. Everything else keeps its ordinary voice.

## Sign your own work

**Commits are authored by you, not by the agent or the company that made it.**
Set your git identity before you start and leave it alone:

```sh
git config user.name "Your Name"
git config user.email "you@example.com"
```

An agent must not substitute its own identity, add itself as a co-author, or
append a "generated with" footer to a commit message, a pull request, or a
review comment. If your tooling does any of that by default, turn it off.

This is not modesty about how the work was done. Say so in the pull request if
you like. It is about who answers for the result. A commit is a claim that
somebody read this, understood it, and is willing to be asked about it in six
months. An agent cannot make that claim and will not be there for the question.
Putting its name in the author field moves the accountability somewhere it
cannot be collected.

The practical version: if you would not be comfortable being asked to explain a
line of it, do not commit it yet. Reviewing what an agent wrote for you is the
work, not an optional extra afterwards.

## Do not take reserved work

The open issues are deliberately sized and written for people who want to
contribute. Several are labelled good first issue. **Read the open issues before
adding a builtin or any syntax**, and if one covers what you were about to do,
leave it alone.

## What earns trust here

Claims in this repository are expected to be checked rather than reasoned to.
The habit that keeps paying:

- Ran it, rather than concluded it would work.
- When adding a test, broke the thing on purpose to confirm the test fails. A
  test that cannot fail is worse than none, because it makes something look
  guarded while it drifts.
- Said plainly when a measurement did not support the conclusion.

Four defects in this project were found by writing a specification and none by
any test. Three separate abort defects were found by asking what else walks a
structure by recursion. Both came from taking a claim seriously enough to check
the next case.
