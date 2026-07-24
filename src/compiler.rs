//! The bytecode compiler: it walks the AST and emits a [`Chunk`] for the VM.
//!
//! This is the counterpart to the tree walker's evaluation. Where the
//! interpreter executes each AST node as it visits it, the compiler records what
//! to do as bytecode once, so the VM can run it without re-walking the tree.
//!
//! Support is added feature by feature across the v0.4 milestone; anything not
//! yet handled compiles to a clear error, so the VM can run the growing subset
//! it understands while the tree walker keeps running everything.

use std::rc::Rc;

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::value::Value;
use crate::MiruError;

/// A local variable in scope during compilation, at a known stack slot (its
/// index in the list) and the block depth where it was declared.
struct Local {
    name: String,
    depth: usize,
}

/// Bookkeeping for the loop currently being compiled, so `break` and `continue`
/// know where to jump and how many locals to discard first.
struct LoopContext {
    /// Where `continue` jumps back to (the condition check).
    start: usize,
    /// The scope depth of the loop body; locals at or below it are popped when
    /// `break` or `continue` leaves the current iteration.
    body_depth: usize,
    /// Offsets of `break` jumps to patch to the loop's exit.
    breaks: Vec<usize>,
}

/// Compiles an AST into a [`Chunk`].
pub struct Compiler {
    chunk: Chunk,
    /// 0 at the top level (where variables are globals), higher inside blocks.
    scope_depth: usize,
    /// Locals currently in scope, in stack order.
    locals: Vec<Local>,
    /// The stack of loops enclosing the code being compiled.
    loops: Vec<LoopContext>,
}

impl Compiler {
    /// Compile a whole program into a chunk that ends in a `Return`. The value
    /// returned by the VM is that of the program's final expression, matching
    /// what the tree walker's `run_program` yields.
    pub fn compile(program: &[Stmt]) -> Result<Chunk, MiruError> {
        let mut compiler = Compiler {
            chunk: Chunk::new(),
            scope_depth: 0,
            locals: Vec::new(),
            loops: Vec::new(),
        };
        compiler.program(program)?;
        let (line, column) = program.last().map(|stmt| (stmt.line, 1)).unwrap_or((0, 0));
        compiler.chunk.write_op(OpCode::Return, line, column);
        Ok(compiler.chunk)
    }

    fn program(&mut self, program: &[Stmt]) -> Result<(), MiruError> {
        let Some((last, rest)) = program.split_last() else {
            self.chunk.write_op(OpCode::Nil, 0, 0);
            return Ok(());
        };
        for stmt in rest {
            self.statement(stmt)?;
        }
        // The program's value is that of a trailing expression, or nil otherwise,
        // matching the tree walker's run_program.
        if let StmtKind::Expr(expr) = &last.kind {
            self.expression(expr)?;
        } else {
            self.statement(last)?;
            self.chunk.write_op(OpCode::Nil, last.line, 1);
        }
        Ok(())
    }

    /// Compile a statement for its side effects, leaving the stack unchanged.
    fn statement(&mut self, stmt: &Stmt) -> Result<(), MiruError> {
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                self.expression(expr)?;
                self.chunk.write_op(OpCode::Pop, stmt.line, 1);
            }
            StmtKind::Let { name, value } => {
                // Compile the value first, so a right-hand reference to the same
                // name resolves to the outer binding, not the one being declared.
                self.expression(value)?;
                if self.scope_depth == 0 {
                    self.named_global(OpCode::DefineGlobal, name, stmt.line, 1)?;
                } else {
                    // A local's value stays on the stack as the slot's storage.
                    self.declare_local(name, stmt.line)?;
                }
            }
            StmtKind::Assign { target, value } => {
                self.expression(value)?;
                match &target.kind {
                    ExprKind::Identifier(name) => {
                        if let Some(slot) = self.resolve_local(name) {
                            self.chunk
                                .write_op(OpCode::SetLocal, target.line, target.column);
                            self.chunk.write(slot, target.line, target.column);
                        } else {
                            self.named_global(OpCode::SetGlobal, name, target.line, target.column)?;
                        }
                    }
                    _ => {
                        return Err(MiruError::with_column(
                            target.line,
                            target.column,
                            "the bytecode VM does not support this assignment yet",
                        ));
                    }
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.if_statement(condition, then_branch, else_branch.as_deref())?,
            StmtKind::While { condition, body } => self.while_statement(condition, body)?,
            StmtKind::Break => self.break_statement(stmt.line)?,
            StmtKind::Continue => self.continue_statement(stmt.line)?,
            _ => {
                return Err(MiruError::new(
                    stmt.line,
                    "the bytecode VM does not support this statement yet",
                ));
            }
        }
        Ok(())
    }

    /// Compile `while cond { .. }`. The condition is re-checked at the top of each
    /// iteration; `Loop` jumps back to it, and `break` jumps past the whole loop.
    fn while_statement(&mut self, condition: &Expr, body: &[Stmt]) -> Result<(), MiruError> {
        let line = condition.line;
        let column = condition.column;
        let loop_start = self.chunk.code.len();
        self.expression(condition)?;
        let exit_jump = self.emit_jump(OpCode::JumpIfFalse, line, column);
        self.chunk.write_op(OpCode::Pop, line, column);
        self.begin_scope();
        self.loops.push(LoopContext {
            start: loop_start,
            body_depth: self.scope_depth,
            breaks: Vec::new(),
        });
        for stmt in body {
            self.statement(stmt)?;
        }
        let context = self.loops.pop().expect("loop context");
        let (end_line, end_column) = body.last().map(|s| (s.line, 1)).unwrap_or((line, column));
        self.end_scope(end_line, end_column);
        self.emit_loop(loop_start, line, column)?;
        self.patch_jump(exit_jump)?;
        self.chunk.write_op(OpCode::Pop, line, column);
        for break_jump in context.breaks {
            self.patch_jump(break_jump)?;
        }
        Ok(())
    }

    fn break_statement(&mut self, line: usize) -> Result<(), MiruError> {
        let Some(body_depth) = self.loops.last().map(|context| context.body_depth) else {
            return Err(MiruError::new(line, "break outside of a loop"));
        };
        self.pop_locals_to_depth(body_depth, line, 1);
        let jump = self.emit_jump(OpCode::Jump, line, 1);
        self.loops
            .last_mut()
            .expect("loop context")
            .breaks
            .push(jump);
        Ok(())
    }

    fn continue_statement(&mut self, line: usize) -> Result<(), MiruError> {
        let Some((start, body_depth)) = self
            .loops
            .last()
            .map(|context| (context.start, context.body_depth))
        else {
            return Err(MiruError::new(line, "continue outside of a loop"));
        };
        self.pop_locals_to_depth(body_depth, line, 1);
        self.emit_loop(start, line, 1)
    }

    /// Emit a `Pop` for each local at or deeper than `min_depth` without removing
    /// them from the compiler's list, since later code in the same scope still
    /// sees them. Used by `break` and `continue`, which jump over the normal
    /// end-of-scope cleanup.
    fn pop_locals_to_depth(&mut self, min_depth: usize, line: usize, column: usize) {
        let count = self
            .locals
            .iter()
            .rev()
            .take_while(|local| local.depth >= min_depth)
            .count();
        for _ in 0..count {
            self.chunk.write_op(OpCode::Pop, line, column);
        }
    }

    /// Compile a block of statements in a nested scope, popping any locals it
    /// declares off the stack when the scope ends.
    fn block(&mut self, statements: &[Stmt]) -> Result<(), MiruError> {
        self.begin_scope();
        for stmt in statements {
            self.statement(stmt)?;
        }
        let (line, column) = statements
            .last()
            .map(|stmt| (stmt.line, 1))
            .unwrap_or((0, 0));
        self.end_scope(line, column);
        Ok(())
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    /// Leave the current scope, emitting a `Pop` for every local it declared.
    fn end_scope(&mut self, line: usize, column: usize) {
        self.scope_depth -= 1;
        while matches!(self.locals.last(), Some(local) if local.depth > self.scope_depth) {
            self.chunk.write_op(OpCode::Pop, line, column);
            self.locals.pop();
        }
    }

    /// Record a new local at the current scope depth. Its slot is its position in
    /// the list, which is where its value already sits on the stack.
    fn declare_local(&mut self, name: &str, line: usize) -> Result<(), MiruError> {
        if self.locals.len() > u8::MAX as usize {
            return Err(MiruError::new(line, "too many local variables in scope"));
        }
        self.locals.push(Local {
            name: name.to_string(),
            depth: self.scope_depth,
        });
        Ok(())
    }

    /// Find a local variable's stack slot by name, searching innermost first.
    fn resolve_local(&self, name: &str) -> Option<u8> {
        self.locals
            .iter()
            .rposition(|local| local.name == name)
            .map(|slot| slot as u8)
    }

    /// Compile `if cond { .. } else { .. }`. The condition is left on the stack
    /// for `JumpIfFalse` to test, then discarded on whichever branch runs.
    fn if_statement(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
    ) -> Result<(), MiruError> {
        let line = condition.line;
        let column = condition.column;
        self.expression(condition)?;
        let else_jump = self.emit_jump(OpCode::JumpIfFalse, line, column);
        self.chunk.write_op(OpCode::Pop, line, column);
        self.block(then_branch)?;
        let end_jump = self.emit_jump(OpCode::Jump, line, column);
        self.patch_jump(else_jump)?;
        self.chunk.write_op(OpCode::Pop, line, column);
        if let Some(else_branch) = else_branch {
            self.block(else_branch)?;
        }
        self.patch_jump(end_jump)?;
        Ok(())
    }

    fn expression(&mut self, expr: &Expr) -> Result<(), MiruError> {
        let line = expr.line;
        let column = expr.column;
        match &expr.kind {
            ExprKind::Int(n) => self.constant(Value::Int(*n), line, column)?,
            ExprKind::Float(f) => self.constant(Value::Float(*f), line, column)?,
            ExprKind::Str(s) => self.constant(Value::Str(Rc::new(s.clone())), line, column)?,
            ExprKind::Bool(true) => self.chunk.write_op(OpCode::True, line, column),
            ExprKind::Bool(false) => self.chunk.write_op(OpCode::False, line, column),
            ExprKind::Nil => self.chunk.write_op(OpCode::Nil, line, column),
            ExprKind::Identifier(name) => {
                if let Some(slot) = self.resolve_local(name) {
                    self.chunk.write_op(OpCode::GetLocal, line, column);
                    self.chunk.write(slot, line, column);
                } else {
                    self.named_global(OpCode::GetGlobal, name, line, column)?;
                }
            }
            ExprKind::Unary { op, operand } => {
                self.expression(operand)?;
                let opcode = match op {
                    UnaryOp::Negate => OpCode::Negate,
                    UnaryOp::Not => OpCode::Not,
                };
                self.chunk.write_op(opcode, line, column);
            }
            ExprKind::Binary { op, left, right } => {
                self.expression(left)?;
                self.expression(right)?;
                self.chunk.write_op(binary_opcode(*op), line, column);
            }
            ExprKind::Logical { op, left, right } => {
                // MiruScriptX's && and || yield a bool, and short-circuit: the
                // right side is skipped when the left already decides the result.
                self.expression(left)?;
                self.chunk.write_op(OpCode::Truthy, line, column);
                let short_circuit = match op {
                    LogicalOp::And => OpCode::JumpIfFalse,
                    LogicalOp::Or => OpCode::JumpIfTrue,
                };
                let jump = self.emit_jump(short_circuit, line, column);
                self.chunk.write_op(OpCode::Pop, line, column);
                self.expression(right)?;
                self.chunk.write_op(OpCode::Truthy, line, column);
                self.patch_jump(jump)?;
            }
            _ => {
                return Err(MiruError::with_column(
                    line,
                    column,
                    "the bytecode VM does not support this expression yet",
                ));
            }
        }
        Ok(())
    }

    /// Add a value to the constant pool, returning its one-byte index. A chunk
    /// holds at most 256 constants, since the index is a single byte.
    fn constant_index(
        &mut self,
        value: Value,
        line: usize,
        column: usize,
    ) -> Result<u8, MiruError> {
        let index = self.chunk.add_constant(value);
        u8::try_from(index)
            .map_err(|_| MiruError::with_column(line, column, "too many constants in one chunk"))
    }

    /// Emit a jump instruction with a placeholder operand, returning the offset
    /// of that operand so it can be patched once the target is known.
    fn emit_jump(&mut self, op: OpCode, line: usize, column: usize) -> usize {
        self.chunk.write_op(op, line, column);
        self.chunk.write(0xff, line, column);
        self.chunk.write(0xff, line, column);
        self.chunk.code.len() - 2
    }

    /// Fill in a jump emitted earlier so it lands at the current end of the code.
    fn patch_jump(&mut self, operand: usize) -> Result<(), MiruError> {
        let distance = self.chunk.code.len() - (operand + 2);
        let distance = u16::try_from(distance)
            .map_err(|_| MiruError::new(0, "the compiled body is too large to jump over"))?;
        self.chunk.code[operand] = (distance >> 8) as u8;
        self.chunk.code[operand + 1] = (distance & 0xff) as u8;
        Ok(())
    }

    /// Emit a backward `Loop` jump to `target` (an earlier code offset).
    fn emit_loop(&mut self, target: usize, line: usize, column: usize) -> Result<(), MiruError> {
        self.chunk.write_op(OpCode::Loop, line, column);
        let distance = self.chunk.code.len() + 2 - target;
        let distance = u16::try_from(distance)
            .map_err(|_| MiruError::new(0, "the loop body is too large to compile"))?;
        self.chunk.write((distance >> 8) as u8, line, column);
        self.chunk.write((distance & 0xff) as u8, line, column);
        Ok(())
    }

    /// Emit a `Constant` instruction that pushes `value`.
    fn constant(&mut self, value: Value, line: usize, column: usize) -> Result<(), MiruError> {
        let index = self.constant_index(value, line, column)?;
        self.chunk.write_op(OpCode::Constant, line, column);
        self.chunk.write(index, line, column);
        Ok(())
    }

    /// Emit a named-global instruction (`DefineGlobal`, `GetGlobal`, or
    /// `SetGlobal`), storing the variable name as a string constant.
    fn named_global(
        &mut self,
        op: OpCode,
        name: &str,
        line: usize,
        column: usize,
    ) -> Result<(), MiruError> {
        let index = self.constant_index(Value::Str(Rc::new(name.to_string())), line, column)?;
        self.chunk.write_op(op, line, column);
        self.chunk.write(index, line, column);
        Ok(())
    }
}

fn binary_opcode(op: BinaryOp) -> OpCode {
    match op {
        BinaryOp::Add => OpCode::Add,
        BinaryOp::Subtract => OpCode::Subtract,
        BinaryOp::Multiply => OpCode::Multiply,
        BinaryOp::Divide => OpCode::Divide,
        BinaryOp::Modulo => OpCode::Modulo,
        BinaryOp::Equal => OpCode::Equal,
        BinaryOp::NotEqual => OpCode::NotEqual,
        BinaryOp::Less => OpCode::Less,
        BinaryOp::Greater => OpCode::Greater,
        BinaryOp::LessEqual => OpCode::LessEqual,
        BinaryOp::GreaterEqual => OpCode::GreaterEqual,
    }
}

#[cfg(test)]
mod tests {
    /// Describe an engine's outcome as a string, covering both the value (by its
    /// inspect form) and any error (message and position), so the two engines can
    /// be compared without `Value` needing `Debug` or `PartialEq`.
    fn describe(result: &Result<crate::value::Value, crate::MiruError>) -> String {
        match result {
            Ok(value) => format!("ok {}", value.repr()),
            Err(error) => format!("err {} @ {}:{}", error.message, error.line, error.column),
        }
    }

    /// Assert the tree walker and the VM agree on a source expression, in both
    /// the value produced and any error reported.
    fn agree(source: &str) {
        let tree = crate::eval_source(source);
        let vm = crate::eval_source_vm(source);
        assert_eq!(
            describe(&tree),
            describe(&vm),
            "engines disagreed on `{source}`"
        );
    }

    #[test]
    fn vm_matches_the_tree_walker_on_expressions() {
        let corpus = [
            "1 + 2 * 3",
            "(1 + 2) * 3",
            "10 - 4 - 3",
            "2 * (3 + 4)",
            "7 / 2",
            "7 % 3",
            "7.0 / 2.0",
            "-5 + 3",
            "- -8",
            "!true",
            "!nil",
            "!!0",
            "1 < 2",
            "2 <= 2",
            "3 > 4",
            "5 >= 5",
            "1 == 1.0",
            "2 != 3",
            "\"a\" + \"b\"",
            "\"x\" < \"y\"",
            "nil",
            "3.5",
            "1\n2\n3",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_runtime_errors() {
        let corpus = [
            "1 / 0",
            "1 % 0",
            "9223372036854775807 + 1",
            "1 + true",
            "-nil",
            "1 < \"a\"",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_globals() {
        let corpus = [
            "let x = 5\nx + 1",
            "let x = 5\nlet y = 10\nx * y",
            "let x = 1\nx = x + 1\nx",
            "let name = \"Aiko\"\n\"Hi \" + name",
            "let x = 5",
            "let a = 2\nlet a = 3\na",
            // Errors: reading and assigning an undefined variable.
            "missing",
            "missing = 5",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_control_flow() {
        let corpus = [
            "let x = 0\nif true { x = 1 }\nx",
            "let x = 0\nif false { x = 1 }\nx",
            "let x = 0\nif false { x = 1 } else { x = 2 }\nx",
            "let x = 5\nlet r = 0\nif x > 10 { r = 1 } else if x > 3 { r = 2 } else { r = 3 }\nr",
            "let n = 7\nlet label = \"\"\nif n % 2 == 0 { label = \"even\" } else { label = \"odd\" }\nlabel",
            "let x = 3\nif x > 0 { if x > 2 { x = 100 } }\nx",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_logical_operators() {
        let corpus = [
            "true && true",
            "true && false",
            "false && true",
            "true || false",
            "false || false",
            "false || true",
            "1 && 2",
            "0 && 1",
            "nil || 5",
            "1 < 2 && 3 < 4",
            "!(true && false)",
            // Short-circuit: the right side would divide by zero if evaluated.
            "false && (1 / 0 == 0)",
            "true || (1 / 0 == 0)",
            // Not short-circuited: the error must surface.
            "true && (1 / 0 == 0)",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_local_variables() {
        let corpus = [
            // A block-local declaration does not leak out.
            "let x = 1\nif true { let x = 2 }\nx",
            // Locals compute a value assigned back to a global.
            "let result = 0\nif true {\n  let a = 10\n  let b = 20\n  result = a + b\n}\nresult",
            // A local shadows an outer name inside the block only.
            "let n = 5\nlet out = 0\nif n > 0 {\n  let n = 100\n  out = n\n}\nout",
            // A right-hand reference resolves to the outer binding.
            "let out = 0\nif true {\n  let a = 3\n  let a = a + 1\n  out = a\n}\nout",
            // Nested blocks with their own locals.
            "let total = 0\nif true {\n  let a = 1\n  if true {\n    let b = 2\n    total = a + b\n  }\n}\ntotal",
            // Assigning to a local inside the block.
            "let out = 0\nif true {\n  let c = 1\n  c = c + 5\n  out = c\n}\nout",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_while_loops() {
        let corpus = [
            "let i = 0\nlet sum = 0\nwhile i < 5 { sum = sum + i\ni = i + 1 }\nsum",
            "let i = 0\nwhile i < 3 { i = i + 1 }\ni",
            "let i = 0\nwhile false { i = 1 }\ni",
            // A local declared in the loop body each iteration.
            "let i = 0\nlet sum = 0\nwhile i < 4 {\n  let step = i * 2\n  sum = sum + step\n  i = i + 1\n}\nsum",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_break_and_continue() {
        let corpus = [
            // break stops the loop early.
            "let i = 0\nwhile true {\n  if i == 3 { break }\n  i = i + 1\n}\ni",
            // continue skips the rest of an iteration.
            "let i = 0\nlet sum = 0\nwhile i < 6 {\n  i = i + 1\n  if i % 2 == 0 { continue }\n  sum = sum + i\n}\nsum",
            // break out of a loop that declares a local.
            "let i = 0\nlet last = 0\nwhile i < 10 {\n  let doubled = i * 2\n  last = doubled\n  if i == 4 { break }\n  i = i + 1\n}\nlast",
        ];
        for source in corpus {
            agree(source);
        }
    }
}
