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
use crate::value::{CompiledFunction, Value};
use crate::MiruError;

/// A local variable in scope during compilation, at a known stack slot (its
/// index in the list) and the block depth where it was declared. `captured`
/// records whether a nested function closes over it, so it is closed rather than
/// simply popped when its scope ends.
struct Local {
    name: String,
    depth: usize,
    captured: bool,
}

/// How a closure captures one upvalue: from a local of the immediately enclosing
/// function (`is_local`), or from that function's own upvalue. `index` is the
/// slot or upvalue index accordingly.
#[derive(Clone, Copy, PartialEq)]
struct UpvalueSpec {
    is_local: bool,
    index: u8,
}

/// A function whose compilation is paused while a nested function inside it is
/// compiled. The nested function may reach back into these to capture upvalues.
struct FunctionState {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: usize,
    loops: Vec<LoopContext>,
    upvalues: Vec<UpvalueSpec>,
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

/// Compiles an AST into a [`Chunk`]. The fields describe the function currently
/// being compiled; `enclosing` holds the functions paused around it.
pub struct Compiler {
    chunk: Chunk,
    /// 0 at the top level (where variables are globals), higher inside blocks.
    scope_depth: usize,
    /// Locals currently in scope, in stack order.
    locals: Vec<Local>,
    /// The stack of loops enclosing the code being compiled.
    loops: Vec<LoopContext>,
    /// The upvalues the current function captures.
    upvalues: Vec<UpvalueSpec>,
    /// Functions whose compilation is paused while this one is compiled.
    enclosing: Vec<FunctionState>,
}

impl Compiler {
    fn new() -> Compiler {
        Compiler {
            chunk: Chunk::new(),
            scope_depth: 0,
            locals: Vec::new(),
            loops: Vec::new(),
            upvalues: Vec::new(),
            enclosing: Vec::new(),
        }
    }

    /// Compile a whole program into a script function whose chunk ends in a
    /// `Return`. The value the VM returns is that of the program's final
    /// expression, matching what the tree walker's `run_program` yields.
    pub fn compile(program: &[Stmt]) -> Result<Rc<CompiledFunction>, MiruError> {
        let mut compiler = Compiler::new();
        compiler.program(program)?;
        let (line, column) = program.last().map(|stmt| (stmt.line, 1)).unwrap_or((0, 0));
        compiler.chunk.write_op(OpCode::Return, line, column);
        Ok(Rc::new(CompiledFunction {
            name: None,
            arity: 0,
            chunk: compiler.chunk,
        }))
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
                        } else if let Some(upvalue) = self.resolve_upvalue(name)? {
                            self.chunk
                                .write_op(OpCode::SetUpvalue, target.line, target.column);
                            self.chunk.write(upvalue, target.line, target.column);
                        } else {
                            self.named_global(OpCode::SetGlobal, name, target.line, target.column)?;
                        }
                    }
                    ExprKind::Index {
                        target: object,
                        index,
                    } => {
                        // The value is already on the stack; push the target and
                        // index above it, then store through them.
                        self.expression(object)?;
                        self.expression(index)?;
                        self.chunk
                            .write_op(OpCode::SetIndex, index.line, index.column);
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
            StmtKind::For {
                name,
                iterable,
                body,
            } => self.for_statement(name, iterable, body)?,
            StmtKind::Break => self.break_statement(stmt.line)?,
            StmtKind::Continue => self.continue_statement(stmt.line)?,
            StmtKind::Return(value) => {
                match value {
                    Some(expr) => self.expression(expr)?,
                    None => self.chunk.write_op(OpCode::Nil, stmt.line, 1),
                }
                // Return unwinds the whole call frame, so locals need no cleanup.
                self.chunk.write_op(OpCode::Return, stmt.line, 1);
            }
            StmtKind::Function { name, params, body } => {
                self.function(Some(name), params, body, stmt.line, 1)?;
                if self.scope_depth == 0 {
                    self.named_global(OpCode::DefineGlobal, name, stmt.line, 1)?;
                } else {
                    self.declare_local(name, stmt.line)?;
                }
            }
        }
        Ok(())
    }

    /// Compile a function's parameters and body into a nested [`CompiledFunction`]
    /// and emit a `Closure` in the current chunk that builds it at runtime, along
    /// with the specs for capturing its upvalues.
    fn function(
        &mut self,
        name: Option<&str>,
        params: &[String],
        body: &[Stmt],
        line: usize,
        column: usize,
    ) -> Result<(), MiruError> {
        self.begin_function();
        // Inside a function, declarations are local; parameters take the first
        // slots (0..arity), where the call places the arguments.
        self.scope_depth = 1;
        for param in params {
            self.declare_local(param, line)?;
        }
        for stmt in body {
            self.statement(stmt)?;
        }
        // A function that runs off the end returns nil.
        self.chunk.write_op(OpCode::Nil, line, column);
        self.chunk.write_op(OpCode::Return, line, column);
        let (chunk, upvalues) = self.end_function();

        let function = Rc::new(CompiledFunction {
            name: name.map(str::to_string),
            arity: params.len(),
            chunk,
        });
        let index = u8::try_from(self.chunk.add_function(function))
            .map_err(|_| MiruError::with_column(line, column, "too many functions in one chunk"))?;
        self.chunk.write_op(OpCode::Closure, line, column);
        self.chunk.write(index, line, column);
        self.chunk.write(upvalues.len() as u8, line, column);
        for upvalue in upvalues {
            self.chunk.write(u8::from(upvalue.is_local), line, column);
            self.chunk.write(upvalue.index, line, column);
        }
        Ok(())
    }

    /// Pause the current function and start a fresh one for a nested function.
    fn begin_function(&mut self) {
        let suspended = FunctionState {
            chunk: std::mem::take(&mut self.chunk),
            locals: std::mem::take(&mut self.locals),
            scope_depth: self.scope_depth,
            loops: std::mem::take(&mut self.loops),
            upvalues: std::mem::take(&mut self.upvalues),
        };
        self.enclosing.push(suspended);
        self.scope_depth = 0;
    }

    /// Finish the current function, returning its chunk and captured upvalues, and
    /// restore the enclosing function's state.
    fn end_function(&mut self) -> (Chunk, Vec<UpvalueSpec>) {
        let chunk = std::mem::take(&mut self.chunk);
        let upvalues = std::mem::take(&mut self.upvalues);
        let restored = self.enclosing.pop().expect("an enclosing function");
        self.chunk = restored.chunk;
        self.locals = restored.locals;
        self.scope_depth = restored.scope_depth;
        self.loops = restored.loops;
        self.upvalues = restored.upvalues;
        (chunk, upvalues)
    }

    /// Resolve `name` as an upvalue of the current function, capturing it through
    /// any functions in between. Returns `None` when `name` is not a local of any
    /// enclosing function, so the caller falls back to a global.
    fn resolve_upvalue(&mut self, name: &str) -> Result<Option<u8>, MiruError> {
        let mut found = None;
        for level in (0..self.enclosing.len()).rev() {
            if let Some(slot) = local_slot(&self.enclosing[level].locals, name) {
                found = Some((level, slot));
                break;
            }
        }
        let (level, slot) = match found {
            Some(pair) => pair,
            None => return Ok(None),
        };
        self.enclosing[level].locals[slot as usize].captured = true;
        // The function just inside the declaring one captures its local; each
        // function deeper in captures the previous one's upvalue.
        let mut index = self.add_upvalue(level + 1, true, slot)?;
        for deeper in (level + 2)..=self.enclosing.len() {
            index = self.add_upvalue(deeper, false, index)?;
        }
        Ok(Some(index))
    }

    /// Add (or reuse) an upvalue at function `level`, where `level` equal to the
    /// number of enclosing functions means the current one.
    fn add_upvalue(&mut self, level: usize, is_local: bool, index: u8) -> Result<u8, MiruError> {
        let spec = UpvalueSpec { is_local, index };
        let upvalues = if level == self.enclosing.len() {
            &mut self.upvalues
        } else {
            &mut self.enclosing[level].upvalues
        };
        if let Some(position) = upvalues.iter().position(|existing| *existing == spec) {
            return Ok(position as u8);
        }
        if upvalues.len() >= u8::MAX as usize {
            return Err(MiruError::new(
                0,
                "too many captured variables in one function",
            ));
        }
        upvalues.push(spec);
        Ok((upvalues.len() - 1) as u8)
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

    /// Compile `for name in iterable { .. }`. The iterable is snapshotted into a
    /// hidden local, a hidden index walks it, and each iteration binds `name` to
    /// the next element in a fresh per-iteration scope.
    fn for_statement(
        &mut self,
        name: &str,
        iterable: &Expr,
        body: &[Stmt],
    ) -> Result<(), MiruError> {
        let line = iterable.line;
        let column = iterable.column;
        self.begin_scope();
        // The snapshot and index are hidden locals whose names cannot be written
        // in source ('$' is not a valid identifier), so they never clash.
        self.expression(iterable)?;
        self.chunk.write_op(OpCode::IterSnapshot, line, column);
        let seq_slot = u8::try_from(self.locals.len())
            .map_err(|_| MiruError::new(line, "too many local variables in scope"))?;
        self.declare_local("$seq", line)?;
        self.constant(Value::Int(0), line, column)?;
        self.declare_local("$idx", line)?;

        let loop_start = self.chunk.code.len();
        self.chunk.write_op(OpCode::ForNext, line, column);
        self.chunk.write(seq_slot, line, column);
        self.chunk.write(0xff, line, column);
        self.chunk.write(0xff, line, column);
        let exit_jump = self.chunk.code.len() - 2;

        // The loop variable and body share one fresh scope per iteration.
        self.begin_scope();
        self.declare_local(name, line)?;
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
        for break_jump in context.breaks {
            self.patch_jump(break_jump)?;
        }
        self.end_scope(line, column);
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

    /// Discard each local at or deeper than `min_depth` (popping, or closing a
    /// captured one) without removing them from the compiler's list, since later
    /// code in the same scope still sees them. Used by `break` and `continue`,
    /// which jump over the normal end-of-scope cleanup.
    fn pop_locals_to_depth(&mut self, min_depth: usize, line: usize, column: usize) {
        let ops: Vec<OpCode> = self
            .locals
            .iter()
            .rev()
            .take_while(|local| local.depth >= min_depth)
            .map(|local| {
                if local.captured {
                    OpCode::CloseUpvalue
                } else {
                    OpCode::Pop
                }
            })
            .collect();
        for op in ops {
            self.chunk.write_op(op, line, column);
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

    /// Leave the current scope. Each local it declared is popped, or closed as an
    /// upvalue first if a nested function captured it.
    fn end_scope(&mut self, line: usize, column: usize) {
        self.scope_depth -= 1;
        while matches!(self.locals.last(), Some(local) if local.depth > self.scope_depth) {
            let captured = self.locals.last().expect("a local").captured;
            let op = if captured {
                OpCode::CloseUpvalue
            } else {
                OpCode::Pop
            };
            self.chunk.write_op(op, line, column);
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
            captured: false,
        });
        Ok(())
    }

    /// Find a local variable's stack slot by name, searching innermost first.
    fn resolve_local(&self, name: &str) -> Option<u8> {
        local_slot(&self.locals, name)
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
            ExprKind::Array(elements) => {
                let count = u8::try_from(elements.len()).map_err(|_| {
                    MiruError::with_column(line, column, "array literal has too many elements")
                })?;
                for element in elements {
                    self.expression(element)?;
                }
                self.chunk.write_op(OpCode::Array, line, column);
                self.chunk.write(count, line, column);
            }
            ExprKind::Map(entries) => {
                let count = u8::try_from(entries.len()).map_err(|_| {
                    MiruError::with_column(line, column, "map literal has too many entries")
                })?;
                for (key, value) in entries {
                    self.expression(key)?;
                    self.expression(value)?;
                }
                self.chunk.write_op(OpCode::Map, line, column);
                self.chunk.write(count, line, column);
            }
            ExprKind::Index { target, index } => {
                self.expression(target)?;
                self.expression(index)?;
                self.chunk.write_op(OpCode::Index, index.line, index.column);
            }
            ExprKind::Identifier(name) => {
                if let Some(slot) = self.resolve_local(name) {
                    self.chunk.write_op(OpCode::GetLocal, line, column);
                    self.chunk.write(slot, line, column);
                } else if let Some(upvalue) = self.resolve_upvalue(name)? {
                    self.chunk.write_op(OpCode::GetUpvalue, line, column);
                    self.chunk.write(upvalue, line, column);
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
            ExprKind::Call { callee, arguments } => {
                self.expression(callee)?;
                let argcount = u8::try_from(arguments.len())
                    .map_err(|_| MiruError::with_column(line, column, "too many call arguments"))?;
                for argument in arguments {
                    self.expression(argument)?;
                }
                self.chunk.write_op(OpCode::Call, line, column);
                self.chunk.write(argcount, line, column);
            }
            ExprKind::Function { params, body } => {
                self.function(None, params, body, line, column)?;
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

/// Find a local's stack slot by name in a scope, searching innermost first.
fn local_slot(locals: &[Local], name: &str) -> Option<u8> {
    locals
        .iter()
        .rposition(|local| local.name == name)
        .map(|slot| slot as u8)
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

    #[test]
    fn vm_matches_the_tree_walker_on_arrays_and_for_in() {
        let corpus = [
            "[1, 2, 3]",
            "[]",
            "[1 + 1, 2 * 2, \"a\" + \"b\"]",
            "[1, 2] == [1, 2]",
            "let sum = 0\nfor x in [1, 2, 3, 4] { sum = sum + x }\nsum",
            "let s = \"\"\nfor c in [\"a\", \"b\", \"c\"] { s = s + c }\ns",
            // The loop variable does not leak out and does not touch an outer one.
            "let i = 99\nfor i in [1, 2, 3] { }\ni",
            // An empty iterable runs the body zero times.
            "let sum = 0\nfor x in [] { sum = 1 }\nsum",
            // break and continue inside for-in.
            "let sum = 0\nfor x in [1, 2, 3, 4, 5] {\n  if x == 4 { break }\n  sum = sum + x\n}\nsum",
            "let sum = 0\nfor x in [1, 2, 3, 4] {\n  if x % 2 == 0 { continue }\n  sum = sum + x\n}\nsum",
            // A local declared in the loop body.
            "let sum = 0\nfor x in [1, 2, 3] {\n  let sq = x * x\n  sum = sum + sq\n}\nsum",
            // Iterating a non-array is the same error at the same place.
            "for x in 5 { }",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_functions() {
        let corpus = [
            "fn add(a, b) { return a + b }\nadd(2, 3)",
            "fn square(x) { return x * x }\nsquare(9)",
            // A function value is first class.
            "fn greet() { return \"hi\" }\ngreet",
            // No explicit return yields nil.
            "fn nothing() { }\nnothing()",
            // Recursion through a global name.
            "fn fib(n) {\n  if n < 2 { return n }\n  return fib(n - 1) + fib(n - 2)\n}\nfib(10)",
            "fn fact(n) {\n  if n < 2 { return 1 }\n  return n * fact(n - 1)\n}\nfact(6)",
            // A function called from a loop, accumulating into a global.
            "fn double(x) { return x * 2 }\nlet sum = 0\nfor x in [1, 2, 3] { sum = sum + double(x) }\nsum",
            // Anonymous function bound to a variable.
            "let inc = fn(x) { return x + 1 }\ninc(41)",
            // Early return from inside control flow.
            "fn sign(n) {\n  if n > 0 { return 1 }\n  if n < 0 { return -1 }\n  return 0\n}\nsign(-8)",
            // Local parameters do not leak to globals.
            "fn use_it(p) { return p * 10 }\nlet r = use_it(4)\nr",
            // Errors: wrong arity and calling a non-function, at the call site.
            "fn one(a) { return a }\none(1, 2)",
            "let x = 5\nx(1)",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_closures() {
        let corpus = [
            // Capture a parameter, called after the enclosing function returns.
            "fn make_adder(n) { return fn(x) { return x + n } }\nlet add5 = make_adder(5)\nadd5(10)",
            // Capture and mutate a closed-over variable across several calls.
            "fn make_counter() {\n  let count = 0\n  return fn() { count = count + 1\nreturn count }\n}\nlet c = make_counter()\nlet a = c()\nlet b = c()\na + b",
            // Each closure instance captures its own variable.
            "fn make_counter() {\n  let count = 0\n  return fn() { count = count + 1\nreturn count }\n}\nlet c1 = make_counter()\nlet c2 = make_counter()\nlet a = c1()\nlet b = c1()\nlet d = c2()\na + b + d",
            // Capture an outer local (not a parameter), while it is still live.
            "fn outer() {\n  let base = 100\n  fn inner() { return base + 1 }\n  return inner()\n}\nouter()",
            // Capture through two levels of nesting.
            "fn a() {\n  let x = 10\n  fn b() {\n    fn c() { return x }\n    return c()\n  }\n  return b()\n}\na()",
            // The closure sees writes the enclosing function makes after capture.
            "fn f() {\n  let x = 1\n  let g = fn() { return x }\n  x = 99\n  return g()\n}\nf()",
            // A closure that captures and assigns the outer variable.
            "fn f() {\n  let x = 1\n  let bump = fn() { x = x + 10 }\n  bump()\n  return x\n}\nf()",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_indexing_and_maps() {
        let corpus = [
            // Array and map reads.
            "[10, 20, 30][1]",
            "let a = [1, 2, 3]\na[0] + a[2]",
            "{\"a\": 1, \"b\": 2}[\"a\"]",
            "let m = {\"x\": 10}\nm[\"x\"]",
            // A missing map key reads as nil.
            "{\"a\": 1}[\"missing\"]",
            // Map literals print sorted, computed keys work.
            "{\"b\": 2, \"a\": 1}",
            "let k = \"name\"\nlet m = {k: \"Aiko\"}\nm[\"name\"]",
            // Nested indexing.
            "[[1, 2], [3, 4]][1][0]",
            // Index assignment into arrays and maps.
            "let a = [1, 2, 3]\na[1] = 99\na",
            "let m = {\"a\": 1}\nm[\"b\"] = 2\nm",
            // Iterating over an array built and indexed inline.
            "let sum = 0\nlet a = [5, 6, 7]\nfor x in a { sum = sum + x }\nsum",
            // Errors reported at the index, matching the tree walker.
            "[1, 2, 3][5]",
            "[1, 2, 3][-1]",
            "[1, 2][\"x\"]",
            "{\"a\": 1}[5]",
            "let a = [1, 2, 3]\na[9] = 0",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_builtins() {
        let corpus = [
            "len([1, 2, 3])",
            "len(\"hello\")",
            "type(1) + type(\"a\")",
            "str(42) + \"!\"",
            "range(4)",
            "range(2, 5)",
            "let a = [1]\npush(a, 2)\na",
            "upper(\"abc\") + lower(\"DEF\")",
            "trim(\"  hi  \")",
            "split(\"a,b,c\", \",\")",
            "join([1, 2, 3], \"-\")",
            "sort([3, 1, 2])",
            "reverse([1, 2, 3])",
            "slice([1, 2, 3, 4], 1, 3)",
            "abs(-5) + min(3, 1) + max(2, 7)",
            "floor(2.7) + ceil(2.1) + round(2.5)",
            "sqrt(16)",
            "pow(2, 8)",
            "int(\"42\") + int(2.9)",
            "float(3)",
            "keys({\"b\": 2, \"a\": 1})",
            "values({\"b\": 2, \"a\": 1})",
            "has({\"a\": 1}, \"a\")",
            "contains([1, 2], 2)",
            "index_of([10, 20], 20)",
            "find(\"hello\", \"l\")",
            "pop([1, 2, 3])",
            // A builtin used inside a function and a loop.
            "fn total(xs) {\n  let sum = 0\n  for x in xs { sum = sum + x }\n  return sum\n}\ntotal(range(5))",
            // Builtin errors report the same message at the same place.
            "len(1)",
            "upper(5)",
            "sqrt(-1)",
            "pop([])",
            "int(\"abc\")",
            "sort([1, \"a\"])",
        ];
        for source in corpus {
            agree(source);
        }
    }

    #[test]
    fn vm_matches_the_tree_walker_on_higher_order_builtins() {
        let corpus = [
            "map([1, 2, 3], fn(x) { return x * 2 })",
            "filter([1, 2, 3, 4], fn(x) { return x % 2 == 0 })",
            "reduce([1, 2, 3, 4], fn(acc, x) { return acc + x }, 0)",
            "map([], fn(x) { return x })",
            "reduce([], fn(acc, x) { return acc + x }, 42)",
            // A named function passed by name.
            "fn double(x) { return x * 2 }\nmap([1, 2, 3], double)",
            // A builtin passed as the function argument.
            "map([-1, -2, 3], abs)",
            // A closure capturing an outer variable.
            "let n = 10\nlet add = fn(x) { return x + n }\nmap([1, 2, 3], add)",
            // Chained higher-order calls.
            "reduce(map(filter([1, 2, 3, 4, 5], fn(x) { return x % 2 == 1 }), fn(x) { return x * x }), fn(a, b) { return a + b }, 0)",
            // Nested: a map inside the function applied by another map.
            "map([1, 2], fn(x) { return reduce([1, 2, 3], fn(a, b) { return a + b }, x) })",
            // Higher-order builtins used inside a user function.
            "fn sum(xs) { return reduce(xs, fn(a, b) { return a + b }, 0) }\nsum([4, 5, 6])",
            // Errors from the applied function propagate identically.
            "map([1, 0], fn(x) { return 1 / x })",
            // Errors in the higher-order builtins themselves.
            "map(5, fn(x) { return x })",
            "map([1, 2], 3)",
            "map([1, 2])",
            "filter(5, fn(x) { return true })",
            "reduce([1, 2], fn(a, b) { return a })",
        ];
        for source in corpus {
            agree(source);
        }
    }
}
