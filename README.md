<div align="center">

<img src="assets/Miru.png" alt="MiruScriptX" width="300">

### A small, general-purpose scripting language, written in Rust

_Two engines: a tree-walking interpreter and a bytecode virtual machine_

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.94%2B-CE422B?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"/>
  <img src="https://img.shields.io/badge/dependencies-2_(66),_1_dev-007ec6?style=for-the-badge" alt="2 direct dependencies, 66 total crates, 1 of them a dev dependency"/>
  <img src="https://img.shields.io/badge/tests-196_passing-427819?style=for-the-badge" alt="196 tests passing"/>
</p>

<p align="center">
  <a href="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml"><img src="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <img src="https://img.shields.io/badge/version-0.4-blue?style=flat-square" alt="Version 0.4"/>
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
clean, modern syntax, written from scratch in Rust. Write functions, closures,
loops, arrays, and maps in familiar syntax, then run them from a file or an
interactive REPL. Programs use the `.miru` extension.

It ships two execution engines that run the same language: a tree-walking
interpreter, and a bytecode compiler with a stack virtual machine that is
several times faster. Every release checks them against each other, so choosing
one is purely a speed decision.

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
- Higher-order builtins: `map`, `filter`, and `reduce`
- File runner, a source formatter (`miru fmt`), and a REPL with history
- Errors with a line, a column, and a caret under the problem
- A bytecode compiler and stack VM, opt in with `run --vm`
- Minimal dependencies: rustyline at runtime, criterion for benchmarks
- Unit, integration, and cross-engine differential tests, plus CI

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
miru run examples/greet.miru
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

Reformat a program in the canonical style (add `-w` to rewrite it in place):

```
miru fmt examples/greet.miru
```

Run it on the older tree-walking interpreter (being retired) instead:

```
miru run --tree-walk examples/greet.miru
```

For a step-by-step guide, start the wiki at
[wiki/01-introduction.md](wiki/01-introduction.md).

---

## Examples

Runnable programs live in [examples/](examples):

| Program | Shows off |
| ------- | --------- |
| [greet.miru](examples/greet.miru) | Functions, arrays, and a loop |
| [fib.miru](examples/fib.miru) | Recursion |
| [fizzbuzz.miru](examples/fizzbuzz.miru) | Control flow and the modulo operator |
| [contacts.miru](examples/contacts.miru) | Maps, lookups, and iteration |
| [transform.miru](examples/transform.miru) | Higher-order functions: map, filter, reduce |

Run one with `miru run examples/contacts.miru`.

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

Source becomes tokens, tokens become an abstract syntax tree, and the tree is
then either evaluated directly or compiled to bytecode and run on a VM.

| Stage | Files | Lines | Responsibility |
| ----- | ----- | ----- | -------------- |
| **Lexer** | token.rs, lexer.rs | 753 | Source text to tokens, with line and column tracking |
| **Parser** | ast.rs, parser.rs | 1031 | Recursive descent plus a Pratt expression parser |
| **Interpreter** | value, environment, interpreter, builtins, ops | 2382 | Tree-walking evaluation, scopes, closures, builtins |
| **Bytecode engine** | chunk.rs, compiler.rs, vm.rs | 2238 | Compiles the AST to bytecode and runs it on a stack VM |
| **Formatter** | formatter.rs | 617 | Reprints a program in canonical form (`miru fmt`) |
| **CLI and REPL** | main.rs, repl.rs | 307 | File runner, formatter command, and interactive REPL |
| **Library** | lib.rs | 388 | Ties it together (`parse_program`, `run_source`, `run_source_vm`) |
| **Total** | **16 files** | **7716** | Written from scratch in Rust |

```
src/         the language (lexer, parser, evaluator, compiler, VM, CLI, REPL)
examples/    runnable .miru programs
wiki/        step-by-step learning lessons
docs/        language reference, architecture, and roadmap
tests/       end-to-end integration tests
benches/     criterion benchmarks comparing the two engines
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
compiled binary against the example programs. Differential tests run the same
programs on both engines and require identical values, errors, and output, so
the two can never quietly diverge. The same checks run in CI, along with
`cargo fmt --check` and `cargo clippy -D warnings`.

Benchmark the engines against each other with:

```
cargo bench
```

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
<sub>Built from scratch in Rust. Start writing MiruScriptX with the <a href="wiki/01-introduction.md">wiki</a>.</sub>
</div>
