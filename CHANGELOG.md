<div align="center">
  <a href="README.md"><img src="assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Changelog

All notable changes to MiruScriptX are recorded here. The format is based on
Keep a Changelog (https://keepachangelog.com), and the project aims to follow
semantic versioning.

## 1.1.0 (2026-07-30)

### Added

- `miru -e PROGRAM`, and its long form `miru --eval PROGRAM`, run a program
  given on the command line. No file is necessary.

  ```
  $ miru -e 'print(6 * 7)'
  42
  ```

  An error reports the same way it does for a file, with the source line and the
  mark below it, but it names no file, because there is none. A program given
  this way cannot use `import`: there is no directory to resolve a module
  against, and the existing error says so.

  Thanks to @ahmadalguydi for the first contribution to this project.

- **Files and the command line.** Four builtins, which together are what turn a
  program into a script:

  ```
  let names = args()
  if file_exists(names[0]) {
    write_file("out.txt", upper(read_file(names[0])))
  }
  ```

  `read_file(path)` gives a file as a string. `write_file(path, text)` writes
  one, replacing what was there. `file_exists(path)` says whether one is there.
  `args()` gives the arguments the program was given, without its own path.

  **A path is resolved against the working directory**, not against the script.
  `import` does the opposite, and the difference is deliberate: a module is part
  of the program and travels with it, while a data file belongs to whoever runs
  the program. Section 8.2 of the specification states both rules together.

  Everything after the file path on the command line now belongs to the program,
  including anything shaped like an option, so `miru run tool.miru --verbose`
  hands `--verbose` to the program. Invocations that were errors before now
  work; none that worked has changed.

  **The capability is absent by default.** Reading and writing refuse unless the
  host supplies a file system, which only the `miru` program does. The browser
  playground refuses with a sentence rather than a platform error, and a Rust
  embedder gets nothing until it asks. `file_exists` and `args` answer `false`
  and `[]` rather than refusing, because those are the honest answers where
  there is no file system and no command line.

- **Reading a map no longer copies the key.** `m["k"]` allocated a `String` and
  dropped it a line later; `BTreeMap` keyed by `String` accepts a `&str`. This
  is recorded as a simplification and **not** as a speed-up, because on the
  machine it was measured on the benchmark harness cannot resolve a change this
  size. `docs/architecture.md` has the numbers.

- **`AGENTS.md`**, for contributors working with an AI agent. It records the
  traps this project has actually fallen into rather than general advice: the
  verification gate and why the WebAssembly half of it is not optional, which
  files are generated, which numbers are pinned by tests, what the stability
  guarantee forbids, and how to tell a real benchmark result from a machine
  having a bad minute. `CLAUDE.md` points at it rather than repeating it.

  It also asks that commits be authored by the contributor rather than the
  agent or its vendor. A commit is a claim that somebody read the change and
  will answer for it later, and an agent cannot make that claim.

- **The README's numbers are pinned by a test.** The line counts, the file
  count, the playground's size, and the test badge are all facts about the tree
  that were maintained by hand, and all of them drifted repeatedly. They are now
  computed and checked, so the README cannot quietly disagree with the
  repository it describes.

- **The size of a `Value` is pinned** by an assertion. The virtual machine's
  stack is a `Vec<Value>`, so that figure is the unit of most of the copying the
  engine does, and nothing was watching it.

- **A release takes its notes from this file.** The release workflow asked
  GitHub to generate them, which produces a list of merged pull requests. There
  were none when 1.0.1 shipped, so the body read "There were no pull requests
  associated with the commits included in this release" while the entry written
  for that version sat here unused, and it was pasted in by hand.

  The section for the tag being built is now the body of the draft. A version
  with no section here falls back to the generated notes rather than publishing
  an empty release.

### Fixed

- **Deeply nested source no longer aborts the process.** `[[[[ ... ]]]]`, a long
  `1 + 1 + 1 ...`, `a[0][0][0] ...`, nested `if` blocks, and every other way of
  nesting overflowed the Rust stack and killed the process outright:

  ```
  thread 'main' has overflowed its stack
  fatal runtime error: stack overflow, aborting
  ```

  No message, no line, no caret, and nothing `try` could catch. `miru fmt` and
  `miru disasm` did the same, so formatting a file was enough to trigger it.

  The parser now refuses past a limit and reports it the way any other syntax
  error is reported, with a line and a column. Both limits are in the
  specification, section 9.

  Two separate things had to be counted, and neither implies the other. Nesting
  such as `[[[ ... ]]]` makes the parser call itself, and overflows on the way
  down before a tree exists to measure. A chain such as `1 + 1 + 1 ...` is one
  frame and one loop however long it runs, and leaves behind a tree as tall as
  the chain is long, which then overflows the compiler, the formatter, or the
  code that releases it. Expressions carry their height for the second case,
  and the limit is applied as each level is added: a tree too tall to walk was
  also too tall to release, so rejecting it after building it aborted in the
  destructor instead.

  **The two get different numbers, because they cost different amounts of
  stack.** Nesting is limited to 1000 and one expression to 10000. A level of
  nesting spends a parser frame; a term in a chain spends none at parse time and
  shows up only in the later passes, whose frames are much smaller. Both figures
  were measured, from below against what 1.0 managed on the smallest stack it
  ever ran on, and from above against what the passes survive on the smallest
  stack that ships. A chain past its limit says `the expression is too long`
  rather than `the program is nested too deeply`, because nothing in
  `1 + 1 + 1 ...` is nested inside anything.

  This is the same class of defect 1.0 closed for values, where `push(a, a)`
  followed by `a == a` aborted. That fix guarded comparing and printing. The
  parser was never checked.

- **Releasing a long chain of values no longer aborts the process.** Nothing
  bounds how deeply a value nests, because a loop builds one a link at a time:

  ```
  let a = []
  let i = 0
  while i < 60000 { a = [a]  i = i + 1 }
  print("built it")     // printed
  a = 0                 // aborted here, in the destructor
  ```

  It happened at the assignment that dropped the last reference, or at the end
  of the program, where it also lost whatever was still buffered on standard
  output. Nothing could catch it, because by then the program had finished.

  Inspecting such a value was already guarded, so the language would let a
  program build a value it then refused to look at, and died on releasing it.

  Releasing is iterative now: the children go on a list rather than on the
  stack. An array chain of two million releases where thirty thousand used to
  abort.

  Three kinds of chain, not one. Arrays and maps were expected. **Closures** were
  not: a closure holds its captures, a capture can hold a closure, and rebinding
  one in a loop builds a chain the same way. That one was found by auditing for
  the class rather than by fixing the instance.

  A value that contains itself is unchanged. It is a cycle, so nothing is
  released, which is what 1.0 promised.

- **`tests/never_aborts.rs`** generates programs and requires that none of them
  kills the process. Every instance of this defect so far was found by a person
  reasoning about the code, and each time the reasoning stopped one instance
  short of the class. An error is a pass; only death by signal, a panic, or an
  exit code that is neither 0 nor 1 fails.

### Changed

- **The interpreter chooses its own stack.** `miru` runs its work on a thread of
  64 MiB, and the WebAssembly build links a shadow stack of 16 MiB.

  How deeply a program may nest depends on how much stack the interpreter has,
  and that was whatever the machine handed out: 8 MiB on a main thread, 2 MiB on
  a spawned one, and `ulimit -s` moving both. The first nesting limit was
  measured against the tightest of those and came out at 64, which refused
  programs 1.0 accepted and so broke section 2.1 of the stability guarantee.

  Measured against a stack the project controls, nesting is limited to 1000 and
  the length of one expression to 10000.

  Both clear what 1.0 did, which is the test that matters rather than whether
  they clear an abort. 1.0 had no limit and simply ran until the stack ran out,
  and on the 1 MiB shadow stack the playground had, the smallest any 1.0 build
  had, its release binary reached 917 levels of nesting and a sum of 4959 terms.

  One case is narrower than 1.0 and is worth stating rather than leaving to be
  found. A sum of more than about 10000 terms is now a syntax error. 1.0 reached
  40255 of them on an 8 MiB main thread and nothing near that in a browser, so
  such a program ran on one build and stopped the process on another.

  An explicit thread stack is mapped rather than grown from the process stack,
  so `ulimit -s` no longer reaches it: a value 100000 deep survives under
  `ulimit -s 512`, where it aborted before.

  **If you use `miruscriptx` as a Rust library**, `run_source` uses the stack of
  the thread that calls it, which does not support these limits by default.
  Section 3.3 of the stability guarantee is new and says what to do about it.

- Corrected the line counts and the test count in the README. Both had drifted
  when `-e` was added.

## 1.0.1 (2026-07-29)

### Fixed

- The release workflow builds the Intel macOS binary on an Apple Silicon runner.
  It was pinned to GitHub's `macos-13` image, which has been retired: the job sat
  queued with no runner assigned while every other platform finished in under two
  minutes. Intel Mac runners are being withdrawn generally, so this builds
  x86-64 from Apple Silicon, which Apple's toolchain supports directly.

**No change to the language or the package.** This release touches only
`.github/workflows/release.yml`, which the crate's `exclude` list keeps out of
the published package, so the contents of 1.0.1 and 1.0 are identical. 1.0.1 is
the first published version because the fix had to land before any release could
be built at all.

## 1.0 (2026-07-29)

The first stable release. From here, [docs/stability.md](docs/stability.md) says
what will not change.

### Added

- **[A specification](docs/specification.md)**, in ASD-STE100 Simplified
  Technical English. It defines the lexical structure, the grammar and
  precedence, the values, the semantics, errors, modules, all 37 builtins, the
  limits, and the CLI. It states what was previously true only because the
  implementation said so: evaluation order, numeric promotion, overflow,
  truthiness, scoping, and capture.

- **[A stability guarantee](docs/stability.md)**. A program that is correct with
  1.0 stays correct with every later 1.x. The document is explicit about what is
  *not* covered: the bytecode, opcode numbering, the Rust API, `miru disasm`
  output, speed, the wording of any error message, and the nesting limit.

- **Prebuilt binaries** for Linux and macOS on x86-64 and arm64, and Windows on
  x86-64, attached to each release with a `SHA256SUMS` file.

- **An install script**:

  ```
  curl -fsSL https://raw.githubusercontent.com/stiven-gjekaj/miruscriptx/main/scripts/install.sh | sh
  ```

  It verifies the download against the published checksum and refuses to install
  if it does not match, or if the platform has no build.

- **The crate on crates.io**, installable with `cargo install miruscriptx`.

### Changed

- **A `let` at the top level now shadows a builtin instead of replacing it.**
  Previously `let print = 1` wrote over the builtin's slot, and builtin slots
  are shared by every module, so one file could break `print` inside a module it
  imported.

- **An assignment never introduces a name.** `print = 1` is now `cannot assign
  to undefined variable 'print'`, which is what assigning to any other
  undeclared name already did. Use `let` to introduce a name.

- Comparing two different values that each contain themselves raises `value is
  nested too deeply to compare` rather than answering. There is no answer.

- The runtime message `unhandled failure:` is now `unhandled error:`, and `a
  failure has no field` is now `an error has no field`. The language uses one
  word for one thing.

### Fixed

- **`filter` no longer reads a caught error as true.** It tested its callback's
  result with a plain truthiness check, so an error kept every element and said
  nothing.

- **A value that contains itself no longer stops the process.** `let a = [];
  push(a, a); a == a` aborted on a Rust stack overflow: no message, no caret,
  and `try` could not catch it. Comparing now answers `true` by identity;
  printing shows `[[...]]` at the point the value returns to itself.

- **`for x in r` over a caught error** now names the original error instead of
  reporting `cannot iterate over a error`.

- **A runtime error inside an imported module** now names the module. It
  previously reported a position belonging to the module against the *importing*
  file's source, drawing a caret on an unrelated line. A caught error's `file`
  field now answers correctly for a module, as 0.9 specified but could not do.

- The playground colours `import`, `as`, and `try` as keywords.

## 0.9 (2026-07-28)

### Added

- `try`, which turns a failure into a value instead of ending the program:

  ```
  let r = try 10 / 0
  if is_error(r) {
    print("could not:", r.message)
  }
  ```

  Without `try`, a failure is fatal exactly as before, so nothing already
  written behaves differently. `try` takes the whole expression after it, so
  `try a / b` covers the division; parentheses narrow it.

  A caught failure can be caught from any call depth. The frame stack, value
  stack, pending higher-order builtins, and open upvalues all rewind to where
  the `try` began.

- A failure is a value of a new type. `type(r)` answers `"error"`, and
  `is_error(r)` is the idiomatic check. It carries five fields, read with a dot:
  `message`, `line`, `column`, `file`, and `trace`. A name outside that set is
  an error rather than `nil`.

- The call trace survives into a caught failure, so `(try f()).trace` reads
  `["in f, called from line 2"]`. Knowing that something failed is much less
  useful than knowing where it came from.

- Assignment through a field: `m.a = 1`. A field that is not there is created,
  which is what `m["a"] = 1` has always done, and the opposite of what reading
  one does. This closes the only entry in Known limitations, which is now empty.

- `examples/recover.miru`, a program that survives a failure mid-loop, and a
  wiki lesson on handling failure.

### Changed

- Using a caught failure as an ordinary value stops the program. It may be
  assigned, asked its type, checked with `is_error`, and have its fields read;
  arithmetic, comparison, indexing, calling, passing it to any other builtin, or
  testing it in a condition all report `unhandled failure: <what went wrong>` at
  the line that misused it.

  A failure is deliberately not falsy. `if r { .. }` cannot mean "if it worked",
  because a successful `0`, `false`, `nil`, or `""` would be indistinguishable
  from a failure.

- Exceeding the call depth limit cannot be caught. Runaway recursion is a bug
  rather than a condition to recover from.

- `MiruError` gained a `fatal` flag, which marks the errors `try` may not catch.

### Fixed

- The playground colours `import`, `as`, and `try` as keywords. The first two
  have shown as ordinary text since 0.8: the list was maintained by hand, and
  nothing failed when it fell behind.

## 0.8 (2026-07-27)

### Added

- Modules. `import "./prices.miru" as prices` runs that file and binds
  everything it defines under `prices`:

  ```
  import "./prices.miru" as prices

  print(prices.with_tax(1300))   // 1404
  ```

  The path is relative to the file that names it, not to the working directory.
  Every name defined at the top level of a module is reachable through the
  alias; there is no `export` keyword, and so no way for a module to keep a
  helper to itself.

  A file runs the first time it is imported and not again. The cache is keyed by
  canonical path, so `./m.miru` and `./sub/../m.miru` are one file, and a
  diamond of imports runs the shared file once. An import cycle is reported as
  the chain of files that formed it rather than recursing until the stack gives
  out. An import is only valid at the top level of a file.

  `import` compiles to no bytecode. Imports are resolved before the file that
  names them compiles, so a module's exports are an ordinary map in an ordinary
  global by the time anything reads them.

- Field access with `.`, and a `GetField` opcode behind it. `m.a` reads a map's
  entry the way `m["a"]` does, and differs on a name that is not there: `m.nope`
  is an error, where `m["nope"]` is `nil`. Assignment through a field
  (`m.a = 1`) is not part of this release; `m["a"] = 1` does that job.

- An error raised inside an imported file names that file:

  ```
  error (./prices.miru, line 4, column 12): undefined variable 'rate'
  ```

  `MiruError` carries an optional file, set as the error leaves the module it
  came from, innermost first. Nothing is underlined in that case, because the
  source in hand belongs to a different file.

- `examples/shop.miru` and `examples/prices.miru`, a pair of files that work
  together, and a wiki lesson on modules.

### Changed

- Each file has its own names. `Globals` keeps a name-to-slot map per module
  over one flat slot space, so two files can both define `total` without
  colliding, while `GetGlobal` still takes a slot number and indexes a vector.
  The builtins are visible from every module.

- The playground page says that it runs one file. There is no file system in a
  browser, so `import` there reports that the program was not loaded from a
  file.

### Fixed

- A function body of more than one statement now parses inside `(` or `[`, so a
  multi-line callback can be written directly as an argument to `map` instead of
  being bound to a name first. A brace restores newline significance rather than
  merely not suppressing it, which is what the lexer was missing.

## 0.7 (2026-07-26)

### Added

- Errors underline the whole token they blame instead of pointing a caret at its
  first character, so a name is marked over its length:

  ```
  error (line 2, column 7): undefined variable 'subtotal'
      print(subtotal)
            ^^^^^^^^
  ```

  It needed no new data. `render` re-lexes the source it is already handed and
  matches a token by line and column, using the spans added in 0.6 for the
  playground's syntax highlighting, so nothing about a token's extent is carried
  through the syntax tree, the compiler, or the bytecode's position table. A
  source that does not lex, and an error at end of input, both fall back to a
  single caret.

### Changed

- Recursion has one limit instead of two. A function called by `map`, `filter`,
  or `reduce` used to run on a nested bytecode loop, a real Rust call per level,
  so recursion through a builtin failed at 64 levels while direct recursion had
  10,000. The higher-order builtins now suspend and let the single dispatch loop
  make their calls, so a callback costs an ordinary heap frame. Five hundred
  levels deep through `map` works; two hundred failed before.

  This is what the change bought. It did not make anything faster, which is what
  it was built for: `map` with a closure went from 128.4 to 117.1 nanoseconds
  per element, and with a builtin callback it got worse, 84.6 to 107.4. The
  nested call the work aimed at removing was never the dominant cost. Kept for
  the limit rather than the speed; `benches/vm.rs` has the numbers.

- `HostFn`, the signature of a higher-order builtin, no longer receives the
  virtual machine and returns a task rather than a value. `Vm::call_value` is
  gone. Both were public, and neither had a caller outside the crate.

### Fixed

- A callback that is an ordinary builtin, as in `map(xs, abs)`, no longer takes
  the general suspend-and-resume path, which had made it about 43% slower. It is
  driven straight through instead, leaving it about 27% slower than before this
  release rather than 43%.

### Known limitations

- A function body of more than one statement does not parse inside `(` or `[`,
  so a multi-line callback written directly as an argument to `map` is rejected.
  Binding it to a name first works. Recorded in `docs/milestones.md` with the
  mechanism and where the fix goes.

## 0.6 (2026-07-25)

### Added

- Call stack traces on runtime errors. An error inside a call reports the path
  of calls that reached it, innermost first, beneath the caret:

  ```
  error (line 2, column 12): cannot multiply a nil and a int
        return n * 2
                 ^
    in double, called from line 7
    in total, called from line 11
  ```

  A very deep trace is shortened in the middle when rendered, never when
  captured, so runaway recursion reports its error in fourteen lines rather than
  ten thousand and two.
- A [playground](https://stiven-gjekaj.github.io/MiruScriptX/) that runs the
  language in a browser, built to WebAssembly from the same lexer, compiler, and
  virtual machine as the `miru` command. It has an editor with syntax
  highlighting, the bundled example programs, a Format button, and a tab showing
  the bytecode a program compiles to. Published by its own workflow, separate
  from CI so a failed deploy is not reported as a broken language.
- `Lexer::tokenize_with_spans`, which records where every token and comment sits.
  A span cannot be recovered from a token afterwards, because a token's value
  does not determine its source text.
- `Globals::contains`, a membership test that does not create a slot.
- `ConstantLong`, `GetLocalLong`, and `SetLocalLong`.

### Changed

- Every one-byte operand limit is retired. A file may hold more than 256
  functions, a chunk more than 256 distinct constants, a function more than 256
  locals, and a literal more than 255 elements or entries. Cold instructions
  were widened outright; `Constant`, `GetLocal`, and `SetLocal` kept their short
  encoding and gained wide twins the compiler emits only when an index does not
  fit, so an ordinary program emits exactly the bytecode it did before.
- Every compiler error carries a position. Five did not, two of which reported
  no line either and rendered as a bare `error:` with nothing to point at.
- Finding an existing constant in the pool is a hash lookup rather than a linear
  scan. The scan had been bounded by the 256-constant cap; raising the cap took
  the bound with it and made compilation quadratic. 20,000 distinct constants
  went from 257 ms to 27 ms.
- `rustyline` is a non-wasm dependency. It is used only by the REPL, which
  belongs to the binary, so the library now compiles for
  `wasm32-unknown-unknown` unchanged.
- CI builds the WebAssembly target and lints and tests the whole workspace.

### Removed

- `Value::same_constant`, superseded by a `ConstantKey` hash in `src/chunk.rs`.
  Two definitions of when two constants are the same is how they come to
  disagree.

## 0.5 (2026-07-24)

### Added

- `miru disasm <file>` prints the bytecode a program compiles to, walking into
  nested functions, with each instruction's source line and the value behind
  each constant index.
- `tests/golden.rs`, a corpus pairing programs with the exact outcome each must
  produce, values and errors alike, down to the line and column a caret points
  at. Expectations are literals rather than regenerated, so a test cannot
  quietly absorb a change in behavior.
- A limit on call depth, as two separate caps because deep recursion can exhaust
  two different resources: 10,000 heap call frames, and 64 levels of calls made
  from inside a builtin, which run on nested bytecode loops that cost real
  machine stack.
- `BinaryConst`, an instruction carrying a constant right operand, and a
  `constants` benchmark workload for the folding pass that had no coverage.

### Changed

- The virtual machine is now the only engine. `src/interpreter.rs` and
  `src/environment.rs` are gone, and `Value` carries one function
  representation rather than two. `run --vm` is still accepted, so a command
  written against v0.4 keeps working, but it selects the only engine there is.
- Globals resolve to a slot at compile time in a table shared by the compiler
  and the VM, replacing a hash lookup by name on every access.
- Constant expressions are folded at compile time. A fold that fails is
  abandoned rather than reported, so a runtime error keeps its position.
- Performance, with every change measured against a baseline taken immediately
  before it. Relative to the start of v0.5: loop and global workloads about
  4.4x faster, strings 2.6x, arrays 2.4x, recursive `fib` 1.7x, maps 1.4x. The
  higher-order workload did not move, because its cost is in the builtin
  bridge, which none of this touched.
- The dependency badge reads `2 (57), 1 dev`, recounted from the resolved tree.
  It said 66 through v0.4. The direct dependencies are unchanged, rustyline at
  runtime and criterion for benchmarks; running a MiruScriptX program still
  pulls in only rustyline and its 15 crates.

### Fixed

- A program could fail to compile with "too many constants in one chunk" for
  wanting the *same* literal too often. The pool is capped at 256 by its
  one-byte operand, and each occurrence took a slot, so a three-hundred-line
  program that added 1 to a counter on each line failed at line 257. Entries are
  now reused, and the cap counts distinct values.
- `5[0]` put its caret under the index rather than under the unindexable target.
  Found by freezing behavior into golden tests before removing the engine that
  had it right.
- A runtime error inside a session left its call frame behind, and the next
  program pushed onto the abandoned stack and resumed into it. A failed program
  now returns with the value stack, frame stack, and open upvalues empty.

## 0.4 (2026-07-24)

### Added

- A bytecode compiler and a stack-based virtual machine, a second execution
  engine covering the whole language: globals and locals with block scoping, all
  control flow and loops, functions and calls, closures with upvalues, arrays,
  maps, indexing, and every builtin.
- `miru run --vm` selects the VM. The tree-walking interpreter remains the
  default while the VM is validated.
- Differential testing across the two engines: the same programs run on both and
  must agree on values, on error messages and their line and column, and on
  printed output, including every example program run through the binary.
- criterion benchmarks comparing the engines (`cargo bench`). The VM runs
  recursive `fib` about 3x faster, tight loops about 1.5x, and closure-heavy
  code about 1.8x.

### Changed

- Arithmetic, comparison, and indexing rules moved into a shared `ops` module,
  and the higher-order builtins now reach the running engine through a `Caller`
  trait, so both engines share one implementation of each.
- The dependency badge now reads `2 (66), 1 dev`: criterion is counted even
  though it is a dev-dependency. Running a MiruScriptX program still pulls in
  only rustyline and its 15 crates.

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
