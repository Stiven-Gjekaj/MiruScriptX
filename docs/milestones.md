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

Four milestones, all of them now behind. v0.7 finished the engine work v0.5
measured but could not reach. v0.8 and v0.9 added the two things that separate a
script runner from a language you could build something in: a way to split a
program across files, and a way to survive an error. v1.0 was the promise, the
packaging, and the specification rather than more language, and it is done.

The road ends here. A 1.1 would add builtins or syntax under the guarantee in
[stability.md](stability.md); it would not be another milestone on this list,
because the list was about reaching a language you could depend on and that has
happened.

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

Shipped. A program can be more than one file. `import "./prices.miru" as prices`
runs that file and binds everything it defines under `prices`, and the names in
each file are that file's own.

- **The lexer opened first.** A brace now restores newline significance inside a
  group, so a multi-line function handed straight to `map` parses. That was the
  defect logged in v0.7, worked around by binding the callback to a variable,
  and it had to go before a milestone that adds syntax could be trusted. The
  `Dot` token came with it, with the float boundary pinned: `2.5` is a number
  and `1.foo` is not one.

- **`GetField` exists beside `Index` for one reason**, and it is not speed. A
  module is an ordinary map, so `math["add"]` already worked. `math.nope` is an
  error naming the field, where `math["nope"]` is `nil`, because a missing key
  reading as `nil` is what maps have always meant and changing it would break
  every program that tests a lookup. The dot is for where a typo should stop the
  program; the brackets are for where a missing key is an answer.

  The risk flagged as the milestone's largest, a wildcard arm in the formatter's
  `precedence`, turned out to be inert: ten cases printed identically with the
  arm present and absent.

- **Namespacing `Globals` cost the VM nothing, and the diff proves it.** Each
  module gets its own name-to-slot map over one flat slot space, so `GetGlobal`
  still takes a slot number and indexes a vector. The commit has no diff in
  `src/vm.rs` at all; the benchmark agreed at -1.3% on a min of four, which is
  inside this host's noise either way.

- **`import` compiles to nothing.** There is no opcode and the compiler never
  touches the file system: imports resolve in a pre-pass before the importing
  file compiles, so by the time `math.add` runs, `math` is a global holding a
  map. The parser rejects an import outside the top level of a file, because it
  is the only stage that can tell. The loader caches by canonical path, so a
  diamond runs the shared file once and `./m.miru` and `./sub/../m.miru` are one
  file. A cycle reports its chain rather than recursing until the stack gives
  out, following `MAX_CALL_DEPTH`'s precedent of turning an unbounded failure
  into an ordinary error.

- **An error names the file it is in.** `MiruError` gained an optional file,
  which the loader sets on the way out of a module, innermost first, so an error
  three files deep names the one at fault rather than the one at the top of the
  chain. When a file is set nothing is underlined, because the source in hand
  belongs to a different file and a caret on the wrong line is worse than none.

- **The playground says what it cannot do.** Of the two honest options the
  roadmap offered, a bundled virtual file system or a plain statement, it took
  the statement: the page says it runs one file, and `import` there reports that
  the program was not loaded from a file. A browser has no file system, and
  pretending otherwise would have made the playground behave differently from
  the terminal it claims to match.

What v0.9 inherits: a module cannot keep anything to itself. Every top-level
name is exported, which is deliberately the permissive rule, because an `export`
keyword can narrow it later without invalidating a program written today and the
reverse would not be true. The per-element allocation v0.7 handed on is still
there as well; nothing in v0.8 went near `BuiltinFn`.

## v0.9: failure as a value

Shipped. `try` turns a failure into a value instead of ending the program, and a
program that reads input can now survive input it did not expect.

- **Errors became values, and the error path did not change.** All forty-odd
  raise sites still return `Err(MiruError)` through the same `?`. What changed is
  what happens on the way out: if a `try` is waiting, the VM rewinds to the mark
  it recorded, pushes the failure as a value, and re-enters the dispatch loop. No
  opcode gained a comparison, and a program with no `try` in it runs the bytecode
  it ran in v0.8.

- **A handler rewinds four things**, and forgetting one leaves a VM running on
  corrupt state rather than failing: frames, the value stack, pending
  higher-order builtins, and the upvalues still pointing at slots about to
  disappear. Upvalues close first, because closing one reads a slot the next step
  throws away.

- **Using a failure is fatal, and that is the design.** A failure may be
  assigned, asked its type, checked with `is_error`, and have its fields read.
  Anything else stops the program, naming the original failure at the line that
  misused it. The usual complaint about errors as values is that an unchecked one
  flows on as data and surfaces somewhere unrelated; here it cannot.

- **The compiler found none of the places that needed changing.** Adding
  `Value::Error` compiled clean on the first attempt, because sixty wildcard
  match arms absorbed it. Two refusals were silent rather than generic: `!`
  answers for every value through `is_truthy` and `==` for every pair through
  `Value::equals`, so a failure read as `false` and as unequal to everything,
  which is indistinguishable from an ordinary value that did not match. The same
  hole sat in `if` and `while`. A test per consumer path found them; the build
  never would have.

- **A failure remembers where it came from.** `MiruError` rides inside the value
  whole rather than as a copy of its parts, and the v0.6 trace is captured before
  anything unwinds, so `(try f()).trace` reads
  `["in f, called from line 2"]`. Knowing that something failed is much less
  useful than knowing where from.

- **The call depth limit is not catchable.** Runaway recursion is a bug rather
  than a condition to handle, and a `try` that swallowed it would hide the only
  thing worth knowing.

- **Field assignment landed here too**, closing the last entry in Known
  limitations. `m.a = 1` creates the field when it is absent, which is what
  `m["a"] = 1` has always done, and the opposite of what reading does.

- **What it cost the hot path is below the harness's resolution.** The one change
  that could have taxed every program is the conditional guard. Measured against
  v0.8.23 from a worktree, best of three a side: `fib` -0.1%, `loop_sum` +1.2%,
  against a within-side spread of 5.4% and 4.7%. Quoting the 1.2% would be
  reporting noise as a finding.

What v1.0 inherits: nothing in the language. Every feature on the road has
shipped, and Known limitations is empty for the first time. What is left is the
stability guarantee, prebuilt binaries, crates.io, the written specification, and
deciding what to call this thing now that "small" no longer describes it.

## v1.0: the promise, the package, and the specification

Shipped. The road is complete. There is no eleventh milestone, because
everything the road set out to build has been built.

- **A written specification.** `docs/specification.md`, in ASD-STE100
  Simplified Technical English, because a document whose job is to be readable
  one way only should not be written in a register that rewards flourish. Ten
  sections: the lexical structure, the grammar and precedence, the values, the
  semantics, errors, modules, all 37 builtins, the limits, and the CLI.

  Every claim was run through the binary rather than read off the source. That
  discipline paid twice. It caught that **arithmetic cannot produce `inf` or
  `nan` at all**, because division by zero is an error rather than an infinity.
  But `float("nan")`, `float("inf")`, and `pow` can produce them, which makes
  the comparison rules real rather than dead text. And it caught a bad *method*: the check for
  "`else` on the next line" failed in the REPL, which evaluates each line as
  soon as it is complete, while the claim itself was true. Verification can
  fail on a true statement.

  Every limit was reached by a program instead of read from a constant. That is
  why the table says 255 call arguments and not 256: the compiler accepts 255,
  and the error at 256 is what proves where the edge is.

- **A stability guarantee.** `docs/stability.md`. The line is **shape, not
  detail**. The shape of an error report is promised and the words in it are
  not, so the document says outright not to compare a message against a fixed
  string. The nesting limit of 256 and the `[...]` truncation mark are named as
  explicitly unstable, because both were chosen during this milestone and
  nothing measured either.

- **Four defects closed first**, because a 1.0 cannot promise stability over
  behaviour that is wrong. All four surfaced from writing the specification
  rather than from any test: `filter` read a caught error as true and silently
  kept every element; a value containing itself aborted the process on a Rust
  stack overflow; `for x in r` on a caught error reported the type rather than
  the failure; and a runtime error inside an imported module drew its caret on
  the *importing* file's source, marking innocent code.

- **One rule for names, replacing two.** `let print = 1` used to write over the
  builtin's slot, and builtin slots are shared by every module, so one file
  could break `print` inside a module it imported. A declaration now shadows,
  and an assignment never introduces a name. That is what assignment already
  did for every other undeclared name. This reverses a v0.8 decision on purpose:
  it contradicted what v0.8 was for.

- **Prebuilt binaries.** `.github/workflows/release.yml` builds five targets,
  each on a runner of its own architecture so nothing is cross-compiled, and
  attaches them to a **draft** release with one `SHA256SUMS`. A tag should build
  a release, not announce one.

  `scripts/install.sh` is POSIX sh and refuses rather than guesses: an unknown
  platform, a missing checksum file, or a checksum that does not match all stop
  it, because a wrong binary installed quietly is worse than no binary.

- **crates.io.** `cargo publish --dry-run` is clean at 59 files and 162 KiB
  compressed. The README image had to become an absolute URL, because crates.io
  does not resolve a relative image path and the logo would have been broken on
  the page most people see first.

- **The word "small" is retired**, in the three places it was a claim rather
  than ordinary English. 9,936 lines now, which makes the case by itself.

Two things did not go to plan, both recorded because the next person will hit
them:

- **The Node 24 bump is three of four.** `checkout`, `cache`, and `deploy-pages`
  declare `node24` in their new majors. `upload-pages-artifact` is a composite
  action in both v3 and v4 and calls `upload-artifact`, which is still `node20`.
  v4 is worth taking for pinning its inner action to a SHA, but it does not
  reach node24 and nothing here claims it does.

- **`workflow_dispatch` only works from the default branch.** Dispatching the
  release workflow while it lived on a feature branch returned a 404 that named
  nothing. The order that works is: merge to `main`, dispatch to prove all five
  platforms build and run without publishing, and only then push the tag.

## Known limitations

Defects found and reproduced but not yet scheduled into a milestone. Recorded
here rather than left in a conversation, so the next person to hit one finds it
already described instead of rediscovering it.

Nothing currently recorded.

(The multi-line callback defect logged in v0.7 was fixed in v0.8, which had the
lexer open anyway. Field assignment, logged in v0.8, was fixed in v0.9: `m.a = 1`
parses and assigns, and creates the field when it is absent, which is what
`m["a"] = 1` has always done. Reading a field that is not there stays an error,
because a misspelling on the way in is almost always a mistake and on the way
out almost never is.

v1.0 closed four more, none of which were ever logged here, because writing the
specification found all four before anybody could hit them: `filter` reading a
caught error as true, a self-referential value aborting the process, `for` over
a caught error reporting the type instead of the failure, and a module's runtime
error drawing its caret on the importing file. That is the argument for writing
a specification, stated as a fact rather than as a hope.)

## How versions are cut

Within a milestone, work lands as small, numbered commits (for example `v0.2.1`
through the final `v0.2.x`). The last commit of a milestone marks it complete.
