<div align="center">
  <a href="../README.md"><picture>
    <source media="(prefers-color-scheme: dark)" srcset="../assets/miru-dark.svg">
    <img src="../assets/miru-light.svg" alt="MiruScriptX" height="44">
  </picture></a>
</div>

# Milestones and roadmap

This is where MiruScriptX is today and where it is headed. It is kept out of the
README on purpose, so the front page stays short.

## v0.1: a working core

The first milestone is a small but genuinely usable language, built end to end:

- Lexer with line tracking, comments, and semicolon-free statements.
- Recursive-descent parser with a Pratt expression parser.
- Tree-walking interpreter with:
  - integers, floats, booleans, strings, arrays, functions, and nil;
  - arithmetic with integer and float promotion, comparisons, and
    short-circuiting logical operators;
  - `let` bindings and reassignment;
  - `if` / `else if` / `else`, `while`, and `for ... in` control flow;
  - first-class functions and closures with `return`;
  - array literals, indexing, and index assignment.
- Builtins: `print`, `len`, `push`, `str`, `type`, `range`.
- A command line runner (`miru run file.msx`) and an interactive REPL.
- Example programs, a unit and integration test suite, and this documentation.

## v0.2 (current): maps and loop control

- Maps / dictionaries: `{"key": value}` literals, reading and writing by key
  (a missing key reads as `nil`), with deterministic sorted-key ordering.
- Map builtins `keys`, `values`, and `has`, and `len` extended to maps.
- `break` and `continue` in `while` and `for` loops, checked at parse time so
  using them outside a loop is caught early.

Deferred from this milestone, still planned:

- More general builtins (string helpers, array helpers, `input`, basic math).
- Error messages with a column and a caret under the offending token.

## v0.3: more builtins and better errors

- The general string, array, math, and `input` builtins deferred from v0.2.
- Column-and-caret error spans for both syntax and runtime errors.
- REPL history and a source formatter (`miru fmt`).

## v0.4: performance

- A bytecode compiler and a stack-based virtual machine, replacing the tree
  walker for a large speedup while keeping the same language.
- Benchmarks tracked over time.

## v0.5: reach

- Compile the interpreter to WebAssembly.
- Ship a live in-browser playground on GitHub Pages so anyone can try
  MiruScriptX without installing anything.

## How versions are cut

Within a milestone, work lands as small, numbered commits (for example `v0.2.1`
through the final `v0.2.x`). The last commit of a milestone marks it complete.
