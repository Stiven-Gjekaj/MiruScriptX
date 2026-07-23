<div align="center">
  <a href="../README.md"><img src="../assets/Miru.png" alt="MiruScriptX" height="44"></a>
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
- A command line runner (`miru run file.miru`) and an interactive REPL.
- Example programs, a unit and integration test suite, and this documentation.

## v0.2: maps, loop control, a bigger standard library, and better errors

- Maps / dictionaries: `{"key": value}` literals, reading and writing by key
  (a missing key reads as `nil`), with deterministic sorted-key ordering.
- Map builtins `keys`, `values`, and `has`, and `len` extended to maps.
- `break` and `continue` in `while` and `for` loops, checked at parse time so
  using them outside a loop is caught early.
- String builtins: `upper`, `lower`, `trim`, `replace`, `split`, `join`,
  `contains`, and `find`.
- Array builtins: `pop`, `index_of`, `slice`, `sort`, and `reverse`.
- Math and conversion builtins: `abs`, `min`, `max`, `floor`, `ceil`, `round`,
  `sqrt`, `pow`, `int`, and `float`.
- `input` for reading a line of input, through a testable input channel that
  mirrors the existing output trait.
- Error messages with a line, a column, and a caret under the offending token,
  for both syntax and runtime errors.

Documentation and project infrastructure landed alongside this milestone: a
guided wiki, a single-page reference, an architecture guide, community docs
(contributing, code of conduct, security, support, terms), issue and pull
request templates, and a branded README with a project logo.

## v0.3: tooling and richer functions

- REPL history and a source formatter (`miru fmt`).
- Higher-order builtins (`map`, `filter`, `reduce`), which require letting
  builtins call back into user-defined functions.

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
