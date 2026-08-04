<div align="center">
  <a href="../README.md"><img src="../assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Architecture

MiruScriptX is a scripting language written from scratch in Rust, with a single
runtime dependency (rustyline, used only for REPL line editing and history).
Programs are compiled to bytecode and run on a stack virtual machine. This
document explains how the pieces fit together, so you can find your way around
the code and extend it with confidence.

The `playground/` crate builds the same language to WebAssembly so it runs in a
browser. It is separate so that its dependencies are not the language's: nothing
it pulls in is involved in running a `.miru` file.

## The pipeline

Source text goes through four stages, each handing its output to the next:

```
source text
   |
   |  lexer     (src/lexer.rs, src/token.rs)
   v
 tokens
   |
   |  parser    (src/parser.rs, src/ast.rs)
   v
 AST (Vec<Stmt>)
   |
   |  compiler  (src/compiler.rs)
   v
 bytecode (src/chunk.rs)
   |
   |  virtual machine  (src/vm.rs)
   v
 values and printed output
```

Compiling happens inside the VM rather than in the caller, because the two share
state that has to outlive a single program. A session compiles one input at a
time, and the second input has to resolve names the first defined.

`miru disasm <file>` prints the bytecode for a program, which is the quickest
way to see what any of this produces.

Every stage reports problems as a single `MiruError` (defined in `src/lib.rs`),
so a syntax error and a runtime error are surfaced the same way, both carrying
the source line and column where they happened. The error's `render` method
draws the offending source line with an underline beneath the token at that
column, and, for a runtime error raised inside a call, the path of calls it came
through.

The underline's width is recovered rather than stored. `render` re-lexes the
source it is handed and matches a token by line and column, using the spans the
lexer already records for the playground's highlighting, so nothing about a
token's extent has to be carried through the AST, the compiler, or the chunk's
position table. It costs one extra lex on a program that has already failed. A
source that does not lex, and an error at end of input, both fall back to a
single caret, the first because a lexer error points at a character rather than
a token and the second because `Eof` covers no text at all.

## Module map

| File                 | Responsibility                                             |
| -------------------- | ---------------------------------------------------------- |
| `src/token.rs`       | `Token` and `TokenKind`, plus a `describe` helper for errors |
| `src/lexer.rs`       | Turns source text into tokens; tracks lines, columns, and spans |
| `src/ast.rs`         | `Expr` and `Stmt` node definitions                         |
| `src/parser.rs`      | Builds the AST (recursive descent plus a Pratt expression parser) |
| `src/value.rs`       | `Value`, functions and closures, the `Output` and `Caller` traits |
| `src/ops.rs`         | Arithmetic, comparison, and indexing rules, in one place    |
| `src/chunk.rs`       | Bytecode chunks: opcodes, constants, positions, disassembler |
| `src/globals.rs`     | The global table the compiler and VM share, addressed by slot, with a name space per module |
| `src/compiler.rs`    | Compiles the AST into bytecode                             |
| `src/vm.rs`          | The stack-based virtual machine that runs bytecode, the module loader, and the `try` handler stack |
| `src/formatter.rs`   | Reprints a program in canonical form (`miru fmt`)          |
| `src/builtins.rs`    | The native builtins: printing, plus string, array, math, map, and input helpers |
| `src/lib.rs`         | Ties the modules together (`parse_program`, `run_source`, `disassemble_source`, `format_source`) |
| `src/main.rs`        | The `miru` command line interface                          |
| `src/repl.rs`        | The interactive REPL                                       |
| `playground/`        | A separate crate: WebAssembly bindings and the in-browser playground |

## Key design decisions

### Newlines are significant, with a twist

The language uses newlines (or `;`) to separate statements, which keeps the
syntax clean and semicolon-free. To let expressions span multiple lines, the
lexer suppresses newline tokens while it is inside parentheses or brackets
(tracked by `group_depth` in `src/lexer.rs`). Braces do not suppress newlines,
because a block's statements need those separators.

A brace does more than not suppress: it *restores* significance. `{` pushes the
current group depth and resets it to zero, `}` pops it back. Without that, a
multi-line function body written inside a call, which is what a callback handed
straight to `map` is, had its statement separators swallowed by the parenthesis
it sat inside, and the whole body parsed as one run-on statement. That was a
real defect from v0.6 to v0.7, worked around by binding the callback to a
variable first, and fixed in v0.8.

### A Pratt parser for expressions

Statements are parsed with plain recursive descent. Expressions use a small
Pratt (precedence-climbing) parser: `Parser::parse_binary` loops over infix
operators using a binding-power table (`infix_binding_power`), recursing with a
higher minimum power to get left associativity. Prefix operators, calls, and
indexing are handled by `unary` and `postfix`. This keeps operator precedence in
one readable place instead of a deep cascade of functions.

The loop is worth noticing for a second reason. Because same-precedence
operators are consumed by iterating rather than by recursing, a long chain costs
one stack frame while producing a tree as tall as the chain is long. That gap
between how deep the parser goes and how deep its output is turns out to matter;
"Parsing has a limit too" below is about closing it.

### Names are resolved at compile time, not at run time

Nothing looks a variable up by name while a program runs.

A local lives in a stack slot. The compiler tracks the locals in scope
(`Compiler::locals`) and turns each mention into the slot number it resolves to,
so `GetLocal` is an index into the stack rather than a search. A frame's slots
are a window into the shared value stack starting at `slot_base`, so entering a
function is a frame push rather than a heap-allocated scope.

A global lives in a slot too, in the table in `src/globals.rs`. The compiler
hands each name a slot the first time it sees it and emits that number. The
table is shared with the VM and outlives any one program, which is what lets a
REPL session define `x` in one input and read it in the next.

The table separates "a name has a slot" from "that slot holds a value": slots
are `Option<Value>`, and reading an empty one is the "undefined variable" error.

### A name belongs to its file

Through v0.7 a program was one file, and every name it defined shared one table
with the builtins. Two files could not be written without colliding, and there
was no way to say where a name came from.

`Globals` now holds a name-to-slot map per module over *one flat slot space*.
Module 0 is the root, the file you ran; each imported file gets the next id.
A look-up checks the builtins first, so `print` means the same thing in every
file, then the asking module's own map, and a name found in neither is given a
fresh slot recorded in that module's map alone.

The slot space stayed flat on purpose. `GetGlobal` takes a slot number and
indexes a `Vec<Option<Value>>`, and that is the instruction a loop full of
global reads executes. Making it a (module, name) pair would have charged every
program for the feature, including the ones that never import anything. The
commit that introduced namespacing has no diff in `src/vm.rs` at all, which is
the proof rather than the claim: the VM cannot have got slower, because it did
not change.

### The dot and the brackets differ on a name that is not there

A module's exports are an ordinary `Value::Map`, so `math["add"]` works and
`keys(math)` lists what a file defines. `GetField` exists beside `Index` for
what happens when the name is *missing*: `math.nope` is an error naming the
field, where `math["nope"]` is `nil`, because that is what indexing a map with
an absent key has always meant and changing it would break every program that
tests a lookup against `nil`.

So the two are not spellings of one thing. Reach for the dot where a typo should
stop the program, which is most of the time when you are reaching into a module,
and for the brackets where a missing key is an answer.

The field name is emitted as an ordinary constant rather than an opcode operand,
so a file with hundreds of distinct field names gets `ConstantLong` instead of a
cap at 256. `GetField`'s own operand byte holds no value at all, only the target
expression's position, the same trick `Index` uses: "no field 'nope'" underlines
the name, and "cannot read a field of a int" underlines the thing that has none.

`SetField`, added in v0.9, mirrors `SetIndex` down to that operand byte, and
differs from `GetField` on exactly the point above: assigning to a field that is
not there *creates* it, where reading one that is not there is an error. The
asymmetry is deliberate rather than an oversight. A name that is not there is
almost always a misspelling on the way out and almost never one on the way in,
and `m["a"] = 1` has always created the key, so the two spellings agree.

### An import resolves before the file that names it compiles

`import` compiles to nothing. There is no opcode for it, and the compiler never
touches the file system.

`Vm::run` already owned both compiling and running, so imports are resolved in a
pre-pass: every `import` in the parsed program is loaded, run to completion, and
bound to its alias, all before the importing file is compiled. By the time
`math.add` executes, `math` is an ordinary global holding an ordinary map.

That is what keeps each error in the layer that can actually tell:

- **The parser** rejects an `import` anywhere but the top level of a file. It is
  the only stage that knows whether it is inside a block.
- **The compiler** emits nothing and knows nothing about paths. The "not loaded
  from a file" error started there and moved to the loader, where the path is;
  the golden pinning it did not move, which is what proves it is the same error
  and not a new one.
- **The loader**, on `Vm`, has the path, the globals, and the ability to run a
  program, which is exactly what loading a module needs.

The loader keeps two things. A cache from *canonical* path to the exported map,
so a file reached twice runs once and `./m.miru` and `./sub/../m.miru` are
recognised as one file. And a stack of the canonical paths currently loading, so
a cycle is reported as the chain of files that formed it rather than recursing
until the process dies. `MAX_CALL_DEPTH` is the precedent: turn an unbounded
failure into an ordinary error with a position.

A missing file is reported *before* canonicalising, because `canonicalize` fails
on a path that does not exist and its error would be about the wrong thing.

Recursion here never re-enters the dispatch loop. A module's own imports resolve
before it compiles, and it runs to completion before control returns, so no two
`interpret` calls overlap and the assertion that frames and stack are empty
afterwards keeps holding.

Everything a module defines at its top level is exported, and there is no
`export` keyword. A module cannot keep a helper to itself, which is a real cost.
It is one an `export` keyword could pay later without invalidating anything
written against today's rule, which is why the rule is the permissive one.

### A failure becomes a value without the error path changing

`try expr` evaluates the expression and, if it fails at any depth, yields the
failure as a value instead of ending the program. The obvious way to build that
is a second exit route out of the dispatch loop. This has none.

Every one of the forty-odd raise sites still returns `Err(MiruError)` through the
same `?` it always did. What changed is what happens to that `Err` on the way
out: `run_frames` asks whether a handler is installed, and if one is, rewinds the
VM to the mark that handler recorded, pushes the failure as a value, and
*re-enters the loop*. No opcode gained a comparison, and a program with no `try`
in it executes the bytecode it executed before, which `miru disasm` will confirm.

Re-entry works because `run_frames_inner` already derives everything from `self`
at its `'frames` loop: the frame, its `ip`, its `slot_base`, and the chunk
pointer are all re-read there. One initialization had to move with it. The loop
used to start `resume_depth` at zero on the grounds that no task is ever pending
when it is entered, which is true exactly until the loop can be re-entered part
way through a program. It reads `self.resume_depth()` now, and that landed as its
own commit with an assertion proving the two were the same number beforehand.

**A handler records four things, and three of them are not obvious.** A failure
can be raised many frames below the `try` that catches it, so everything that
grew in between has to come back: the frame stack, the value stack, the pending
higher-order builtins, and the upvalues still pointing at stack slots about to
disappear. Upvalues close first, because closing one reads the slot it points at
and the next step throws those slots away. Missing any one of the four leaves a
VM that keeps running with corrupt state rather than failing, which is why the
`debug_assert!` in `interpret` that frames and stack are empty after a program is
worth more than it looks.

**Both ways out converge.** `BeginTry` records the mark and a landing; `EndTry`
discards it and leaves the expression's value. A failure lands at the same
instruction with the failure pushed instead. Since both leave exactly one value
where the expression's result belongs, the success path needs no jump over the
failure path.

`Value::Error` holds `Rc<MiruError>`: the caught value *is* the error, not a copy
of its parts. That is what lets a caught failure still answer `.trace` with the
path it came through, which is captured before anything is unwound.

### Using a failure is fatal, and that is the whole design

Errors as values fail in practice when an unchecked one flows onward as data and
surfaces somewhere unrelated. So a failure is not an ordinary value here. It may
be assigned, passed to `type` or `is_error`, and have its fields read. Every
other operation stops the program, naming the original failure at the position of
the misuse.

Two of those refusals were silent rather than merely missing. `!` answers for
every value through `is_truthy`, and `==` answers for every pair through
`Value::equals`, so without an explicit guard a failure came back as `false` and
as unequal to everything, which reads exactly like an ordinary value that did not
match. The same hole sat in `if` and `while`. `Value::condition` closes it with
one match rather than a truthiness test plus a check, so a conditional reads the
discriminant once as it always did.

**The Rust compiler did not find any of these.** Adding `Value::Error` compiled
clean on the first attempt, because sixty wildcard match arms across the files
that match on `Value` absorbed it. `ops.rs` already answered "cannot negate a
error" through one of them: true, generic, and about the type rather than the
failure being held. A missing arm here does not fail to compile, it behaves
almost right, which is worse. The audit was driven by a test per consumer path.

The one door left open is reading a field, because a program has to be able to
find out what it is holding, and a check that trips the guard it is checking for
is no check at all. It opens exactly as far as a closed set of five names, so a
misspelling fails the way `m.nope` does rather than reading `nil`.

**Not everything is catchable.** `MiruError` carries a `fatal` flag that the
handler search refuses, set only by the two call-depth-limit sites. Runaway
recursion is a bug rather than a condition to handle.

### An error carries the path it came through

A runtime error reports where the program broke and how it got there:

```
error (line 2, column 12): cannot add a nil and a int
      return a + 1
               ^
  in add, called from line 6
  in total, called from line 9
```

Two things make this less obvious than it looks.

**A frame's `ip` is where it resumes, not where the call was written.** The
`Call` arm sets it after reading the argument count, so it points one
instruction past the call. `CallFrame::call_site` steps back over the
instruction and checks the byte it lands on really is a `Call`, returning `None`
rather than a wrong line when it is not.

**The frames are gone by the time the error reaches the caller.**
`Vm::interpret` clears them before the error propagates. So the trace is
captured inside `run_frames`, which wraps the dispatch loop and attaches the
path on the way out, while the frames are still standing.

A callback passed to `map` needs no special handling here, which was not true
before v0.7. It used to run on its own nested `run_frames`, so the trace had to
be recorded by the innermost one and left alone by the outer ones. There is one
loop and one frame stack now, and a suspended builtin occupies no frame, so the
frame beneath a callback is still the one that executed the call to `map` and
the whole path is visible where the error is raised.

Each entry pairs a frame's name with its *caller's* call site, since a frame
knows where it calls onward, so the line a function was reached by lives one
frame out. Long traces are shortened when rendered, never when captured: ten
thousand frames of runaway recursion would otherwise bury the error in itself.

An error from an imported file names that file:

```
error (./prices.miru, line 4, column 12): undefined variable 'rate'
```

`MiruError` carries an optional `file`, which the loader sets as an error passes
out of a module. The innermost file wins, so an error three files deep names the
one it is actually in rather than the one at the top of the chain. When a file
is set, `render` prints no source line and draws no caret: the source it was
handed belongs to a different file, and underlining the wrong line is worse than
underlining nothing.

### Wide operands come in `Long` variants, not by widening everything

Most operands are one byte, which caps what a program may contain: how many
constants, locals, or functions it can have. Those caps used to be low enough to
refuse ordinary programs, and v0.6 raised them all to two bytes' worth.

How that was done depends on how hot the instruction is.

`Closure`, `Array`, and `Map` were widened outright. Each runs once, when a
closure is created or a literal is built, so a byte costs nothing measurable.

`Constant`, `GetLocal`, and `SetLocal` are the hottest instructions in the
language, and widening them would tax every program to buy headroom almost none
of them need. They kept their one-byte form and gained `ConstantLong`,
`GetLocalLong`, and `SetLocalLong`, which the compiler emits only when an index
does not fit. A program with fewer than 256 of a thing emits exactly the
bytecode it did before the variants existed, which `miru disasm` will confirm.

Two operands had to widen because of what they hold rather than how hot they
are. `Closure`'s per-upvalue operand is a flag and an index, and that index *is*
a local slot when the flag says local, so it could not stay narrow once local
slots were wide. The hidden sequence slot in `ForNext` is a local slot too.
Getting those two crossed would not fail to compile, it would capture the wrong
variable, which is why the test for them asserts a value rather than success.

The upvalue *count* keeps its one-byte cap. A function closing over 256 distinct
variables is not a program anyone writes.

### Closures capture upvalues

A closure that outlives the function it came from cannot keep pointing at that
function's stack slots. An *upvalue* is a shared cell that starts out `Open`,
naming a live slot, and is `Closed` into an owned value when the slot leaves the
stack. Closures over the same variable share one upvalue
(`Vm::capture_upvalue` looks for an existing one before making a new one), so a
write through either is seen by both.

### Control flow is jumps, and `return` is an opcode

There is no `Flow` enum threading break, continue, and return up through every
statement. `if` and `while` compile to conditional and unconditional jumps;
`break` and `continue` compile to jumps that the compiler patches once it knows
where the loop ends (`Compiler::loops` holds the pending ones); and `return`
compiles to `Return`, which pops the frame.

### Numbers are integers or floats

`Value` has separate `Int(i64)` and `Float(f64)` variants. Arithmetic promotes
an integer to a float when the other operand is a float (`numeric_pair` in
`src/ops.rs`). Integer division and modulo truncate; division or modulo by zero
is a runtime error; integer operations use checked arithmetic and report
overflow rather than panicking.

The VM has a fast path for two integers, but it *declines* rather than guesses:
overflow and zero divisors fall through to `src/ops.rs`, so there is exactly one
definition of what each operator means and which error it raises.

### How the tree walker was replaced

Through v0.3 a tree walker was the only engine. v0.4 built the compiler and VM
alongside it, and v0.5 deleted the tree walker. The interesting part is the
order, because swapping the engine underneath a language is exactly the change
that quietly alters behavior.

While both engines existed they were checked against each other by *differential
testing*: run the same source on both, and require the same value, or the same
error message at the same line and column. Two independent implementations that
agree are strong evidence neither has drifted.

That check is only as permanent as the second engine, so before deleting the
tree walker its behavior was frozen into `tests/golden.rs`: a corpus of programs
paired with the exact result each must produce, written as literals. A test that
regenerates its expected value cannot fail, and so cannot catch a regression.
Freezing first paid off immediately, by turning up a real bug: given `5[0]` the
VM put the caret under the index and the tree walker under the target, and the
tree walker was right.

Deleting the differential tests then had to be shown lossless rather than
assumed to be. Every source string in both suites was extracted and compared;
eighteen cases the golden corpus did not cover were added before anything was
removed.

The golden corpus is what made the v0.5 optimization work safe. Every change in
this section was made against a test suite that pins exact output and exact
error positions, including while the hot paths were being rewritten.

### Optimization is measured, not assumed

`benches/vm.rs` holds eleven workloads, run end to end so the numbers reflect
`miru run` rather than the dispatch loop in isolation. The rule is that a change
which does not move them is not an optimization and should be reverted rather
than kept for the complexity it adds.

The harness documents its own noise floor, which is worth reading before
trusting any number it prints. Adding a public function that no benchmark calls
measured as a 3.9% improvement, at p = 0.00, with tight confidence intervals: a
rebuild moves where the dispatch loop falls relative to cache lines, and a tight
interpreter loop is sensitive to it. Anything under about five percent from this
harness is unmeasured rather than small.

Over v0.5 the loop and global workloads came down by a factor of about 4.4, from
an integer fast path, an unchecked opcode decode, hoisting the chunk pointer out
of the dispatch loop, resolving globals to slots, and folding a constant operand
into the operator.

#### The five percent rule assumes a machine this one is not

Measured in July 2026 on a shared four-core cloud host: the **same binary**, no
code change of any kind, benchmarked twice back to back.

| Workload | Reported change, from nothing |
| -------- | ----------------------------- |
| `fib` | **+14.8%** |
| `globals` | **-14.8%** |
| `bridge_setup` | -9.2% |
| `bridge_builtin` | -6.2% |
| `constants` | +6.1% |
| `arrays` | -5.8% |
| `higher_order` | -3.5% |
| `maps` | -3.3% |

Every one of those is criterion reporting a difference between a binary and
itself. So on a host like this the resolution is about **fifteen percent**, and
the five percent rule above is not conservative enough. It was written on a
quieter machine and it is right for one; inheriting the number without
re-measuring the floor is the mistake.

The way to know is the experiment in that table, which costs two benchmark runs
and settles what any later comparison is worth. Run it first, on the machine in
front of you, before believing a result from this harness.

**How this misled once, in detail, because the shape recurs.** A change that
removed one allocation from the map *read* path measured `maps` at -6.6%, twice.
The same runs put `bridge_builtin` at -35.6% and -35.7%. That program never
reads a map, so the number could not be the change. Reproducing to a tenth of a
percent looked like proof it was real; it was two runs sharing one stale
baseline, taken before two heavy builds ran on the same host. Re-measured
properly, paired and back to back, the same change put `bridge_builtin` at
-13.3% and `globals` at -10.4%, both still impossible, and the control above
then explained all of it.

Three lessons, in the order they were learned:

1. **A reproducible number is not a true one.** Two runs against one bad
   baseline reproduce each other perfectly.
2. **Check the result against the mechanism.** A change to map reads cannot move
   a benchmark that never reads a map. That contradiction was visible in the
   first run and should have stopped the second.
3. **Measure the floor before the change.** Everything above would have been
   avoided by the two runs in that table.

#### What was left alone, and why

Two candidates were identified and **not attempted**: shrinking `Value` from 32
bytes to 16 by interning `Builtin`, and removing two of the three reference
count operations each call performs on its callee. Both are plausible and both
are real work.

Neither was started, because on a host with a fifteen percent floor there is no
way to tell a win from a regression, and a refactor kept on an unmeasurable
number is worse than no refactor. They are recorded here so the next person
starts from the candidates rather than the search, and they should be attempted
on a quiet machine where the control experiment above shows a floor worth
trusting.

`size_of::<Value>()` is pinned by an assertion in `src/value.rs`, so the figure
those candidates would move cannot drift unnoticed in the meantime.

### Input and output go through traits

Builtins that print do not write to stdout directly. Instead they receive a
`&mut dyn Output` (defined in `src/value.rs`), which the VM implements. The
binary points that at stdout, while `run_capture` in `src/lib.rs` points it at
an in-memory buffer. That is why the test suite can assert on program output
without spawning a process. A parallel `Input` trait feeds `input()` the same
way: real standard input in the binary, a scripted buffer in
`run_capture_with_input`.

### A capability the host may not have is a value, not a `cfg`

`Output` and `Input` are always there. `System` and `Clock` are not, and they
are the two traits an embedder decides about: `System` is the file system and
the command line, and `Clock` is the wall clock that `now()` reads. A VM that is
given neither refuses both, so nothing gains file access or a time by accident.

They are separate traits because a host can have one and not the other. The
browser playground is exactly that case: a page can answer `Date.now()` and
cannot open a file, so it supplies a `Clock` and no `System`, and `read_file`
still refuses there.

**Neither is conditional compilation, and this is the reason.** `std::fs` in a
WebAssembly build has no file system and no way to say so; `SystemTime::now`
there does not fail, it panics. Either would compile cleanly, pass every native
check, and go wrong only on the page. But the question a `cfg` answers is what
the target supports, and the question that matters is what the embedder permits,
which no target knows. So the capability is a value, `NoSystem` and `NoClock`
are what everything gets by default, and `src/main.rs` is the one file that
builds the real ones.

### A higher-order builtin suspends rather than calling back

`map`, `filter`, and `reduce` apply a function the program supplies. The obvious
way to write one is a Rust loop that calls the engine per element, and that is
how they worked through v0.6: `Vm::call_value` entered a *nested* copy of the
bytecode loop, one real Rust call per element.

They are state machines instead. A `HostTask` says what it wants applied by
returning `Step::Call(..)`, the one dispatch loop makes that call, and the
answer comes back through `resume`. `Vm::drive_tasks` is the whole trampoline.
Three details are load-bearing:

- **`Step` carries owned values and `resume` never sees the engine.** A task
  lives on the engine's task stack and the engine must touch its value stack to
  carry a step out, so a `Step` that borrowed from the task would make those two
  borrows overlap.
- **`Return` gained no comparison.** It already tested the frame depth against
  the point where the loop should stop; that number is now the depth a suspended
  task resumes at, and zero when none is pending. One compare, as before.
- **A task's result does not always belong on the stack.** `reduce` passes two
  arguments, which is exactly `map`'s arity, so `reduce([abs, abs], map, ..)`
  suspends one builtin inside another and the inner value belongs to the outer
  task. That is what `TaskSink` distinguishes.

**What it bought, and what it cost.** The reason to do it was speed, and that is
not what it delivered. Measured best-of-four either side, `map` with a closure
went from 128.4 to 117.1 ns per element, and `map` with a *builtin* callback got
worse, from 84.6 to 107.4. A state machine yields a step per element where the
loop it replaced was a plain Rust `for`, and that costs more than the nested
call it removed. Closing the remaining gap would mean either duplicating all
three builtins' semantics or changing `BuiltinFn` across the forty-one
builtins that use it. That is the number of `define` calls and not the number
of builtins, which is fifty; the other nine take a different signature
and would not be touched.

It is kept for the limit below, which is a correctness fix rather than a speed
one, and the numbers are recorded here rather than quietly omitted. The whole
measurement, including a thirty percent result that turned out to be noise and
had to be retracted, is in `benches/vm.rs`.

### Recursion has one limit, and used to have two

Runaway recursion cannot be allowed to take the process down, and there are two
distinct ways it could.

Call frames live on the heap, so deep recursion does not overflow the machine
stack the way a tree walker's would; left alone it grows until memory runs out.
`MAX_CALL_DEPTH` (10,000) turns that into an ordinary runtime error with a line,
a column, and a caret.

That is the only cap on *running* a program. Parsing one has its own, for a
different reason and on a different resource; the next section covers it.

Until v0.7 there was a second cap here, `MAX_HOST_CALL_DEPTH`,
set to 64: a user function called *by a builtin*, as `map` calls the function it
is given, used to run on a nested bytecode loop, which is a real Rust call
consuming real machine stack that a frame count does not account for. Recursion
that went back through a builtin every time would have exhausted that stack long
before ten thousand frames accumulated, so it needed its own, far lower limit.

A higher-order builtin now asks the one dispatch loop to make its calls instead
of making them itself, so a callback costs a heap frame like any other call and
the frame count accounts for all of it. Recursion through `map` reaches the same
limit direct recursion does. Two hundred levels deep failed outright before;
five hundred is unremarkable now.

### Parsing has a limit too, and it needs two counters

Call frames are on the heap, but the *parser* is ordinary Rust recursion on the
machine stack, and so is every later pass over the tree: the compiler, the
formatter, and the destructor that releases it. Until 1.1 nothing bounded any of
that, so deep enough source aborted the process with a Rust stack overflow. No
message, no caret, and nothing `try` could catch, because `try` is a runtime
construct and this happened before the program ran. `miru fmt` did it too, which
made merely formatting an untrusted file dangerous.

Two counters fix it, and the obvious single one is not enough. Two different
quantities have to stay bounded, and neither implies the other:

- **How far the parser calls itself.** `[[[ .. ]]]` descends one frame per
  bracket and overflows on the way *down*, before there is a tree to measure.
  `Parser::enter` counts this, and `unary` is where it is counted, because every
  expression passes through it on the way to `primary`.
- **How tall the tree gets.** `parse_binary` loops over infix operators rather
  than recursing for them, which is what makes precedence readable. The cost is
  that `1 + 1 + 1 ..` is one frame and one loop however long it runs: a counter
  watching recursion sees nothing, and the tree left behind is as tall as the
  chain is long. The same is true of `a[0][0][0]` and `a.b.c` in `postfix`.
  `Expr::height` counts this, computed once in `Expr::new` from the children's
  own heights, so keeping it costs one comparison per node.

Counting only the first lets a spine through. Counting only the second aborts
during the descent, before any height exists. Counting them separately but
independently is also wrong: a spine at every level of nesting grows the tree as
the *product* of the two, and 60 levels of 60 still aborted while both counters
read 60.

The height is tested as each level is added rather than on the finished
expression. That ordering is the point: a tree too tall to walk was also too
tall to release, so building it and then rejecting it moved the abort from the
compiler into the destructor and fixed nothing. Releasing became iterative later
in the same release, which removes that particular reason, but the ordering
stays: it costs one comparison and it keeps the compiler and the formatter safe
without either of them carrying its own guard.

### The two counters need two numbers

They shared one at first, and that was wrong in a way the first measurement hid.

`Parser::MAX_NESTING` is 1000 and `Parser::MAX_HEIGHT` is 10000, because the two
quantities cost different amounts of stack for the same figure. A level of
nesting spends a parser frame, so a thousand of them spend a thousand frames. A
term in a chain spends nothing at parse time and only shows up later, one frame
per level in whichever pass walks the tree, and those frames are much smaller
than the parser's.

Held to one number, the limit was set by the expensive quantity and applied to
the cheap one. The effect was quiet, because it looked like a limit rather than
a regression: `1 + 1 + ...` with two thousand terms parsed under 1.0 on every
build including the browser, and became a syntax error.

Both numbers are now bounded from two directions, measured rather than reasoned
about. From below by what 1.0 did on the smallest stack it ever ran on, which is
the 1 MiB shadow stack the playground had: 917 levels of nesting, and 4959 terms
in a chain. From above by what the current passes survive on the 16 MiB stack
the WebAssembly build links, which is the smallest of any build that ships:

| Pass          | Height reached |
| ------------- | -------------- |
| `miru run`    | 80000          |
| `miru disasm` | 80000          |
| `miru fmt`    | 61000          |

The formatter binds, which is worth knowing on its own: the pass most likely to
be run on a file somebody else wrote is the one with the least room. 10000 keeps
a margin of six under it and is twice what 1.0 reached on its smallest stack.

A chain past the limit reports `the expression is too long` rather than
`the program is nested too deeply`. They are separate faults and a reader given
the wrong one goes looking for brackets that are not in their program.

### Choosing the stack, rather than inheriting it

The first attempt at the limit measured the stack the program happened to have
and set the number from that. It came out at 64, and that was wrong in a way
worth recording, because everything about the measurement was correct.

A main thread usually gets 8 MiB, a spawned thread 2 MiB, `ulimit -s` moves
both, and none of it is promised. Measuring the tightest of them gives a limit
that is safe everywhere and useful nowhere: at 64, `1 + 1 + ...` with a hundred
terms was a syntax error. 1.0 had no limit and accepted it, so the fix for the
abort broke section 2.1 of the stability guarantee, which promises that every
program parsing under 1.0 parses under every later 1.x. A language whose grammar
depends on which thread it runs on does not have a grammar.

So the stack is chosen instead. `miru` does its work on a thread it starts with
64 MiB (`STACK_SIZE` in `src/main.rs`), and the WebAssembly build links a 16 MiB
shadow stack (`.cargo/config.toml`), because wasm has no threads and the linker
is the only lever there. An explicit thread stack is mapped rather than grown
from the process stack, so `ulimit -s` no longer reaches it either.

Measured against that, nested maps, the most expensive construct, survive to
3000 in a debug build and past 12000 in release. Every other construct reaches
further: a chain of operators, an index chain, and a field chain each cleared
12000 even in debug. 1000 leaves a margin of three on the tightest, and accepts
every depth a real program plausibly reaches. For scale, the deepest nesting in
this project's own examples is four.

The catch, and it is a real one: **an embedder calling the library gets its own
thread's stack.** `run_source` does not spawn, because it cannot on wasm, so a
caller on a default 2 MiB thread does not have room for a limit of 1000. Section
3.3 of the stability guarantee states this as a condition and shows the four
lines that satisfy it. The deep tests in `tests/golden.rs` do the same through
`with_interpreter_stack`, rather than assuming a stack libtest does not give
them.

### Releasing a value is iterative, because building one is unbounded

The limit above bounds how deeply a *program* nests. It does not bound how
deeply a *value* nests, and nothing does: a loop builds one a link at a time.

```
let a = []
while .. { a = [a] }
```

Rust's own destructor for that walks the chain by recursion, one frame per link,
so releasing it overflowed the stack and aborted. It happened at the assignment
that dropped the last reference, or at the end of the program, where it also
lost whatever was still buffered on standard output. Nothing could catch it,
because by then the program had finished.

Inspecting such a value was already guarded: `repr` truncates at 256 and
`equals` refuses past the same depth, both from v1.0. So the language would let
a program build a value it then declined to look at, and died on releasing it.
Guarding the two obvious walks and not the third is the shape this defect keeps
taking.

`release` in `src/value.rs` puts the children on a list instead of on the stack.
Each container taken off the list surrenders its own children to the list before
it is released, and the loop runs until the list is empty. Depth becomes length,
and length is heap.

Two details carry the correctness, and both are easy to get wrong in a way no
test of depth would catch:

- **Descend only into a body nobody else holds.** `Rc::try_unwrap` succeeds for
  exactly those. Descending into a shared one releases values another reference
  still points at, which is a use-after-free rather than a crash: nothing
  observable goes wrong until much later.
- **Empty a body before letting it fall out of scope.** Its own destructor runs
  at that moment, and finding nothing left is what stops it recursing back into
  the case being avoided.

`Drop` sits on `ArrayBody`, `MapBody`, and `Closure` rather than on `Value`,
because Rust forbids moving a field out of a type that implements `Drop` and
this codebase matches `Value` by value in a great many places. The bodies
dereference to the `RefCell` they wrap, so every `borrow` and `borrow_mut`
reaches through them untouched.

**Three chains, not one.** Arrays and maps were expected. Closures were not: a
closure holds its captures, a capture can hold a closure, and rebinding one in a
loop builds a chain exactly as an array does. It was found by auditing for the
class rather than by fixing the instance, which is the whole argument for doing
the audit first.

A value that contains itself is unaffected. It is a cycle, so no reference count
reaches zero and nothing is released, which is what happened before and what
section 2.6 of the stability guarantee describes.

## How to extend it

### Add a builtin

Write a function in `src/builtins.rs` with the signature
`fn(&mut dyn Output, &mut dyn Input, Vec<Value>) -> Result<Value, String>`, then
register it in `register`. Return an `Err(String)` for bad arguments; the VM
attaches the current line and column automatically.

It will never be handed a caught failure: `call_native` refuses one before
dispatch, which is what stops `print(r)` turning a failure nobody dealt with into
a line of output that looks deliberate. A builtin whose whole job is to inspect a
failure has to say so in `builtins::accepts_failure`, and there should be very
few of those.

That signature is `BuiltinFn`, and there are three others. `SystemFn` is handed
the file system, `AmbientFn` is handed what a program can ask for that its own
source does not determine, which is the clock, and `HostFn` is the higher-order
shape described below. The first
three are called identically and differ only in what they receive, so the choice
is only ever the question of what the builtin needs. Registration says which:
`define`, `define_system`, `define_ambient`, `define_host`.

### Add a higher-order builtin

One that applies a function the caller supplies, as `map` does, is written
differently, and the shape is not guessable from the one above. It cannot call
the function itself: it says what it wants applied and is resumed with the
answer.

Add a variant to `HostTask` holding whatever the walk needs, give `resume` an
arm for it returning `Step::Call(..)` while there is work and `Step::Done(..)`
at the end, write a constructor of type `HostFn` that checks arguments and
builds the task, and register it with `define_host`. The engine does the rest.

Three things are easy to get wrong:

- **`resume` is told the previous answer, not asked for the next call.** Its
  `last` argument is what the call it asked for last time returned, and `None`
  on the first step. Record that answer first, then decide what to ask for next.
- **Move elements out of the task rather than cloning them.** `take_next` swaps
  in `nil` and hands the element over. `reduce` shows why: its accumulator lives
  in the task, so the obvious spelling clones it on every single step, where the
  loop this replaced moved it into the argument list for free.
- **Test the task without a VM.** `resume` is a pure step function, so a
  sequencing fault can be caught directly rather than as a wrong program output
  three layers away. `builtins::task_tests` does this and nothing else.

### Add an operator

Add a `TokenKind` and lex it in `src/lexer.rs`, add a `BinaryOp` (or `UnaryOp`)
in `src/ast.rs`, give it a binding power in `infix_binding_power` and a mapping
in `make_infix` (both in `src/parser.rs`), then give it a rule in `src/ops.rs`
and an opcode (see below).

### Add a statement

Add a `StmtKind` in `src/ast.rs`, parse it in `Parser::statement`, and compile it
in `Compiler::statement`. Add golden cases in `tests/golden.rs` pinning what it
evaluates to and where its errors point.

### Add an opcode

Add a variant to `OpCode` in `src/chunk.rs`, append it to the `OPCODES` table in
the same order (a byte decodes by indexing that table, and `opcodes_match_their_byte`
checks the two agree), give it a `name` arm and a disassembler case if it takes
operands, emit it from `src/compiler.rs`, and execute it in the `run_frames` loop
in `src/vm.rs`.

Of those, only the disassembler case fails quietly. The exhaustive `match` in
`run_frames` refuses to compile without a VM arm, and `opcodes_match_their_byte`
catches a missing table entry, but a wrong or absent disassembler stride just
prints nonsense from that instruction onwards. Check it against a real program
rather than assuming.

Mind the position table while you are there. It holds one `(line, column)` entry
per *byte*, so an instruction's operand bytes can carry positions of their own.
`Index` uses this deliberately: it has an operand byte holding no value, purely
so a "cannot index" error can point at the target expression while an
out-of-range error points at the index.

## Testing

Each stage has unit tests next to it (`#[cfg(test)] mod tests`), covering the
lexer, parser, formatter, compiler, chunk, globals, VM, operators, and builtins.

Beyond those, the suites in `tests/` each do a different job:

| File                   | What it checks                                          |
| ---------------------- | ------------------------------------------------------- |
| `tests/golden.rs`      | A corpus of programs against literal expected outcomes, values and errors alike, with the exact line and column each error points at |
| `tests/language.rs`    | One behavior each, in prose, through the public API      |
| `tests/session.rs`     | That state carries across inputs, and that a failed input does not poison the next one |
| `tests/integration.rs` | The compiled `miru` binary end to end: `run`, `fmt`, `disasm`, exit codes |

Run everything with `cargo test`, and the benchmarks with `cargo bench`. Read
the module docs in `benches/vm.rs` before drawing conclusions from a benchmark
number.
