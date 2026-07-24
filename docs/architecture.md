<div align="center">
  <a href="../README.md"><img src="../assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Architecture

MiruScriptX is a scripting language written from scratch in Rust, with a single
runtime dependency (rustyline, used only for REPL line editing and history).
Programs are compiled to bytecode and run on a stack virtual machine. This
document explains how the pieces fit together, so you can find your way around
the code and extend it with confidence.

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

Every stage reports problems as a single `MiruError { line, column, message }`
(defined in `src/lib.rs`), so a syntax error and a runtime error are surfaced
the same way, both carrying the source line and column where they happened. The
error's `render` method draws the offending source line with a caret under that
column.

## Module map

| File                 | Responsibility                                             |
| -------------------- | ---------------------------------------------------------- |
| `src/token.rs`       | `Token` and `TokenKind`, plus a `describe` helper for errors |
| `src/lexer.rs`       | Turns source text into tokens; tracks lines and columns    |
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

### Recursion has two different limits

Runaway recursion cannot be allowed to take the process down, and there are two
distinct ways it could.

Call frames live on the heap, so deep recursion does not overflow the machine
stack the way a tree walker's would; left alone it grows until memory runs out.
`MAX_CALL_DEPTH` (10,000) turns that into an ordinary runtime error with a line,
a column, and a caret.

A user function called *by a builtin*, as `map` calls the function it is given,
runs on a nested bytecode loop, which is a real Rust call consuming real machine
stack that the frame count does not account for. That needs its own, much lower
cap: `MAX_HOST_CALL_DEPTH` is 64, chosen against the smallest stack this may run
on (a Rust test thread gets two megabytes, where nesting fails somewhere past
180) rather than the roomiest.

## How to extend it

### Add a builtin

Write a function in `src/builtins.rs` with the signature
`fn(&mut dyn Output, &mut dyn Input, Vec<Value>) -> Result<Value, String>`, then
register it in `register`. Return an `Err(String)` for bad arguments; the VM
attaches the current line and column automatically.

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
