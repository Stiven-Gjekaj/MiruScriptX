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

use crate::ast::{BinaryOp, Expr, ExprKind, Stmt, StmtKind, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::value::Value;
use crate::MiruError;

/// Compiles an AST into a [`Chunk`].
pub struct Compiler {
    chunk: Chunk,
}

impl Compiler {
    /// Compile a whole program into a chunk that ends in a `Return`. The value
    /// returned by the VM is that of the program's final expression, matching
    /// what the tree walker's `run_program` yields.
    pub fn compile(program: &[Stmt]) -> Result<Chunk, MiruError> {
        let mut compiler = Compiler {
            chunk: Chunk::new(),
        };
        compiler.program(program)?;
        let (line, column) = program.last().map(|stmt| (stmt.line, 1)).unwrap_or((0, 0));
        compiler.chunk.write_op(OpCode::Return, line, column);
        Ok(compiler.chunk)
    }

    fn program(&mut self, program: &[Stmt]) -> Result<(), MiruError> {
        if program.is_empty() {
            self.chunk.write_op(OpCode::Nil, 0, 0);
            return Ok(());
        }
        let last = program.len() - 1;
        for (index, stmt) in program.iter().enumerate() {
            self.statement(stmt, index == last)?;
        }
        Ok(())
    }

    /// Compile one statement. When it is the program's last, it leaves exactly one
    /// value on the stack (the expression's value, or `nil`); otherwise it leaves
    /// the stack unchanged. This makes the VM return the same value the tree
    /// walker's `run_program` does.
    fn statement(&mut self, stmt: &Stmt, is_last: bool) -> Result<(), MiruError> {
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                self.expression(expr)?;
                if !is_last {
                    self.chunk.write_op(OpCode::Pop, stmt.line, 1);
                }
            }
            StmtKind::Let { name, value } => {
                self.expression(value)?;
                self.named_global(OpCode::DefineGlobal, name, stmt.line, 1)?;
                if is_last {
                    self.chunk.write_op(OpCode::Nil, stmt.line, 1);
                }
            }
            StmtKind::Assign { target, value } => {
                self.expression(value)?;
                match &target.kind {
                    ExprKind::Identifier(name) => {
                        self.named_global(OpCode::SetGlobal, name, target.line, target.column)?;
                    }
                    _ => {
                        return Err(MiruError::with_column(
                            target.line,
                            target.column,
                            "the bytecode VM does not support this assignment yet",
                        ));
                    }
                }
                if is_last {
                    self.chunk.write_op(OpCode::Nil, stmt.line, 1);
                }
            }
            _ => {
                return Err(MiruError::new(
                    stmt.line,
                    "the bytecode VM does not support this statement yet",
                ));
            }
        }
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
                self.named_global(OpCode::GetGlobal, name, line, column)?
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
}
