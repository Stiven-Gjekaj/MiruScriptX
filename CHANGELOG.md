<div align="center">
  <a href="README.md"><img src="assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Changelog

All notable changes to MiruScriptX are recorded here. The format is based on
Keep a Changelog (https://keepachangelog.com), and the project aims to follow
semantic versioning.

## 0.3 (2026-07-23)

### Added

- Higher-order builtins `map`, `filter`, and `reduce`, backed by an
  interpreter-aware builtin kind so a builtin can call a user-defined function,
  a closure, or another builtin.
- `miru fmt`, a source formatter that reprints a program in one canonical style,
  preserving comments and single blank lines. It prints to standard output by
  default and rewrites the file in place with `-w` / `--write`.
- REPL history and line editing via rustyline, persisted to `~/.miru_history`
  across sessions, with arrow-key recall and Ctrl-C / Ctrl-D handling.
- A `transform.miru` example showing `map`, `filter`, and `reduce`.

### Changed

- MiruScriptX now has one external dependency (rustyline, for REPL history). The
  earlier zero-dependency claim is retired in favor of a dependency count in the
  README.

## 0.2 (2026-07-23)

### Added

- Maps and dictionaries: `{"key": value}` literals, reading and writing by key
  (a missing key reads as `nil`), with deterministic sorted-key ordering.
- Map builtins `keys`, `values`, and `has`; `len` now works on maps too.
- `break` and `continue` for `while` and `for` loops, rejected at parse time
  when used outside a loop.
- String builtins: `upper`, `lower`, `trim`, `replace`, `split`, `join`,
  `contains`, and `find`.
- Array builtins: `pop`, `index_of`, `slice`, `sort`, and `reverse`.
- Math and conversion builtins: `abs`, `min`, `max`, `floor`, `ceil`, `round`,
  `sqrt`, `pow`, `int`, and `float`.
- `input` for reading a line of input, backed by a testable input channel that
  mirrors the existing output trait.
- Error messages now carry a column and draw a caret under the offending token,
  for both syntax and runtime errors.
- Community and project documentation (contributing, code of conduct, security,
  terms, support, code owners, and issue and pull request templates), a restyled
  README with a project logo and badges, and branded headers across the docs.
- New `contacts.miru` and `greeter.miru` examples, and Maps and Errors lessons in
  the wiki.

## 0.1 (2026-07-22)

### Added

- Lexer, Pratt parser, and a tree-walking interpreter with zero dependencies.
- Integers, floats, booleans, strings, arrays, functions, closures, and nil.
- `let` bindings and reassignment; `if` / `else if` / `else`, `while`, and
  `for ... in`.
- Arithmetic with integer and float promotion, comparisons, and short-circuit
  logic.
- Array literals, indexing, and index assignment.
- Builtins `print`, `len`, `push`, `str`, `type`, and `range`.
- A command line runner (`miru run file.miru`) and an interactive REPL.
- Example programs, a test suite, a guided wiki, a single-page reference, and a
  CI workflow.
