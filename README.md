<div align="center">

<img src="assets/Miru.png" alt="MiruScriptX" width="300">

### A small, general-purpose scripting language, written in Rust

_A tree-walking interpreter with zero dependencies: source -&gt; tokens -&gt; AST -&gt; values_

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.94%2B-CE422B?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"/>
  <img src="https://img.shields.io/badge/std_only-zero_deps-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Standard library only"/>
  <img src="https://img.shields.io/badge/tests-120_passing-427819?style=for-the-badge" alt="120 tests passing"/>
</p>

<p align="center">
  <a href="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml"><img src="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <img src="https://img.shields.io/badge/version-0.2-blue?style=flat-square" alt="Version 0.2"/>
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="MIT License"/>
</p>

<p align="center">
  <a href="#quick-start"><b>Quick Start</b></a> |
  <a href="#features"><b>Features</b></a> |
  <a href="#examples"><b>Examples</b></a> |
  <a href="#documentation"><b>Documentation</b></a>
</p>

</div>

---

## Overview

**MiruScriptX** is a minimalist, dynamically typed scripting language with a
clean, modern syntax. It runs through a tree-walking interpreter written from
scratch in Rust, using only the standard library. Write functions, closures,
loops, arrays, and maps in familiar syntax, then run them from a file or an
interactive REPL. Programs use the `.msx` extension.

```
fn greet(name) {
  return "Hello, " + name + "!"
}

let people = ["Aiko", "Ken"]
for name in people {
  print(greet(name))
}
```

---

## Features

<table>
<tr>
<td width="50%" valign="top">

### Language

- Integers, floats, booleans, strings, nil
- Arrays and maps, with indexing
- Functions, closures, and recursion
- `if` / `else if` / `else`, `while`, `for ... in`
- `break` and `continue`
- Reading input with `input`
- Arithmetic, comparison, and short-circuit logic

</td>
<td width="50%" valign="top">

### Interpreter and tooling

- Lexer, Pratt parser, tree-walking evaluator
- A standard library of string, array, math, map, and I/O builtins
- File runner and interactive REPL
- Errors with a line, a column, and a caret under the problem
- Zero dependencies (Rust standard library only)
- Unit and integration tests, plus CI

</td>
</tr>
</table>

---

## Quick Start

Build the interpreter (a recent stable Rust toolchain is all you need):

```
cargo build --release
```

Run a program from a file:

```
miru run examples/greet.msx
```

Or start the REPL and type expressions:

```
miru
```

```
miru> let x = 21
miru> x * 2
42
```

For a step-by-step guide, start the wiki at
[wiki/01-introduction.md](wiki/01-introduction.md).

---

## Examples

Runnable programs live in [examples/](examples):

| Program | Shows off |
| ------- | --------- |
| [greet.msx](examples/greet.msx) | Functions, arrays, and a loop |
| [fib.msx](examples/fib.msx) | Recursion |
| [fizzbuzz.msx](examples/fizzbuzz.msx) | Control flow and the modulo operator |
| [contacts.msx](examples/contacts.msx) | Maps, lookups, and iteration |

Run one with `miru run examples/contacts.msx`.

---

## Language at a glance

```
// Maps, loops, and loop control
let book = {"Aiko": "555-0100", "Ken": "555-0142"}
book["Mai"] = "555-0177"

for name in keys(book) {
  if name == "Ken" { continue }
  print(name + ": " + book[name])
}
```

See the [language reference](docs/language-reference.md) for the whole language
on one page.

---

## Project structure

MiruScriptX is a classic pipeline: source becomes tokens, tokens become an
abstract syntax tree, and the tree is evaluated directly.

| Stage | Files | Lines | Responsibility |
| ----- | ----- | ----- | -------------- |
| **Lexer** | token.rs, lexer.rs | 668 | Source text to tokens, with line and column tracking |
| **Parser** | ast.rs, parser.rs | 1031 | Recursive descent plus a Pratt expression parser |
| **Interpreter** | value, environment, interpreter, builtins | 1969 | Tree-walking evaluation, scopes, closures, builtins |
| **CLI and REPL** | main.rs, repl.rs | 173 | File runner and interactive REPL |
| **Library** | lib.rs | 265 | Ties it together (`parse_program`, `run_source`) |
| **Total** | **11 files** | **4106** | Zero-dependency interpreter |

```
src/         the interpreter (lexer, parser, evaluator, builtins, CLI, REPL)
examples/    runnable .msx programs
wiki/        step-by-step learning lessons
docs/        language reference, architecture, and roadmap
tests/       end-to-end integration tests
scripts/     build_reference.sh regenerates the single-page reference
```

---

## Documentation

<table>
<tr>
<td align="center" width="25%" valign="top">
<h3>Learn</h3>
<p>A guided tour,<br/>read in order</p>
<a href="wiki/01-introduction.md"><b>Wiki</b></a>
</td>
<td align="center" width="25%" valign="top">
<h3>Look up</h3>
<p>The whole language<br/>on one page</p>
<a href="docs/language-reference.md"><b>Reference</b></a>
</td>
<td align="center" width="25%" valign="top">
<h3>Internals</h3>
<p>How the interpreter<br/>is built</p>
<a href="docs/architecture.md"><b>Architecture</b></a>
</td>
<td align="center" width="25%" valign="top">
<h3>Roadmap</h3>
<p>Status and what<br/>comes next</p>
<a href="docs/milestones.md"><b>Milestones</b></a>
</td>
</tr>
</table>

---

## Testing

```
cargo test
```

Unit tests sit next to each module; the integration tests in `tests/` run the
compiled binary against the example programs. The same checks run in CI, along
with `cargo fmt --check` and `cargo clippy -D warnings`.

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get
started, follow the [Code of Conduct](CODE_OF_CONDUCT.md), and check
[SUPPORT.md](SUPPORT.md) if you need help. The [changelog](CHANGELOG.md) records
what changed between versions.

---

## License

Released under the MIT License. See [LICENSE](LICENSE) for the full text, and
[TERMS.md](TERMS.md) for the project terms.

<div align="center">
<sub>Built in Rust with zero dependencies. Start writing MiruScriptX with the <a href="wiki/01-introduction.md">wiki</a>.</sub>
</div>
