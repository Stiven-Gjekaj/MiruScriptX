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
        let mut has_value = false;
        for stmt in program {
            match &stmt.kind {
                StmtKind::Expr(expr) => {
                    // Only the final expression's value is kept; discard earlier
                    // ones so the stack does not grow.
                    if has_value {
                        self.chunk.write_op(OpCode::Pop, stmt.line, 1);
                    }
                    self.expression(expr)?;
                    has_value = true;
                }
                _ => {
                    return Err(MiruError::new(
                        stmt.line,
                        "the bytecode VM does not support this statement yet",
                    ));
                }
            }
        }
        if !has_value {
            self.chunk.write_op(OpCode::Nil, 0, 0);
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

    /// Emit a `Constant` instruction, adding `value` to the pool. A chunk holds
    /// at most 256 constants, since the index is a single byte.
    fn constant(&mut self, value: Value, line: usize, column: usize) -> Result<(), MiruError> {
        let index = self.chunk.add_constant(value);
        if index > u8::MAX as usize {
            return Err(MiruError::with_column(
                line,
                column,
                "too many constants in one chunk",
            ));
        }
        self.chunk.write_op(OpCode::Constant, line, column);
        self.chunk.write(index as u8, line, column);
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
}
