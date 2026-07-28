# Conventions for this repository

## Writing

Two registers, chosen by what a document is for.

**ASD-STE100 Simplified Technical English** applies to documents whose job is to
be unambiguous:

- `docs/language-reference.md` (and therefore the wiki sections it is generated
  from, once those are converted)
- The v1.0 language specification
- The v1.0 stability guarantee

The rules that bite most often: approved words only, one meaning per word,
sentences of 20 words or fewer for instructions and 25 for description, active
voice, present tense, articles always written, no metaphor or idiom, and no
noun cluster longer than three words.

**The existing narrative voice** stays in the documents whose job is to explain
why a decision was made:

- `docs/architecture.md`
- `docs/milestones.md`
- `wiki/*.md` lessons
- `README.md`
- `CHANGELOG.md` and commit messages

These carry arguments, not instructions. "A missing arm does not fail to
compile, it behaves almost right, which is worse" is the point of that
paragraph, and STE cannot say it.

### One term, one meaning

STE forbids using two words for one thing. This project currently uses **error**
and **failure** interchangeably: `Value::Error` and `type(r) == "error"` against
"Handling failure" and "a failure has no field". The specification has to pick
one and use it everywhere, and the reference has to follow. Decide this before
writing the specification, not during.

Candidate split, if a split is wanted: an **error** is what stops a program; a
**failure** is an error that `try` has caught and turned into a value. That is
close to current usage and gives each word one job.

## Code

- Every commit bumps the patch version in `Cargo.toml` and `Cargo.lock`.
- Code and the tests that prove it go in one commit. Documentation goes in its
  own.
- `docs/language-reference.md` is generated. Run `scripts/build_reference.sh`;
  never edit it by hand.
- Before pushing: `cargo fmt --all --check`, `cargo clippy --all-targets -D
  warnings`, `cargo clippy -p miruscriptx-playground --target
  wasm32-unknown-unknown`, and `cargo test --workspace`.
- A measurement is quoted as best-of-several runs, never a single pair. See the
  module comment in `benches/vm.rs` for why.

## Releases

- Milestones are developed on `beta/Version-N.M.X` and land on `main` by
  fast-forward. No merge commits.
- The repository is deliberately untagged until `v1.0.0`, which will be the
  first tag and will trigger the release workflow.
