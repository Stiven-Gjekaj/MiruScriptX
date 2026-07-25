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

## v0.3: tooling, richer functions, and a first dependency

- REPL history and line editing, backed by rustyline and persisted to
  `~/.miru_history` between sessions, with arrow-key recall. This is the
  project's first external dependency, so the earlier zero-dependency claim is
  retired in favor of an honest dependency count.
- `miru fmt`, a source formatter that reprints a program in one canonical style
  (two-space indentation, minimal parentheses, inline literals), preserving
  comments and single blank lines. It prints to standard output by default and
  rewrites the file in place with `-w`.
- Higher-order builtins `map`, `filter`, and `reduce`, which required an
  interpreter-aware builtin kind so a builtin can call back into a user-defined
  function, a closure, or another builtin.

## v0.4: performance, with two engines side by side

- A bytecode compiler (`src/compiler.rs`) and a stack-based virtual machine
  (`src/vm.rs`) covering the whole language: globals and locals, all control
  flow and loops, functions, closures with upvalues, arrays, maps, indexing, and
  every builtin.
- Both engines run side by side. The tree walker stays the default; the VM is
  opt in with `miru run --vm`. Keeping both makes *differential testing*
  possible: the same program runs on each engine and must produce the same
  value, the same error at the same line and column, and the same output.
- Shared foundations so agreement is structural, not accidental: one value
  model, one `ops` module for arithmetic and indexing, and one builtin path
  through the `Caller` trait.
- criterion benchmarks comparing the engines. The VM runs recursive `fib` about
  3x faster, tight loops about 1.5x, and closure-heavy code about 1.8x.

## v0.5: one engine, and optimization

- The tree walker is retired. The VM is the only engine, `src/interpreter.rs`
  and `src/environment.rs` are gone, and `Value` has one function
  representation instead of two. `run --vm` is still accepted so a command
  written against v0.4 does not break, but it selects the only engine there is.
- Behavior was frozen into `tests/golden.rs` before any of that was removed: a
  corpus pairing each program with the exact outcome it must produce, written
  as literals so a test cannot absorb a change silently. Freezing first found a
  real bug, where `5[0]` put the caret under the index rather than the target.
  Deleting the differential tests was then shown lossless by diffing the source
  strings of both suites, which turned up eighteen uncovered cases.
- `miru disasm` prints the bytecode for a program, nested functions and all.
- Optimization, each change measured against a baseline taken immediately
  before it: globals resolved to slots at compile time, constant folding, an
  integer fast path for binary operators, an unchecked opcode decode, the
  running chunk derived once per frame instead of once per instruction, and a
  constant right operand folded into the operator. Loop and global workloads
  came down by a factor of about 4.4, strings 2.6, arrays 2.4, `fib` 1.7.
- Constant pool entries are now reused. The pool is capped at 256 by its
  one-byte operand, and every *occurrence* of a literal used to take a slot, so
  a three-hundred-line program that added 1 to a counter failed to compile.
- The benchmark harness documents its own noise floor. A public function that
  no benchmark calls measures as a 3.9% improvement at p = 0.00, because a
  rebuild moves where the dispatch loop lands; anything under five percent from
  this harness is unmeasured rather than small.
- Two recursion limits, since heap frames and nested host calls run out of
  different resources: 10,000 call frames, and 64 levels of calls made from
  inside a builtin.

## v0.6: better errors, and reach

- Call stack traces on runtime errors. An error raised three functions deep
  currently reports only the innermost position, so the caret shows where the
  program broke but not how it got there. The VM already keeps a frame stack
  with a position per frame, so the information exists; what is missing is
  carrying it out of the failure and rendering it, something like:

  ```
  error (line 2, column 12): cannot add a nil and a int
      return a + 1
             ^
    in add, called from line 6
    in total, called from line 9
  ```

  This wants a little care rather than a little code: the position saved in a
  frame is where that frame will resume, not where the call was written, and a
  deeply recursive failure needs the middle of the trace elided rather than ten
  thousand identical lines printed.

- Compile the interpreter to WebAssembly.
- Ship a live in-browser playground on GitHub Pages so anyone can try
  MiruScriptX without installing anything.
- The builtin bridge is the next thing worth optimizing, and v0.5 says so with
  a number rather than a guess. Every other benchmark workload came down by
  somewhere between 1.4x and 4.4x; `higher_order` did not move at all, because
  its cost is not in the dispatch loop. `map`, `filter`, and `reduce` allocate a
  result array and call back into a nested bytecode loop once per element, and
  nothing in v0.5 touched either.

- Widen the one-byte operands that cap what a program may contain. Four of them
  exist, and v0.5 fixed a fifth that was worse than any of these, so they are
  worth going through deliberately rather than one at a time as each is hit:

  | Operand | Cap | Reachable? |
  | ------- | --- | ---------- |
  | `Closure`'s function index | 256 functions per chunk | Yes. A 300-function file is an ordinary library, and it does not compile. |
  | A local's stack slot | 256 locals in scope | Unlikely by hand. |
  | `Array`'s element count | 255 elements in one literal | Unlikely; building with `push` in a loop has no such limit. |
  | `Map`'s entry count | 255 entries in one literal | Same. |

  Only the first is worth calling a defect. All four fail loudly, with a message
  and a position, rather than miscompiling, which is why none of them surfaced in
  testing. Widening them costs a byte per affected instruction to buy headroom
  most programs will never use, so it wants measuring like any other change.

  While in there, give "too many local variables in scope" the column and caret
  that every other error in the language carries. It reports a line only.

## How versions are cut

Within a milestone, work lands as small, numbered commits (for example `v0.2.1`
through the final `v0.2.x`). The last commit of a milestone marks it complete.
