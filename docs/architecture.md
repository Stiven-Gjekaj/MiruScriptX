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

### Environments are reference counted

A `Scope` (in `src/environment.rs`) maps names to values and points at an
optional parent. Scopes are shared through `Rc<RefCell<Scope>>`. When a function
value is created it captures a clone of the current scope handle in its
`closure` field, which is exactly what makes closures and recursion work: the
captured scope stays alive as long as the function does, and a recursive
function can find itself in the scope it was defined in.

### Return uses a control-flow signal

Executing a statement returns a `Flow` value: `Flow::Normal`,
`Flow::Return(value)`, or `Flow::Break` / `Flow::Continue` for loop control.
Blocks and loops propagate these upward; a function call catches `Flow::Return`
to produce its result, and a loop catches `Break` and `Continue`. This avoids
threading special cases through every statement.

### Numbers are integers or floats

`Value` has separate `Int(i64)` and `Float(f64)` variants. Arithmetic promotes
an integer to a float when the other operand is a float (`numeric_pair` in the
interpreter). Integer division and modulo truncate; division or modulo by zero
is a runtime error; integer operations use checked arithmetic and report
overflow rather than panicking.

### Two engines, one language

As of v0.3 the tree walker was the only way to run a program. v0.4 adds a second
engine: the compiler turns the AST into bytecode once, and the VM executes that
flat instruction stream. This avoids re-walking the tree and re-resolving names
on every evaluation, which is where a tree walker spends much of its time. In
benchmarks the VM runs recursive `fib` about three times faster.

The two run side by side on purpose. `miru run` uses the tree walker; `miru run
--vm` uses the VM. Keeping both lets every change be checked by *differential
testing*: the tests in `src/compiler.rs` run the same source on both engines and
assert they produce the same value, or the same error at the same line and
column, and the tests in `src/lib.rs` do the same for the printed output of every
example program. A language with two independent implementations that agree is a
strong signal that neither has drifted.

Three things make the agreement structural rather than accidental. Both engines
use the same `Value` type; both apply operators through `src/ops.rs`, so numeric
promotion, overflow checks, and index bounds are defined in exactly one place;
and both reach the builtins through the `Caller` trait, so `map`, `filter`, and
`reduce` are shared code rather than two implementations.

The VM's stack holds locals directly (a call is a frame push, not a heap-allocated
scope), and closures capture variables as *upvalues*: shared cells that start out
pointing at a live stack slot and are "closed" into owned values when that slot
goes away. That is what lets a closure outlive the function it came from while
still seeing writes made through the original variable.

### Input and output go through traits

Builtins that print do not write to stdout directly. Instead they receive a
`&mut dyn Output` (defined in `src/value.rs`), which the interpreter implements.
The binary points that at stdout, while `run_capture` in `src/lib.rs` points it
at an in-memory buffer. That is why the test suite can assert on program output
without spawning a process. A parallel `Input` trait feeds `input()` the same
way: real standard input in the binary, a scripted buffer in
`run_capture_with_input`.

## How to extend it

### Add a builtin

Write a function in `src/builtins.rs` with the signature
`fn(&mut dyn Output, &mut dyn Input, Vec<Value>) -> Result<Value, String>`, then
register it in `register`. Return an `Err(String)` for bad arguments; the
interpreter attaches the current line and column automatically.

### Add an operator

Add a `TokenKind` and lex it in `src/lexer.rs`, add a `BinaryOp` (or `UnaryOp`)
in `src/ast.rs`, give it a binding power in `infix_binding_power` and a mapping
in `make_infix` (both in `src/parser.rs`), then handle it in `eval_binary` in
`src/interpreter.rs`.

### Add a statement

Add a `StmtKind` in `src/ast.rs`, parse it in `Parser::statement`, execute it in
`Interpreter::execute`, and compile it in `Compiler::statement`. Add a
differential test so both engines are checked against each other.

### Add an opcode

Add a variant to `OpCode` in `src/chunk.rs` (with its `from_u8` and `name`
arms, and a disassembler case if it takes operands), emit it from
`src/compiler.rs`, and execute it in the `run_frames` loop in `src/vm.rs`.

## Testing

Each stage has unit tests next to it (`#[cfg(test)] mod tests`), covering the
lexer, parser, interpreter, formatter, compiler, and builtins. The differential
tests in `src/compiler.rs` run the same programs on both engines and compare
results and errors, and `src/lib.rs` compares their printed output across the
example programs. End-to-end tests in `tests/integration.rs` run the compiled
`miru` binary, on both engines, against the examples and check output and exit
codes. Run everything with `cargo test`, and the benchmarks with `cargo bench`.
