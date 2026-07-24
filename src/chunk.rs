//! Bytecode chunks: the compiled form the virtual machine executes.
//!
//! A [`Chunk`] is a flat stream of bytes (opcodes and their operands), a pool of
//! constant [`Value`]s referred to by index, and a parallel table of source
//! positions. The position table has one `(line, column)` entry per byte, so the
//! VM can point a caret at the exact place a runtime error happened, just as the
//! tree walker does from the AST.

use crate::value::Value;

/// A single virtual-machine instruction. Encoded as one byte in a chunk,
/// sometimes followed by operand bytes (for example, [`OpCode::Constant`] is
/// followed by a one-byte constant index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// Push `constants[operand]` onto the stack. One operand byte.
    Constant,
    /// Push `nil`.
    Nil,
    /// Push `true`.
    True,
    /// Push `false`.
    False,
    /// Negate the number on top of the stack.
    Negate,
    /// Logically negate the value on top of the stack.
    Not,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    /// Pop a value and bind it to a global name. One operand byte: the constant
    /// index of the name.
    DefineGlobal,
    /// Push the value of a global name. One operand byte: the name's constant
    /// index.
    GetGlobal,
    /// Pop a value and assign it to an existing global. One operand byte: the
    /// name's constant index.
    SetGlobal,
    /// Jump forward unconditionally. Two operand bytes: a big-endian distance.
    Jump,
    /// Jump forward if the top of the stack is falsy (peeked, not popped). Two
    /// operand bytes.
    JumpIfFalse,
    /// Jump forward if the top of the stack is truthy (peeked, not popped). Two
    /// operand bytes.
    JumpIfTrue,
    /// Replace the top of the stack with its truthiness as a bool.
    Truthy,
    /// Push the value of a local variable. One operand byte: its stack slot.
    GetLocal,
    /// Pop a value and store it into a local variable's slot. One operand byte:
    /// the stack slot.
    SetLocal,
    /// Discard the value on top of the stack.
    Pop,
    /// Return from the current function (or end the program).
    Return,
}

impl OpCode {
    /// Decode a byte back into an opcode, or `None` if it is not a valid one.
    pub fn from_u8(byte: u8) -> Option<OpCode> {
        use OpCode::*;
        let op = match byte {
            b if b == Constant as u8 => Constant,
            b if b == Nil as u8 => Nil,
            b if b == True as u8 => True,
            b if b == False as u8 => False,
            b if b == Negate as u8 => Negate,
            b if b == Not as u8 => Not,
            b if b == Add as u8 => Add,
            b if b == Subtract as u8 => Subtract,
            b if b == Multiply as u8 => Multiply,
            b if b == Divide as u8 => Divide,
            b if b == Modulo as u8 => Modulo,
            b if b == Equal as u8 => Equal,
            b if b == NotEqual as u8 => NotEqual,
            b if b == Less as u8 => Less,
            b if b == Greater as u8 => Greater,
            b if b == LessEqual as u8 => LessEqual,
            b if b == GreaterEqual as u8 => GreaterEqual,
            b if b == DefineGlobal as u8 => DefineGlobal,
            b if b == GetGlobal as u8 => GetGlobal,
            b if b == SetGlobal as u8 => SetGlobal,
            b if b == Jump as u8 => Jump,
            b if b == JumpIfFalse as u8 => JumpIfFalse,
            b if b == JumpIfTrue as u8 => JumpIfTrue,
            b if b == Truthy as u8 => Truthy,
            b if b == GetLocal as u8 => GetLocal,
            b if b == SetLocal as u8 => SetLocal,
            b if b == Pop as u8 => Pop,
            b if b == Return as u8 => Return,
            _ => return None,
        };
        Some(op)
    }

    /// A short mnemonic used by the disassembler.
    pub fn name(self) -> &'static str {
        match self {
            OpCode::Constant => "CONSTANT",
            OpCode::Nil => "NIL",
            OpCode::True => "TRUE",
            OpCode::False => "FALSE",
            OpCode::Negate => "NEGATE",
            OpCode::Not => "NOT",
            OpCode::Add => "ADD",
            OpCode::Subtract => "SUBTRACT",
            OpCode::Multiply => "MULTIPLY",
            OpCode::Divide => "DIVIDE",
            OpCode::Modulo => "MODULO",
            OpCode::Equal => "EQUAL",
            OpCode::NotEqual => "NOT_EQUAL",
            OpCode::Less => "LESS",
            OpCode::Greater => "GREATER",
            OpCode::LessEqual => "LESS_EQUAL",
            OpCode::GreaterEqual => "GREATER_EQUAL",
            OpCode::DefineGlobal => "DEFINE_GLOBAL",
            OpCode::GetGlobal => "GET_GLOBAL",
            OpCode::SetGlobal => "SET_GLOBAL",
            OpCode::Jump => "JUMP",
            OpCode::JumpIfFalse => "JUMP_IF_FALSE",
            OpCode::JumpIfTrue => "JUMP_IF_TRUE",
            OpCode::Truthy => "TRUTHY",
            OpCode::GetLocal => "GET_LOCAL",
            OpCode::SetLocal => "SET_LOCAL",
            OpCode::Pop => "POP",
            OpCode::Return => "RETURN",
        }
    }
}

/// A compiled chunk of bytecode: the instructions, the constants they reference,
/// and the source position of every byte.
#[derive(Clone, Default)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<Value>,
    /// One `(line, column)` per byte in `code`, so `code.len() == positions.len()`.
    pub positions: Vec<(usize, usize)>,
}

impl Chunk {
    pub fn new() -> Chunk {
        Chunk::default()
    }

    /// Append a raw byte (an opcode or an operand), tagged with its source
    /// position.
    pub fn write(&mut self, byte: u8, line: usize, column: usize) {
        self.code.push(byte);
        self.positions.push((line, column));
    }

    /// Append an opcode byte.
    pub fn write_op(&mut self, op: OpCode, line: usize, column: usize) {
        self.write(op as u8, line, column);
    }

    /// Add a constant to the pool and return its index, for use as an operand.
    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.push(value);
        self.constants.len() - 1
    }

    /// The source position of the byte at `offset`, or `(0, 0)` when unknown.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        self.positions.get(offset).copied().unwrap_or((0, 0))
    }

    /// Render the whole chunk as human-readable assembly, one instruction per
    /// line. Used for debugging the compiler and the VM.
    pub fn disassemble(&self, name: &str) -> String {
        let mut out = format!("== {name} ==\n");
        let mut offset = 0;
        while offset < self.code.len() {
            offset = self.disassemble_instruction(&mut out, offset);
        }
        out
    }

    /// Disassemble the single instruction at `offset` into `out`, returning the
    /// offset of the next instruction.
    fn disassemble_instruction(&self, out: &mut String, offset: usize) -> usize {
        use std::fmt::Write;

        let _ = write!(out, "{offset:04} ");
        match OpCode::from_u8(self.code[offset]) {
            Some(
                op @ (OpCode::Constant
                | OpCode::DefineGlobal
                | OpCode::GetGlobal
                | OpCode::SetGlobal),
            ) => {
                let index = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let value = self
                    .constants
                    .get(index)
                    .map(Value::repr)
                    .unwrap_or_else(|| "?".to_string());
                let _ = writeln!(out, "{:<14}{index} ({value})", op.name());
                offset + 2
            }
            Some(op @ (OpCode::Jump | OpCode::JumpIfFalse | OpCode::JumpIfTrue)) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let target = offset + 3 + ((high << 8) | low);
                let _ = writeln!(out, "{:<14}{offset} -> {target}", op.name());
                offset + 3
            }
            Some(op @ (OpCode::GetLocal | OpCode::SetLocal)) => {
                let slot = self.code.get(offset + 1).copied().unwrap_or(0);
                let _ = writeln!(out, "{:<14}slot {slot}", op.name());
                offset + 2
            }
            Some(op) => {
                let _ = writeln!(out, "{}", op.name());
                offset + 1
            }
            None => {
                let _ = writeln!(out, "UNKNOWN {}", self.code[offset]);
                offset + 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_round_trip_through_bytes() {
        for op in [
            OpCode::Constant,
            OpCode::Nil,
            OpCode::Return,
            OpCode::GreaterEqual,
            OpCode::Modulo,
        ] {
            assert_eq!(OpCode::from_u8(op as u8), Some(op));
        }
        assert_eq!(OpCode::from_u8(250), None);
    }

    #[test]
    fn add_constant_returns_increasing_indices() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.add_constant(Value::Int(1)), 0);
        assert_eq!(chunk.add_constant(Value::Int(2)), 1);
        assert_eq!(chunk.constants.len(), 2);
    }

    #[test]
    fn positions_track_each_byte_and_clamp_out_of_range() {
        let mut chunk = Chunk::new();
        let index = chunk.add_constant(Value::Int(42)) as u8;
        chunk.write_op(OpCode::Constant, 3, 5);
        chunk.write(index, 3, 5);
        chunk.write_op(OpCode::Return, 3, 7);
        assert_eq!(chunk.code.len(), chunk.positions.len());
        assert_eq!(chunk.position(0), (3, 5));
        assert_eq!(chunk.position(2), (3, 7));
        assert_eq!(chunk.position(99), (0, 0));
    }

    #[test]
    fn disassembles_a_small_chunk() {
        let mut chunk = Chunk::new();
        let a = chunk.add_constant(Value::Int(1)) as u8;
        let b = chunk.add_constant(Value::Int(2)) as u8;
        chunk.write_op(OpCode::Constant, 1, 1);
        chunk.write(a, 1, 1);
        chunk.write_op(OpCode::Constant, 1, 5);
        chunk.write(b, 1, 5);
        chunk.write_op(OpCode::Add, 1, 3);
        chunk.write_op(OpCode::Return, 1, 1);
        let text = chunk.disassemble("test");
        assert_eq!(
            text,
            "== test ==\n\
             0000 CONSTANT      0 (1)\n\
             0002 CONSTANT      1 (2)\n\
             0004 ADD\n\
             0005 RETURN\n"
        );
    }
}
