<div align="center">

<img src="assets/Miru.png" alt="MiruScriptX" width="300">

### A small, general-purpose scripting language, written in Rust

_Compiled to bytecode, run on a stack virtual machine_

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.94%2B-CE422B?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"/>
  <img src="https://img.shields.io/badge/dependencies-2_(57),_1_dev-007ec6?style=for-the-badge" alt="The language has 2 direct dependencies and 57 total crates, 1 of them a dev dependency"/>
  <img src="https://img.shields.io/badge/tests-267_passing-427819?style=for-the-badge" alt="267 tests passing"/>
</p>

<p align="center">
  <a href="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml"><img src="https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <img src="https://img.shields.io/badge/version-0.7-blue?style=flat-square" alt="Version 0.7"/>
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="MIT License"/>
</p>

<p align="center">
  <a href="https://stiven-gjekaj.github.io/MiruScriptX/"><b>Try it in your browser</b></a> |
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

Programs are compiled to bytecode and run on a stack virtual machine. A tree
walker came first and was replaced in v0.5, once a corpus of golden tests had
frozen its behavior exactly; `miru disasm` will show you what any program
compiles to.

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

### Engine and tooling

- Lexer, Pratt parser, bytecode compiler, stack virtual machine
- A standard library of string, array, math, map, and I/O builtins
- Higher-order builtins: `map`, `filter`, and `reduce`
- File runner, a source formatter (`miru fmt`), and a REPL with history
- A disassembler (`miru disasm`) that prints the bytecode for a program
- Errors with a line, a column, and an underline under the token at fault
- Minimal dependencies: rustyline at runtime, criterion for benchmarks
- A WebAssembly build and an in-browser playground, in a separate crate
- Unit, golden, session, and end-to-end tests, benchmarks, and CI

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

Source becomes tokens, tokens become an abstract syntax tree, the tree is
compiled to bytecode, and the bytecode runs on a stack virtual machine.

| Stage | Files | Lines | Responsibility |
| ----- | ----- | ----- | -------------- |
| **Lexer** | token.rs, lexer.rs | 880 | Source text to tokens, with line, column, and span tracking |
| **Parser** | ast.rs, parser.rs | 1031 | Recursive descent plus a Pratt expression parser |
| **Runtime model** | value.rs, ops.rs, builtins.rs | 1607 | Values, operator and indexing rules, the builtin library |
| **Bytecode engine** | chunk.rs, globals.rs, compiler.rs, vm.rs | 2710 | Compiles the AST to bytecode and runs it on a stack VM |
| **Formatter** | formatter.rs | 617 | Reprints a program in canonical form (`miru fmt`) |
| **CLI and REPL** | main.rs, repl.rs | 331 | File runner, `fmt` and `disasm` commands, and the REPL |
| **Library** | lib.rs | 490 | Ties it together (`parse_program`, `run_source`, `disassemble_source`) |
| **Total** | **15 files** | **7666** | Written from scratch in Rust |

The playground is a separate crate: 379 lines of Rust binding the language to
WebAssembly, and 551 of hand-written HTML, CSS, and JavaScript. It is counted
apart because it is not the language, and neither its code nor its dependencies
are involved in running a `.miru` file.

```
src/         the language (lexer, parser, compiler, VM, CLI, REPL)
playground/  WebAssembly bindings and the in-browser playground
examples/    runnable .miru programs
wiki/        step-by-step learning lessons
docs/        language reference, architecture, and roadmap
tests/       golden, language, session, and end-to-end tests
benches/     criterion benchmarks for the bytecode engine
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

Unit tests sit next to each module. Beyond those, `tests/golden.rs` pins a
corpus of programs to the exact result each must produce, values and errors
alike, down to the line and column a caret points at. Those expectations are
literals rather than regenerated, so a test cannot quietly absorb a change in
behavior, which is what made rewriting the engine's hot paths safe.
`tests/integration.rs` runs the compiled binary against the example programs.
The same checks run in CI, along with `cargo fmt --check` and
`cargo clippy -D warnings`.

Benchmark the engine with:

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

## Support

If you find MiruScriptX useful, you can support its development here.

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/U1G623RXOE)

---

## License

Released under the MIT License. See [LICENSE](LICENSE) for the full text, and
[TERMS.md](TERMS.md) for the project terms.

<div align="center">
<sub>Built from scratch in Rust. Start writing MiruScriptX with the <a href="wiki/01-introduction.md">wiki</a>.</sub>
</div>
