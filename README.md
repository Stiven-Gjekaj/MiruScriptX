# MiruScriptX

[![CI](https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml/badge.svg)](https://github.com/stiven-gjekaj/miruscriptx/actions/workflows/ci.yml)

A small, general-purpose scripting language written in Rust, with zero external
dependencies.

MiruScriptX (files use the `.msx` extension) is dynamically typed with a clean,
modern syntax. It runs through a tree-walking interpreter: source is tokenized,
parsed into an abstract syntax tree, and then evaluated.

```
fn greet(name) {
  return "Hello, " + name + "!"
}

let names = ["Aiko", "Ken"]
for n in names {
  print(greet(n))
}
```

```
Hello, Aiko!
Hello, Ken!
```

## Features

- Integers, floats, booleans, strings, arrays, maps, and nil
- Arithmetic with integer and float promotion, comparisons, and short-circuit logic
- `let` bindings and reassignment
- `if` / `else if` / `else`, `while`, `for ... in`, and `break` / `continue`
- First-class functions and closures with `return`
- Array literals, indexing, and index assignment
- Maps with `{"key": value}` literals, reading, and writing by key
- Builtins: `print`, `len`, `push`, `str`, `type`, `range`, `keys`, `values`, `has`
- A file runner and an interactive REPL
- Friendly error messages with line numbers
- Zero dependencies: pure Rust standard library

## Quick start

Build the interpreter (you need a recent Rust toolchain):

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

## Learn the language

The `wiki/` folder is a guided, w3schools-style tour you can read in order,
starting at [Introduction](wiki/01-introduction.md). For a single searchable
page, see the [language reference](docs/language-reference.md).

## Documentation

- [wiki/](wiki): step-by-step lessons for learning the language
- [docs/language-reference.md](docs/language-reference.md): the whole language on one page
- [docs/architecture.md](docs/architecture.md): how the interpreter is built
- [docs/milestones.md](docs/milestones.md): status and roadmap

## Examples

Runnable programs live in [examples/](examples):

- `greet.msx`: functions, arrays, and a loop
- `fib.msx`: recursion
- `fizzbuzz.msx`: control flow

Run one with `miru run examples/fizzbuzz.msx`.

## Testing

```
cargo test
```

Unit tests cover the lexer, parser, interpreter, and builtins; the integration
tests in `tests/` run the compiled binary against the example programs.

## Project layout

```
src/         the interpreter (lexer, parser, evaluator, builtins, CLI, REPL)
examples/    runnable .msx programs
wiki/        learning stages
docs/        reference, architecture, and roadmap
tests/       end-to-end integration tests
scripts/     build_reference.sh regenerates the single-page reference
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get
started, and please follow the [Code of Conduct](CODE_OF_CONDUCT.md). For help
using the language, see [SUPPORT.md](SUPPORT.md); the
[changelog](CHANGELOG.md) records what changed between versions.

## License

MIT. See [LICENSE](LICENSE). Use of the project is also subject to the
[terms and conditions](TERMS.md).
