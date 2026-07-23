<div align="center">
  <a href="../README.md"><img src="../assets/Miru.png" alt="MiruScriptX" height="44"></a>
</div>

# Architecture

MiruScriptX is a tree-walking interpreter written in Rust with zero external
dependencies. This document explains how the pieces fit together, so you can
find your way around the code and extend it with confidence.

## The pipeline

Source text flows through three stages:

```
source text
   |
   |  lexer   (src/lexer.rs, src/token.rs)
   v
 tokens
   |
   |  parser  (src/parser.rs, src/ast.rs)
   v
 AST (Vec<Stmt>)
   |
   |  interpreter  (src/interpreter.rs, src/value.rs,
   v                src/environment.rs, src/builtins.rs)
 values and printed output
```

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
| `src/value.rs`       | `Value`, `Function`, the `Output` trait, display and equality |
| `src/environment.rs` | `Scope` chain for lexical scoping and closures             |
| `src/interpreter.rs` | Walks the AST and evaluates it                             |
| `src/builtins.rs`    | The native builtins: printing, plus string, array, math, map, and input helpers |
| `src/lib.rs`         | Ties the modules together (`parse_program`, `run_source`, `run_capture`) |
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

Add a `StmtKind` in `src/ast.rs`, parse it in `Parser::statement`, and execute
it in `Interpreter::execute`.

## Testing

Each stage has unit tests next to it (`#[cfg(test)] mod tests`), covering the
lexer, parser, interpreter, and builtins. End-to-end tests in
`tests/integration.rs` run the compiled `miru` binary against the example
programs and check both output and exit codes. Run everything with
`cargo test`.
