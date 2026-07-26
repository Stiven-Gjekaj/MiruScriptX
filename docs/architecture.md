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
| `src/globals.rs`     | The global table the compiler and VM share, addressed by slot |
| `src/compiler.rs`    | Compiles the AST into bytecode                             |
| `src/vm.rs`          | The stack-based virtual machine that runs bytecode         |
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

### A Pratt parser for expressions

Statements are parsed with plain recursive descent. Expressions use a small
Pratt (precedence-climbing) parser: `Parser::parse_binary` loops over infix
operators using a binding-power table (`infix_binding_power`), recursing with a
higher minimum power to get left associativity. Prefix operators, calls, and
indexing are handled by `unary` and `postfix`. This keeps operator precedence in
one readable place instead of a deep cascade of functions.

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

`benches/vm.rs` holds eight workloads, run end to end so the numbers reflect
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

### Input and output go through traits

Builtins that print do not write to stdout directly. Instead they receive a
`&mut dyn Output` (defined in `src/value.rs`), which the VM implements. The
binary points that at stdout, while `run_capture` in `src/lib.rs` points it at
an in-memory buffer. That is why the test suite can assert on program output
without spawning a process. A parallel `Input` trait feeds `input()` the same
way: real standard input in the binary, a scripted buffer in
`run_capture_with_input`.

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
three builtins' semantics or changing `BuiltinFn` across all thirty-seven.

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

That is the only cap. Until v0.7 there was a second one, `MAX_HOST_CALL_DEPTH`,
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

## How to extend it

### Add a builtin

Write a function in `src/builtins.rs` with the signature
`fn(&mut dyn Output, &mut dyn Input, Vec<Value>) -> Result<Value, String>`, then
register it in `register`. Return an `Err(String)` for bad arguments; the VM
attaches the current line and column automatically.

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
