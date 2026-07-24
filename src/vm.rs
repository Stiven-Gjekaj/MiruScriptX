//! The bytecode virtual machine: it executes a compiled [`Chunk`] on a value
//! stack.
//!
//! The VM is the second of MiruScriptX's two execution engines. It computes with
//! the same [`Value`]s as the tree walker and applies operators through the same
//! [`crate::ops`] functions, so the two engines agree on results and errors. What
//! differs is how it gets there: instead of walking the AST, it runs a flat
//! stream of bytecode, which avoids the pointer chasing and repeated name
//! lookups that make a tree walker slow.

use std::collections::HashMap;

use crate::ast::{BinaryOp, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::value::Value;
use crate::MiruError;

/// A stack-based bytecode interpreter.
#[derive(Default)]
pub struct Vm {
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
}

impl Vm {
    pub fn new() -> Vm {
        Vm::default()
    }

    /// Execute a chunk to completion, returning the value left on top of the
    /// stack (or `nil` if the stack is empty). Runtime errors carry the source
    /// position of the instruction that failed.
    pub fn interpret(&mut self, chunk: &Chunk) -> Result<Value, MiruError> {
        let mut ip = 0;
        while ip < chunk.code.len() {
            let op_ip = ip;
            let byte = chunk.code[ip];
            ip += 1;
            let op = OpCode::from_u8(byte)
                .ok_or_else(|| runtime_error(chunk, op_ip, format!("unknown opcode {byte}")))?;
            match op {
                OpCode::Constant => {
                    let index = chunk.code[ip] as usize;
                    ip += 1;
                    self.stack.push(chunk.constants[index].clone());
                }
                OpCode::Nil => self.stack.push(Value::Nil),
                OpCode::True => self.stack.push(Value::Bool(true)),
                OpCode::False => self.stack.push(Value::Bool(false)),
                OpCode::Negate => self.unary(UnaryOp::Negate, chunk, op_ip)?,
                OpCode::Not => self.unary(UnaryOp::Not, chunk, op_ip)?,
                OpCode::Add
                | OpCode::Subtract
                | OpCode::Multiply
                | OpCode::Divide
                | OpCode::Modulo
                | OpCode::Equal
                | OpCode::NotEqual
                | OpCode::Less
                | OpCode::Greater
                | OpCode::LessEqual
                | OpCode::GreaterEqual => self.binary(binary_op(op), chunk, op_ip)?,
                OpCode::DefineGlobal => {
                    let name = global_name(chunk, chunk.code[ip]);
                    ip += 1;
                    let value = self.pop();
                    self.globals.insert(name.to_string(), value);
                }
                OpCode::GetGlobal => {
                    let name = global_name(chunk, chunk.code[ip]);
                    ip += 1;
                    match self.globals.get(name) {
                        Some(value) => self.stack.push(value.clone()),
                        None => {
                            return Err(runtime_error(
                                chunk,
                                op_ip,
                                format!("undefined variable '{name}'"),
                            ))
                        }
                    }
                }
                OpCode::SetGlobal => {
                    let name = global_name(chunk, chunk.code[ip]);
                    ip += 1;
                    let value = self.pop();
                    if self.globals.contains_key(name) {
                        self.globals.insert(name.to_string(), value);
                    } else {
                        return Err(runtime_error(
                            chunk,
                            op_ip,
                            format!("cannot assign to undefined variable '{name}'"),
                        ));
                    }
                }
                OpCode::Jump => {
                    let offset = read_u16(chunk, ip);
                    ip += 2 + offset as usize;
                }
                OpCode::JumpIfFalse => {
                    let offset = read_u16(chunk, ip);
                    ip += 2;
                    if !self.peek().is_truthy() {
                        ip += offset as usize;
                    }
                }
                OpCode::JumpIfTrue => {
                    let offset = read_u16(chunk, ip);
                    ip += 2;
                    if self.peek().is_truthy() {
                        ip += offset as usize;
                    }
                }
                OpCode::Truthy => {
                    let value = self.pop();
                    self.stack.push(Value::Bool(value.is_truthy()));
                }
                OpCode::Pop => {
                    self.pop();
                }
                OpCode::Return => return Ok(self.stack.pop().unwrap_or(Value::Nil)),
            }
        }
        Ok(self.stack.pop().unwrap_or(Value::Nil))
    }

    fn unary(&mut self, op: UnaryOp, chunk: &Chunk, offset: usize) -> Result<(), MiruError> {
        let value = self.pop();
        let result = crate::ops::unary(op, value).map_err(|m| runtime_error(chunk, offset, m))?;
        self.stack.push(result);
        Ok(())
    }

    fn binary(&mut self, op: BinaryOp, chunk: &Chunk, offset: usize) -> Result<(), MiruError> {
        let right = self.pop();
        let left = self.pop();
        let result =
            crate::ops::binary(op, left, right).map_err(|m| runtime_error(chunk, offset, m))?;
        self.stack.push(result);
        Ok(())
    }

    /// Pop the top of the stack. A missing value means the compiler emitted
    /// unbalanced bytecode, which is a bug in this crate rather than user error.
    fn pop(&mut self) -> Value {
        self.stack.pop().expect("value stack underflow")
    }

    /// Look at the top of the stack without removing it.
    fn peek(&self) -> &Value {
        self.stack.last().expect("value stack underflow")
    }
}

/// Read a big-endian two-byte operand at `ip`.
fn read_u16(chunk: &Chunk, ip: usize) -> u16 {
    ((chunk.code[ip] as u16) << 8) | (chunk.code[ip + 1] as u16)
}

/// Map a binary opcode to its AST operator. Panics on a non-binary opcode, which
/// only the VM's own dispatch can cause.
fn binary_op(op: OpCode) -> BinaryOp {
    match op {
        OpCode::Add => BinaryOp::Add,
        OpCode::Subtract => BinaryOp::Subtract,
        OpCode::Multiply => BinaryOp::Multiply,
        OpCode::Divide => BinaryOp::Divide,
        OpCode::Modulo => BinaryOp::Modulo,
        OpCode::Equal => BinaryOp::Equal,
        OpCode::NotEqual => BinaryOp::NotEqual,
        OpCode::Less => BinaryOp::Less,
        OpCode::Greater => BinaryOp::Greater,
        OpCode::LessEqual => BinaryOp::LessEqual,
        OpCode::GreaterEqual => BinaryOp::GreaterEqual,
        other => unreachable!("not a binary opcode: {}", other.name()),
    }
}

/// The variable name held as a string constant at `index`. The compiler always
/// emits a string here, so a non-string means the bytecode is malformed.
fn global_name(chunk: &Chunk, index: u8) -> &str {
    match &chunk.constants[index as usize] {
        Value::Str(name) => name.as_str(),
        _ => unreachable!("global name operand is not a string constant"),
    }
}

/// Build a runtime error at the source position of the byte at `offset`.
fn runtime_error(chunk: &Chunk, offset: usize, message: impl Into<String>) -> MiruError {
    let (line, column) = chunk.position(offset);
    MiruError::with_column(line, column, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assemble a chunk from a compact list of ops (each with a dummy position),
    /// where `Constant` is written as its own op followed by pushing the value.
    fn run(build: impl FnOnce(&mut Chunk)) -> Result<Value, MiruError> {
        let mut chunk = Chunk::new();
        build(&mut chunk);
        chunk.write_op(OpCode::Return, 1, 1);
        Vm::new().interpret(&chunk)
    }

    fn constant(chunk: &mut Chunk, value: Value) {
        let index = chunk.add_constant(value) as u8;
        chunk.write_op(OpCode::Constant, 1, 1);
        chunk.write(index, 1, 1);
    }

    #[test]
    fn evaluates_arithmetic() {
        let result = run(|chunk| {
            constant(chunk, Value::Int(2));
            constant(chunk, Value::Int(3));
            chunk.write_op(OpCode::Add, 1, 1);
            constant(chunk, Value::Int(4));
            chunk.write_op(OpCode::Multiply, 1, 1);
        })
        .unwrap();
        assert!(result.equals(&Value::Int(20)));
    }

    #[test]
    fn evaluates_unary_and_comparison() {
        let negated = run(|chunk| {
            constant(chunk, Value::Int(5));
            chunk.write_op(OpCode::Negate, 1, 1);
        })
        .unwrap();
        assert!(negated.equals(&Value::Int(-5)));

        let compared = run(|chunk| {
            constant(chunk, Value::Int(1));
            constant(chunk, Value::Int(2));
            chunk.write_op(OpCode::Less, 1, 1);
        })
        .unwrap();
        assert!(compared.equals(&Value::Bool(true)));
    }

    #[test]
    fn pushes_literals() {
        let value = run(|chunk| {
            chunk.write_op(OpCode::True, 1, 1);
            chunk.write_op(OpCode::Not, 1, 1);
        })
        .unwrap();
        assert!(value.equals(&Value::Bool(false)));
    }

    #[test]
    fn reports_a_runtime_error_with_position() {
        let error = run(|chunk| {
            constant(chunk, Value::Int(1));
            constant(chunk, Value::Int(0));
            chunk.write_op(OpCode::Divide, 7, 9);
        })
        .err()
        .unwrap();
        assert_eq!(error.line, 7);
        assert_eq!(error.column, 9);
        assert_eq!(error.message, "division by zero");
    }
}
