//! The bytecode compiler: it walks the AST and emits a [`Chunk`] for the VM.
//!
//! Compiling happens once per program, so the work of resolving names to slots,
//! laying out control flow as jumps, and folding each construct into
//! instructions is paid a single time rather than on every evaluation.

use std::rc::Rc;

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::globals::Globals;
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
pub struct Compiler<'g> {
    /// The shared global table, so a name gets the same slot across the
    /// separately compiled inputs of a session.
    globals: &'g mut Globals,
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

impl<'g> Compiler<'g> {
    fn new(globals: &'g mut Globals) -> Compiler<'g> {
        Compiler {
            globals,
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
    /// expression, which is what the REPL echoes.
    pub fn compile(
        program: &[Stmt],
        globals: &'g mut Globals,
    ) -> Result<Rc<CompiledFunction>, MiruError> {
        let mut compiler = Compiler::new(globals);
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
        // The program's value is that of a trailing expression, or nil otherwise.
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
                        self.chunk.write(0, object.line, object.column);
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
        // An operator applied to literals is computed here rather than every
        // time the program runs. Only compound expressions are worth folding: a
        // bare literal already compiles to a single instruction.
        if matches!(expr.kind, ExprKind::Unary { .. } | ExprKind::Binary { .. }) {
            if let Some(value) = fold(expr) {
                return self.emit_value(value, line, column);
            }
        }
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
                // The opcode carries the index's position and its operand byte
                // the target's, so each index error points at the part at fault.
                self.chunk.write_op(OpCode::Index, index.line, index.column);
                self.chunk.write(0, target.line, target.column);
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

    /// Push a known value using the cheapest encoding: `true`, `false`, and
    /// `nil` have dedicated single-byte opcodes, so folding to one of those must
    /// not turn it into a two-byte constant load.
    fn emit_value(&mut self, value: Value, line: usize, column: usize) -> Result<(), MiruError> {
        match value {
            Value::Bool(true) => self.chunk.write_op(OpCode::True, line, column),
            Value::Bool(false) => self.chunk.write_op(OpCode::False, line, column),
            Value::Nil => self.chunk.write_op(OpCode::Nil, line, column),
            other => self.constant(other, line, column)?,
        }
        Ok(())
    }

    /// Emit a `Constant` instruction that pushes `value`.
    fn constant(&mut self, value: Value, line: usize, column: usize) -> Result<(), MiruError> {
        let index = self.constant_index(value, line, column)?;
        self.chunk.write_op(OpCode::Constant, line, column);
        self.chunk.write(index, line, column);
        Ok(())
    }

    /// Emit a global instruction (`DefineGlobal`, `GetGlobal`, or `SetGlobal`),
    /// resolving the name to its slot in the shared table. Resolving here rather
    /// than at run time is the point: the VM indexes instead of hashing.
    fn named_global(
        &mut self,
        op: OpCode,
        name: &str,
        line: usize,
        column: usize,
    ) -> Result<(), MiruError> {
        let slot = self.globals.slot_for(name).ok_or_else(|| {
            MiruError::with_column(line, column, "too many global variables in one program")
        })?;
        self.chunk.write_op(op, line, column);
        self.chunk.write((slot >> 8) as u8, line, column);
        self.chunk.write((slot & 0xff) as u8, line, column);
        Ok(())
    }
}

/// Try to evaluate an expression at compile time.
///
/// Returns `None` when the expression is not made only of literals, and also
/// when evaluating it *fails*. That second case matters: `1 / 0` and an
/// overflowing sum have to stay runtime errors reported at their own position,
/// so a fold that would raise is abandoned and the instructions are emitted as
/// usual, leaving the failure to happen where it always did.
fn fold(expr: &Expr) -> Option<Value> {
    match &expr.kind {
        ExprKind::Int(n) => Some(Value::Int(*n)),
        ExprKind::Float(f) => Some(Value::Float(*f)),
        ExprKind::Str(s) => Some(Value::Str(Rc::new(s.clone()))),
        ExprKind::Bool(b) => Some(Value::Bool(*b)),
        ExprKind::Nil => Some(Value::Nil),
        ExprKind::Unary { op, operand } => crate::ops::unary(*op, fold(operand)?).ok(),
        ExprKind::Binary { op, left, right } => {
            crate::ops::binary(*op, fold(left)?, fold(right)?).ok()
        }
        _ => None,
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
