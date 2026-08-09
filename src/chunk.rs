//! Bytecode chunks: the compiled form the virtual machine executes.
//!
//! A [`Chunk`] is a flat stream of bytes (opcodes and their operands), a pool of
//! constant [`Value`]s referred to by index, and a parallel table of source
//! positions. The position table has one `(line, column)` entry per byte, so a
//! runtime error can point a caret at the exact place it happened even though
//! the syntax tree is long gone by then.

use std::collections::HashMap;
use std::rc::Rc;

use crate::value::{CompiledFunction, Value};

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
    /// Pop a value and bind it to a global. Two operand bytes: a big-endian slot
    /// in the shared global table.
    DefineGlobal,
    /// Push the value of a global. Two operand bytes: its slot. A slot that has
    /// never been defined is the "undefined variable" error.
    GetGlobal,
    /// Pop a value and assign it to an already defined global. Two operand
    /// bytes: its slot.
    SetGlobal,
    /// Jump forward unconditionally. Two operand bytes: a big-endian distance.
    Jump,
    /// Jump forward if the top of the stack is falsy (peeked, not popped). Two
    /// operand bytes.
    JumpIfFalse,
    /// Jump forward if the top of the stack is truthy (peeked, not popped). Two
    /// operand bytes.
    JumpIfTrue,
    /// Jump backward unconditionally, for looping. Two operand bytes: a
    /// big-endian distance that is subtracted.
    Loop,
    /// Replace the top of the stack with its truthiness as a bool.
    Truthy,
    /// Push the value of a local variable. One operand byte: its stack slot.
    GetLocal,
    /// Pop a value and store it into a local variable's slot. One operand byte:
    /// the stack slot.
    SetLocal,
    /// Build an array from the top values on the stack. One operand byte: the
    /// element count. Pops that many values and pushes the array.
    Array,
    /// Build a map from the top values on the stack. One operand byte: the entry
    /// count. Pops that many key/value pairs and pushes the map.
    Map,
    /// Index into an array or map: pop the index and the target, push the element
    /// (or `nil` for a missing map key).
    ///
    /// Carries one operand byte that holds no value. It exists so the position
    /// table records two positions for this instruction: the index expression
    /// (on the opcode byte), which out-of-range and bad-key errors point at, and
    /// the target expression (on the operand byte), which "cannot index" points
    /// at. Each error then lands its caret under the part actually at fault.
    Index,
    /// Assign through an index: pop the index, the target, and the value, and
    /// store the value at that index or key. Carries the same position-only
    /// operand byte as [`OpCode::Index`].
    SetIndex,
    /// Read a field: pop the field name and the target, and push the value
    /// stored under that name. Carries the same position-only operand byte as
    /// [`OpCode::Index`], so "cannot read a field" points at the target while a
    /// missing field points at the access.
    ///
    /// The same shape as [`OpCode::Index`] with one difference that is the whole
    /// reason it exists: a field that is not there is an error, where a map key
    /// that is not there reads as `nil`. Indexing is a lookup that may miss;
    /// field access asserts the field is present. A module is reached this way,
    /// so a mistyped member fails where it is written.
    ///
    /// Taking the name off the stack rather than in an operand means it is an
    /// ordinary constant, so it inherits `ConstantLong` and is not capped at the
    /// 256 a one-byte operand would allow.
    GetField,
    /// Pop the iterable, check it is an array, and push a snapshot to iterate. A
    /// runtime error otherwise.
    /// One operand byte: 0 snapshots the elements, 1 snapshots pairs.
    ///
    /// Pairs are what a two-variable `for` walks. A map gives `[key, value]`
    /// and an array gives `[index, element]`. The byte is here rather than in
    /// a second opcode because the two differ only in what they build, and
    /// because a map with one loop variable has to stay the error it was.
    IterSnapshot,
    /// Drive a `for` loop. Operands: the snapshot's slot (two bytes, big-endian)
    /// and a big-endian exit distance (two bytes). The index lives in the next
    /// slot. If the index is past the end, jump by the distance; otherwise push
    /// the next element and advance the index.
    ///
    /// The slot is wide because it is a local slot like any other, and locals
    /// are addressed by two bytes once a function has more than 256 of them.
    /// This instruction runs once per iteration rather than in an inner loop, so
    /// it takes the byte rather than needing a `Long` twin.
    ForNext,
    /// Push a closure built from a nested function. Operands: the function's
    /// index in the pool (two bytes, big-endian), an upvalue count (one byte),
    /// then three bytes per upvalue: whether it captures a local (1) or an
    /// enclosing upvalue (0), then two big-endian bytes holding that local slot
    /// or upvalue index.
    ///
    /// The per-upvalue index is wide for the same reason as `ForNext`: when the
    /// flag says local, the index *is* a local slot.
    Closure,
    /// Push the value of one of the current closure's upvalues. One operand byte:
    /// the upvalue index.
    GetUpvalue,
    /// Pop a value and store it into one of the current closure's upvalues. One
    /// operand byte: the upvalue index.
    SetUpvalue,
    /// Close the upvalue over the local on top of the stack, then pop it.
    CloseUpvalue,
    /// Call the value beneath the arguments on the stack. One operand byte: the
    /// argument count.
    Call,
    /// Discard the value on top of the stack.
    Pop,
    /// Push copies of the top two values, keeping their order.
    ///
    /// `[a, b]` becomes `[a, b, a, b]`. Added for compound assignment, which
    /// has to read through a target and then store through the same target
    /// without evaluating its parts twice.
    DupTwo,
    /// Move the top value below the two under it.
    ///
    /// `[a, b, c]` becomes `[c, a, b]`. The other half of compound assignment:
    /// the new value is computed on top of the target's parts and has to end
    /// up underneath them, because that is the order `SetIndex` and `SetField`
    /// read their operands in.
    Rot3,
    /// Return from the current function (or end the program).
    Return,
    /// Apply a binary operator whose right operand is a constant, to the value
    /// on top of the stack. Two operand bytes: the byte of the plain opcode for
    /// the operator (`Add` through `GreaterEqual`), then the constant's index.
    ///
    /// This is the same work as `Constant` followed by that operator, in one
    /// instruction and without the round trip through the stack. All three of
    /// its bytes carry the operator's position, so an error it raises points
    /// where the unfused pair pointed.
    ///
    /// The constant index is one byte, so this is not emitted for a constant
    /// beyond the first 256. Such a program falls back to the unfused pair
    /// rather than growing this instruction for every program that never needs
    /// it.
    BinaryConst,
    /// Push `constants[operand]` where the index does not fit in one byte. Two
    /// operand bytes, big-endian.
    ///
    /// The wide form of [`OpCode::Constant`], emitted only when the pool has
    /// outgrown a single byte. Loading a constant is one of the two hottest
    /// instructions in the language, so it keeps its short encoding for the
    /// programs that fit and pays the extra byte only where it is needed.
    ConstantLong,
    /// Push the value of a local variable whose slot does not fit in one byte.
    /// Two operand bytes, big-endian.
    ///
    /// The wide form of [`OpCode::GetLocal`], and the same bargain as
    /// [`OpCode::ConstantLong`]: reading a local is the other hottest
    /// instruction in the language, so the short form stays for the functions
    /// that fit in 256 slots, which is nearly all of them.
    GetLocalLong,
    /// The wide form of [`OpCode::SetLocal`]. Two operand bytes, big-endian.
    SetLocalLong,
    /// Install an error handler for the expression that follows. Two operand
    /// bytes: a big-endian distance to the landing, measured the same way a
    /// `Jump`'s is.
    ///
    /// If evaluating that expression fails, at any call depth, the VM unwinds
    /// to the state recorded here, pushes the error as a value, and continues
    /// at the landing. Both paths therefore leave exactly one value where the
    /// expression's result belongs, and converge on the same instruction, so
    /// the success path needs no jump over the error path.
    BeginTry,
    /// Discard the handler installed by the matching [`OpCode::BeginTry`],
    /// leaving the expression's value in place. Reached only when nothing
    /// failed.
    EndTry,
    /// Assign through a field: pop the field name, the target, and the value,
    /// and store the value under that name. Carries the same position-only
    /// operand byte as [`OpCode::SetIndex`].
    ///
    /// Unlike [`OpCode::GetField`], a name that is not there is not an error.
    /// Reading a field asserts it is present, because a misspelling should
    /// fail; assigning to one that is absent is how a field gets there in the
    /// first place, which is what `m["a"] = 1` has always done.
    SetField,
}

/// Every opcode in declaration order, so a byte decodes by indexing rather than
/// by comparison. The order must match the enum, which `opcodes_match_their_byte`
/// checks.
const OPCODES: [OpCode; 50] = [
    OpCode::Constant,
    OpCode::Nil,
    OpCode::True,
    OpCode::False,
    OpCode::Negate,
    OpCode::Not,
    OpCode::Add,
    OpCode::Subtract,
    OpCode::Multiply,
    OpCode::Divide,
    OpCode::Modulo,
    OpCode::Equal,
    OpCode::NotEqual,
    OpCode::Less,
    OpCode::Greater,
    OpCode::LessEqual,
    OpCode::GreaterEqual,
    OpCode::DefineGlobal,
    OpCode::GetGlobal,
    OpCode::SetGlobal,
    OpCode::Jump,
    OpCode::JumpIfFalse,
    OpCode::JumpIfTrue,
    OpCode::Loop,
    OpCode::Truthy,
    OpCode::GetLocal,
    OpCode::SetLocal,
    OpCode::Array,
    OpCode::Map,
    OpCode::Index,
    OpCode::SetIndex,
    OpCode::GetField,
    OpCode::IterSnapshot,
    OpCode::ForNext,
    OpCode::Closure,
    OpCode::GetUpvalue,
    OpCode::SetUpvalue,
    OpCode::CloseUpvalue,
    OpCode::Call,
    OpCode::Pop,
    OpCode::DupTwo,
    OpCode::Rot3,
    OpCode::Return,
    OpCode::BinaryConst,
    OpCode::ConstantLong,
    OpCode::GetLocalLong,
    OpCode::SetLocalLong,
    OpCode::BeginTry,
    OpCode::EndTry,
    OpCode::SetField,
];

impl OpCode {
    /// Decode a byte back into an opcode, or `None` if it is not a valid one.
    ///
    /// This indexes a table. Written as a match, each decode walked a chain of
    /// comparisons proportional to the number of opcodes. The disassembler uses
    /// this; the interpreter uses [`OpCode::decode`], which does not ask.
    #[inline]
    pub fn from_u8(byte: u8) -> Option<OpCode> {
        OPCODES.get(byte as usize).copied()
    }

    /// Decode a byte that the compiler emitted in an opcode position.
    ///
    /// The compiler is the only producer of chunks and there is no bytecode
    /// reader, so such a byte is a valid opcode by construction. This panics
    /// rather than reporting an error for the same reason the value stack panics
    /// on underflow: an error can only mean a bug in this crate, never anything
    /// a program can do. It runs on every instruction, where the `Option` the
    /// caller then has to answer for costs more than it can ever catch.
    #[inline]
    pub fn decode(byte: u8) -> OpCode {
        OPCODES[byte as usize]
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
            OpCode::Loop => "LOOP",
            OpCode::Truthy => "TRUTHY",
            OpCode::GetLocal => "GET_LOCAL",
            OpCode::SetLocal => "SET_LOCAL",
            OpCode::Array => "ARRAY",
            OpCode::Map => "MAP",
            OpCode::Index => "INDEX",
            OpCode::GetField => "GET_FIELD",
            OpCode::SetIndex => "SET_INDEX",
            OpCode::IterSnapshot => "ITER_SNAPSHOT",
            OpCode::ForNext => "FOR_NEXT",
            OpCode::Closure => "CLOSURE",
            OpCode::GetUpvalue => "GET_UPVALUE",
            OpCode::SetUpvalue => "SET_UPVALUE",
            OpCode::CloseUpvalue => "CLOSE_UPVALUE",
            OpCode::Call => "CALL",
            OpCode::Pop => "POP",
            OpCode::DupTwo => "DUP2",
            OpCode::Rot3 => "ROT3",
            OpCode::Return => "RETURN",
            OpCode::BinaryConst => "BINARY_CONST",
            OpCode::ConstantLong => "CONSTANT_LONG",
            OpCode::GetLocalLong => "GET_LOCAL_LONG",
            OpCode::SetLocalLong => "SET_LOCAL_LONG",
            OpCode::BeginTry => "BEGIN_TRY",
            OpCode::EndTry => "END_TRY",
            OpCode::SetField => "SET_FIELD",
        }
    }
}

/// A value's identity as a constant, used to find an existing pool entry.
///
/// This exists rather than hashing a [`Value`] directly because pool identity is
/// stricter than language equality: `1` and `1.0` are equal to the language but
/// must not share a slot, or one literal would be rewritten as the other and
/// change what `type` and `str` report. Floats key on their bits for the same
/// reason, which also keeps `0.0` and `-0.0` apart.
///
/// Only the kinds a literal can produce have a key. Everything else carries
/// identity rather than a value, never reaches a constant pool, and simply does
/// not participate in reuse.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ConstantKey {
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(Rc<String>),
    Nil,
}

impl ConstantKey {
    fn of(value: &Value) -> Option<ConstantKey> {
        match value {
            Value::Int(n) => Some(ConstantKey::Int(*n)),
            Value::Float(f) => Some(ConstantKey::Float(f.to_bits())),
            Value::Bool(b) => Some(ConstantKey::Bool(*b)),
            Value::Str(s) => Some(ConstantKey::Str(Rc::clone(s))),
            Value::Nil => Some(ConstantKey::Nil),
            _ => None,
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
    /// Nested functions this chunk can turn into closures, by index.
    pub functions: Vec<Rc<CompiledFunction>>,
    /// Where each constant already in the pool sits, so reuse is a lookup rather
    /// than a search. Compile-time only; the VM addresses constants by index.
    constant_slots: HashMap<ConstantKey, usize>,
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

    /// Add a constant to the pool and return its index, for use as an operand,
    /// reusing the entry for a value already in the pool.
    ///
    /// Reuse is what makes the pool usable, not a saving. Without it every
    /// *occurrence* of a literal took a slot: a three-hundred-line program that
    /// added `1` to a counter on each line failed to compile, back when the pool
    /// was capped at what one operand byte could address.
    ///
    /// The lookup is by hash. It was a linear scan while that cap was 256, which
    /// bounded the search; `ConstantLong` raised the cap to 65,536 and took the
    /// bound with it, turning compilation of a constant-heavy program quadratic.
    pub fn add_constant(&mut self, value: Value) -> usize {
        let key = ConstantKey::of(&value);
        if let Some(index) = key.as_ref().and_then(|key| self.constant_slots.get(key)) {
            return *index;
        }
        self.constants.push(value);
        let index = self.constants.len() - 1;
        if let Some(key) = key {
            self.constant_slots.insert(key, index);
        }
        index
    }

    /// Add a nested function and return its index, for use as a `Closure` operand.
    pub fn add_function(&mut self, function: Rc<CompiledFunction>) -> usize {
        self.functions.push(function);
        self.functions.len() - 1
    }

    /// The source position of the byte at `offset`, or `(0, 0)` when unknown.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        self.positions.get(offset).copied().unwrap_or((0, 0))
    }

    /// Render the whole chunk as human-readable assembly, one instruction per
    /// line. Used for debugging the compiler and the VM.
    pub fn disassemble(&self, name: &str) -> String {
        use std::fmt::Write;

        let mut out = format!("== {name} ==\n");
        let mut offset = 0;
        let mut previous_line = 0;
        while offset < self.code.len() {
            // Show the source line an instruction came from, and a bar when it
            // continues the line above, so a statement's instructions read as one
            // group rather than a column of repeated numbers.
            let line = self.position(offset).0;
            if line == previous_line {
                let _ = write!(out, "   |  ");
            } else {
                let _ = write!(out, "{line:>4}  ");
                previous_line = line;
            }
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
            Some(op @ OpCode::Constant) => {
                let index = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let value = self
                    .constants
                    .get(index)
                    .map(Value::repr)
                    .unwrap_or_else(|| "?".to_string());
                let _ = writeln!(out, "{:<14}{index} ({value})", op.name());
                offset + 2
            }
            Some(op @ OpCode::BinaryConst) => {
                let operator = self
                    .code
                    .get(offset + 1)
                    .copied()
                    .and_then(OpCode::from_u8)
                    .map(OpCode::name)
                    .unwrap_or("?");
                let index = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let value = self
                    .constants
                    .get(index)
                    .map(Value::repr)
                    .unwrap_or_else(|| "?".to_string());
                let _ = writeln!(out, "{:<14}{operator} {index} ({value})", op.name());
                offset + 3
            }
            Some(
                op @ (OpCode::Jump | OpCode::JumpIfFalse | OpCode::JumpIfTrue | OpCode::BeginTry),
            ) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let target = offset + 3 + ((high << 8) | low);
                let _ = writeln!(out, "{:<14}{offset} -> {target}", op.name());
                offset + 3
            }
            Some(OpCode::Loop) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let target = (offset + 3).saturating_sub((high << 8) | low);
                let _ = writeln!(out, "{:<14}{offset} -> {target}", OpCode::Loop.name());
                offset + 3
            }
            Some(op @ (OpCode::GetLocal | OpCode::SetLocal)) => {
                let slot = self.code.get(offset + 1).copied().unwrap_or(0);
                let _ = writeln!(out, "{:<14}slot {slot}", op.name());
                offset + 2
            }
            Some(op @ (OpCode::GetLocalLong | OpCode::SetLocalLong)) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let _ = writeln!(out, "{:<14}slot {}", op.name(), (high << 8) | low);
                offset + 3
            }
            Some(op @ OpCode::ConstantLong) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let index = (high << 8) | low;
                let value = self
                    .constants
                    .get(index)
                    .map(Value::repr)
                    .unwrap_or_else(|| "?".to_string());
                let _ = writeln!(out, "{:<14}{index} ({value})", op.name());
                offset + 3
            }
            Some(op @ (OpCode::Array | OpCode::Map)) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let _ = writeln!(out, "{:<14}{}", op.name(), (high << 8) | low);
                offset + 3
            }
            Some(op @ (OpCode::Call | OpCode::GetUpvalue | OpCode::SetUpvalue)) => {
                let operand = self.code.get(offset + 1).copied().unwrap_or(0);
                let _ = writeln!(out, "{:<14}{operand}", op.name());
                offset + 2
            }
            Some(op @ (OpCode::DefineGlobal | OpCode::GetGlobal | OpCode::SetGlobal)) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let _ = writeln!(out, "{:<14}slot {}", op.name(), (high << 8) | low);
                offset + 3
            }
            Some(op @ OpCode::IterSnapshot) => {
                let pairs = self.code.get(offset + 1).copied().unwrap_or(0);
                let what = if pairs == 1 { "pairs" } else { "elements" };
                let _ = writeln!(out, "{:<14}{what}", op.name());
                offset + 2
            }
            Some(op @ (OpCode::Index | OpCode::SetIndex | OpCode::GetField | OpCode::SetField)) => {
                // The operand byte carries only a source position, so there is
                // no value to show for it.
                let _ = writeln!(out, "{}", op.name());
                offset + 2
            }
            Some(OpCode::Closure) => {
                let high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let function = (high << 8) | low;
                let upvalues = self.code.get(offset + 3).copied().unwrap_or(0) as usize;
                let _ = writeln!(
                    out,
                    "{:<14}fn {function} ({upvalues} upvalue(s))",
                    OpCode::Closure.name()
                );
                offset + 4 + upvalues * 3
            }
            Some(OpCode::ForNext) => {
                let slot_high = self.code.get(offset + 1).copied().unwrap_or(0) as usize;
                let slot_low = self.code.get(offset + 2).copied().unwrap_or(0) as usize;
                let slot = (slot_high << 8) | slot_low;
                let high = self.code.get(offset + 3).copied().unwrap_or(0) as usize;
                let low = self.code.get(offset + 4).copied().unwrap_or(0) as usize;
                let target = offset + 5 + ((high << 8) | low);
                let _ = writeln!(out, "{:<14}slot {slot} -> {target}", OpCode::ForNext.name());
                offset + 5
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

/// Disassemble a whole program: the top-level script followed by every function
/// nested inside it, depth first, each under its own heading.
pub fn disassemble_program(script: &CompiledFunction) -> String {
    let mut out = String::new();
    disassemble_function(script, "script", &mut out);
    out
}

fn disassemble_function(function: &CompiledFunction, label: &str, out: &mut String) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&function.chunk.disassemble(label));
    for nested in &function.chunk.functions {
        let label = match &nested.name {
            Some(name) => format!("fn {name}"),
            None => "fn <anonymous>".to_string(),
        };
        disassemble_function(nested, &label, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_match_their_byte() {
        // The decode table is positional, so a variant added to the enum without
        // being added here (or added in the wrong place) would silently decode
        // every later opcode as its neighbour. This catches that.
        for (index, op) in OPCODES.iter().enumerate() {
            assert_eq!(
                *op as usize,
                index,
                "{} sits at index {index} but its discriminant is {}",
                op.name(),
                *op as usize
            );
        }
        assert_eq!(OpCode::from_u8(OPCODES.len() as u8), None);
    }

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
            assert_eq!(OpCode::decode(op as u8), op);
        }
        assert_eq!(OpCode::from_u8(250), None);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn decoding_a_byte_that_is_not_an_opcode_panics() {
        // The interpreter's decode does not check, because only the compiler
        // writes chunks. Pin the behaviour so the trade is deliberate rather
        // than something a later change quietly turns into reading garbage.
        let _ = OpCode::decode(250);
    }

    #[test]
    fn add_constant_returns_increasing_indices() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.add_constant(Value::Int(1)), 0);
        assert_eq!(chunk.add_constant(Value::Int(2)), 1);
        assert_eq!(chunk.constants.len(), 2);
    }

    #[test]
    fn add_constant_reuses_an_entry_for_the_same_value() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.add_constant(Value::Int(7)), 0);
        assert_eq!(chunk.add_constant(Value::Str(Rc::new("hi".to_string()))), 1);
        assert_eq!(chunk.add_constant(Value::Int(7)), 0);
        // Strings match on their contents, not on which allocation they are.
        assert_eq!(chunk.add_constant(Value::Str(Rc::new("hi".to_string()))), 1);
        assert_eq!(chunk.constants.len(), 2);
    }

    #[test]
    fn add_constant_keeps_values_of_different_types_apart() {
        let mut chunk = Chunk::new();
        // 1 and 1.0 are equal to the language but are not the same constant:
        // sharing a slot would rewrite one literal as the other and change what
        // `type` and `str` report.
        assert_eq!(chunk.add_constant(Value::Int(1)), 0);
        assert_eq!(chunk.add_constant(Value::Float(1.0)), 1);
        // And 0.0 and -0.0, which compare equal but do not print the same.
        assert_eq!(chunk.add_constant(Value::Float(0.0)), 2);
        assert_eq!(chunk.add_constant(Value::Float(-0.0)), 3);
        assert_eq!(chunk.constants.len(), 4);
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
    fn get_field_is_disassembled_with_its_operand_byte() {
        // The disassembler's fallback prints an unrecognised opcode as though it
        // had no operands, so a new instruction that does have one gets its
        // operand byte read back as the next opcode. Nothing else in the suite
        // catches that: the VM's match is exhaustive and the OPCODES table has
        // its own test, but the disassembler has neither.
        let mut chunk = Chunk::new();
        let name = chunk.add_constant(Value::Str(std::rc::Rc::new("b".to_string()))) as u8;
        chunk.write_op(OpCode::Constant, 1, 1);
        chunk.write(name, 1, 1);
        chunk.write_op(OpCode::GetField, 1, 2);
        chunk.write(0, 1, 1);
        chunk.write_op(OpCode::Return, 1, 1);
        // RETURN at 0004 is the assertion that matters. GET_FIELD starts at
        // 0002 and its operand fills 0003, so landing at 0004 says the operand
        // was consumed. Had it been treated as a zero-operand instruction, the
        // byte at 0003 would have been decoded as one instead, and a zero there
        // is CONSTANT.
        assert_eq!(
            chunk.disassemble("test"),
            "== test ==\n\
             \x20  1  0000 CONSTANT      0 (\"b\")\n\
             \x20  |  0002 GET_FIELD\n\
             \x20  |  0004 RETURN\n"
        );
    }

    /// The same trap as `get_field_is_disassembled_with_its_operand_byte`,
    /// sprung by the operand `IterSnapshot` gained in 1.10. The byte is also
    /// what the two forms of the loop are told apart by, so the disassembler
    /// names which one it is rather than only stepping over it.
    #[test]
    fn iter_snapshot_is_disassembled_with_the_form_it_asks_for() {
        let mut chunk = Chunk::new();
        chunk.write_op(OpCode::IterSnapshot, 1, 1);
        chunk.write(0, 1, 1);
        chunk.write_op(OpCode::IterSnapshot, 1, 1);
        chunk.write(1, 1, 1);
        chunk.write_op(OpCode::Return, 1, 1);
        // RETURN at 0004 says both operand bytes were consumed. Read as
        // zero-operand instructions, the 0 at 0001 would have decoded as
        // CONSTANT and the 1 at 0003 as CONSTANT_LONG.
        assert_eq!(
            chunk.disassemble("test"),
            "== test ==\n\
             \x20  1  0000 ITER_SNAPSHOT elements\n\
             \x20  |  0002 ITER_SNAPSHOT pairs\n\
             \x20  |  0004 RETURN\n"
        );
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
        // The first instruction of a line shows the line number; the rest of
        // that line's instructions show a bar, so a statement reads as a group.
        assert_eq!(
            text,
            "== test ==\n\
             \x20  1  0000 CONSTANT      0 (1)\n\
             \x20  |  0002 CONSTANT      1 (2)\n\
             \x20  |  0004 ADD\n\
             \x20  |  0005 RETURN\n"
        );
    }
}
