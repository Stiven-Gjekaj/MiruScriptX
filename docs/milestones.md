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

## v0.6: better errors, honest limits, and a playground

- Call stack traces on runtime errors. An error inside a call now reports how it
  was reached, not just where it broke:

  ```
  error (line 2, column 12): cannot multiply a nil and a int
        return n * 2
                 ^
    in double, called from line 7
    in total, called from line 11
  ```

  Both hazards this was expected to have turned out to be real. A frame's saved
  instruction pointer is where it *resumes*, one instruction past the call, so
  the trace would have named the line after every call. And the frames are torn
  down before the error reaches whoever renders it, so the capture had to happen
  inside the dispatch loop while they were still standing, with a guard that
  keeps the innermost capture when a builtin such as `map` is in the path.
  Runaway recursion went from 10,002 lines of output to 14.

- Every one-byte operand limit retired. A file may hold more than 256 functions,
  a chunk more than 256 distinct constants, a function more than 256 locals, and
  a literal more than 255 elements or entries.

  Only the first was a real defect, but they were fixed together because they
  are one shape. Cold instructions were widened outright; the three hottest kept
  their short encoding and gained `ConstantLong`, `GetLocalLong`, and
  `SetLocalLong`, emitted only when an index does not fit, so an ordinary
  program emits exactly the bytecode it did before. Two operands widened for a
  different reason: `Closure`'s per-upvalue index and `ForNext`'s slot both hold
  a local slot, and crossing those widths would have captured the wrong variable
  rather than failing to compile.

  Raising the constant cap made compilation quadratic, because the pool's linear
  scan had been justified by that cap bounding it. A hash index took 20,000
  distinct constants from 257 ms back to 27 ms.

- Every compiler error carries a position. Checking the claim that one error was
  missing a column turned up five, two of which reported no line either.

- A WebAssembly build and a [playground](https://stiven-gjekaj.github.io/MiruScriptX/).
  The library needed one line of `Cargo.toml` to compile for
  `wasm32-unknown-unknown`, which is the evidence that this was packaging rather
  than porting: the compiler and VM are computation over a string, with no
  filesystem, process, time, or thread use.

  The page has an editor, the example programs, a Format button, and a tab
  showing the bytecode. Errors cross into the browser as rendered text, so the
  caret and the call trace are the same ones the terminal prints. Syntax
  highlighting runs the real lexer rather than a second grammar in JavaScript,
  which is what `Lexer::tokenize_with_spans` is for: a token's value does not
  determine its source text, so spans have to be recorded rather than
  reconstructed.

  It lives in its own crate. `wasm-bindgen` brings 12 crates, none of which are
  involved in running a `.miru` file.

## v0.7: the builtin bridge

- Optimize the path a higher-order builtin takes to call back into the language.
  v0.5 measured every workload and `higher_order` was the only one that did not
  move, because its cost is not in the dispatch loop that v0.5 rewrote. `map`
  allocates a `Vec` per element and goes through `Vm::call_value` into
  `call_from_host` into a nested `run_frames`, which is a real Rust call per
  element.

  That nesting is also why `MAX_HOST_CALL_DEPTH` has to be as low as 64 while
  ordinary calls get 10,000: each level costs machine stack rather than a heap
  frame. A trampoline keeping everything on one dispatch loop would fix both the
  speed and the asymmetry, and it is an engine redesign, which is why it has a
  milestone rather than a commit.

- Underline a whole token in an error rather than pointing a caret at its first
  character. The spans this needs already exist, added in v0.6 for syntax
  highlighting.

## How versions are cut

Within a milestone, work lands as small, numbered commits (for example `v0.2.1`
through the final `v0.2.x`). The last commit of a milestone marks it complete.
