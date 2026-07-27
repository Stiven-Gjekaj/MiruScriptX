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

## The road to 1.0

Four milestones. v0.7 finishes the engine work v0.5 measured but could not
reach. v0.8 and v0.9 add the two things that separate a script runner from a
language you could build something in: a way to split a program across files,
and a way to survive a failure. v1.0 is the promise, the packaging, and the
specification.

Two features are deliberately **not** on this road. User-defined types stay out:
maps already serve as records, and Lua and early JavaScript both reached
maturity without them. Static typing stays out: it would be a different
language, not a later version of this one.

## v0.7: the builtin bridge, and errors that underline

Shipped. Errors underline the token they blame, there is one call depth limit
instead of two, and the builtin bridge is gone. The speed the milestone was
named for did not arrive, which is the interesting part.

- **Errors underline rather than point.** `undefined variable 'subtotal'` marks
  all eight characters instead of the first. It needed no new data: `render`
  re-lexes the source it is already handed and matches a token by line and
  column, using the spans v0.6 added for the playground's highlighting.

- **One call depth limit.** `MAX_HOST_CALL_DEPTH` is gone. A callback used to
  run on a nested bytecode loop, a real Rust call per level, so recursion
  through `map` died at 64 while direct recursion got 10,000. A higher-order
  builtin now suspends and lets the one dispatch loop make its calls, so a
  callback costs a heap frame like anything else. Five hundred levels deep
  through `map` is unremarkable; two hundred failed outright before.

- **The speed did not come, and the reason is worth keeping.** Best of four runs
  either side: `map` with a closure went 128.4 to 117.1 ns per element, and with
  a *builtin* callback it got worse, 84.6 to 107.4. The nested Rust call the
  whole plan aimed at removing was never the dominant cost. A state machine
  yields a step per element where the loop it replaced was a plain Rust `for`,
  and that costs more than the call did. The trampoline is kept for the limit
  fix rather than for speed, and `benches/vm.rs` carries the numbers.

- **Writing the builtins in MiruScriptX was rejected on evidence**, at 1.63x
  slower than the native `map` that already existed. The roadmap said the choice
  would be made with a benchmark in hand rather than on paper, and it was.

- **The benchmark harness turned out to be less trustworthy than documented.**
  Six runs of one unchanged binary spread thirty percent, each reporting a
  confidence interval under one percent wide. The four percent floor in
  `benches/vm.rs` was measured on the development machine and does not survive a
  shared host. One thirty percent result was believed, committed, and retracted
  an hour later. Two rules came out of it: run a comparison more than once, and
  quote a best of several.

What v0.8 inherits: the last per-element allocation in the engine. A builtin
callback still turns its arguments into a `Vec`, because that is what
`BuiltinFn` takes. Removing it means changing that signature across all
thirty-seven builtins, several of which move values out of the vector they are
handed.

## v0.8: modules

A program is one file today, and every name lives in one flat table alongside
about forty builtins. This is the largest milestone of the four.

- An `import` form that binds a module under a name, so a module's functions are
  reached through it rather than dumped into the importing scope. Explicit
  beats convenient here: a reader should be able to tell where a name came from
  without checking every import at the top of the file.

- **The architectural work is namespacing `Globals`.** It is one flat, shared
  table (`src/globals.rs`), which is exactly what lets a REPL session resolve a
  name defined by an earlier input. Modules need separate slot spaces without
  losing that, and the compiler resolves every global to a slot at compile time,
  so this reaches into the hottest naming path in the engine.

- Resolution relative to the importing file, with a cycle check that reports the
  chain rather than recursing until the stack gives out.

- The playground has no filesystem. Either imports resolve against a bundled
  virtual set of files, or the page states plainly that it runs single files.
  Decide honestly rather than letting the browser silently behave differently
  from the terminal.

- A prelude becomes possible. It is no longer where the higher-order builtins
  might end up: v0.7 measured that design at 1.63x slower than the native one
  and rejected it, so they stay in Rust.

## v0.9: failure as a value

Any runtime error kills the program today. Nothing defensive can be written: not
a bad index, not a failed conversion, not a missing key that matters.

- Errors become values rather than exceptions. No unwinding and no non-local
  jumps, which suits a VM whose control flow is entirely jumps, and which keeps
  the frame stack and the trace machinery from v0.6 intact. It is more typing at
  a call site than `try`/`catch`, and that is the trade: which operations can
  fail stays visible in the source.

- The decisions to make, each of which changes what the language feels like:

  - Which failures become values and which stay fatal. Not everything should be
    catchable; a call depth limit is a bug, not a condition to handle.
  - Whether an error is a new `Value` variant or a map with a known shape.
  - How a caller checks one without every call site growing three lines.
  - What happens to an unchecked error: silently ignored is the failure mode
    that makes this style hated, so it should be hard to drop one by accident.

- The trace captured in v0.6 should survive into a caught error. Knowing a
  failure happened is much less useful than knowing where it came from.

## v1.0: the promise, the package, and the specification

- **A stability guarantee**, which is what the version number actually means.
  Stable: the language's syntax, the behaviour of every builtin, the CLI's
  commands and exit codes, and the rendered shape of an error. Explicitly not
  stable: the bytecode format, opcode numbering, the Rust API, and anything
  `miru disasm` prints. Written down as its own document, so the line is
  findable rather than folded into a changelog.

- **Prebuilt binaries.** Cloning and building is the only way to get `miru`
  today. A tag-triggered workflow producing static binaries for Linux
  (x86_64 and aarch64, musl so they run anywhere), macOS (Intel and Apple
  Silicon), and Windows, attached to a GitHub release, plus a one-line install
  script.

- **crates.io.** `miruscriptx` is free; `miru` is taken by an unrelated crate at
  0.0.0. That costs nothing, since the package is already named `miruscriptx`
  and Cargo installs a binary named `miru` from it either way.

- **A written specification.** A grammar and the semantics behind it, separate
  from the wiki, which teaches rather than defines. The place to state what is
  currently only true because the implementation says so: evaluation order,
  numeric promotion and overflow, truthiness, scoping and capture, and every
  limit a program can hit.

- **Retire the word "small."** It was honest early and is still defensible at
  v0.6 (7,666 lines of Rust, roughly 1,500 of them comments and blanks), but
  modules and error handling will push it past the point where it describes
  anything useful. By 1.0 the interesting claim is not that the language is
  short; it is that it is *finished*, and written from scratch: a bytecode
  compiler, a virtual machine, closures, modules, a formatter, a disassembler,
  a stability guarantee, and a specification. Lead with that instead. The exact
  wording is a matter of voice rather than fact, so it is left open, but the
  direction is to claim completeness rather than size and to keep "written from
  scratch in Rust", which is the part that actually distinguishes it.

- **Move the workflows to Node 24.** GitHub is migrating the Actions runtime off
  Node 20 and warns on any action still declaring it. Nothing is broken today,
  and the deploy that raised the warning succeeded, so this is maintenance
  rather than a fix: bump `actions/checkout`, `actions/cache`,
  `actions/upload-pages-artifact`, and `actions/deploy-pages` to the majors that
  declare `node24`. Doing it here rather than sooner means the workflows are
  touched once, alongside the release workflow this milestone adds, instead of
  twice.

- **Say where the name comes from.** MiruScriptX is a revival of MiruScript, an
  earlier language by the same author, written in C. The X marks the successor
  rather than decorating it. A short note in the README, and a line in the specification's
  introduction, is enough. Worth saying because the lineage is the honest
  version of why this project exists, and because "the same idea, rebuilt in
  Rust with a bytecode VM instead of C" tells a reader more about the project
  than any feature list on the page.

## Known limitations

Defects found and reproduced but not yet scheduled into a milestone. Recorded
here rather than left in a conversation, so the next person to hit one finds it
already described instead of rediscovering it.

- **A field cannot be assigned through.** `m.a` reads, but `m.a = 1` does not
  parse. `m["a"] = 1` does the same job and works, and modules are read only,
  so v0.8 added field access without the assignment half rather than half
  building it. Doing it means a `SetField` opcode and deciding what assigning
  to a field that does not exist should do, which is a different question from
  reading one that does not: reading is an error because a typo should fail,
  where assigning would reasonably create it.

  (The multi-line callback defect logged in v0.7 was fixed in v0.8, which had
  the lexer open anyway.)

## How versions are cut

Within a milestone, work lands as small, numbered commits (for example `v0.2.1`
through the final `v0.2.x`). The last commit of a milestone marks it complete.
