//! The bytecode virtual machine: it executes a compiled [`Chunk`] on a value
//! stack.
//!
//! The VM is the second of MiruScriptX's two execution engines. It computes with
//! the same [`Value`]s as the tree walker and applies operators through the same
//! [`crate::ops`] functions, so the two engines agree on results and errors. What
//! differs is how it gets there: instead of walking the AST, it runs a flat
//! stream of bytecode, which avoids the pointer chasing and repeated name
//! lookups that make a tree walker slow.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::value::{Caller, Closure, CompiledFunction, EmptyInput, Input, Output, Upvalue, Value};
use crate::MiruError;

/// A single active function call: which closure is running, where its
/// instruction pointer sits, and where its window of stack slots begins.
struct CallFrame {
    closure: Rc<Closure>,
    ip: usize,
    slot_base: usize,
}

/// A stack-based bytecode interpreter.
pub struct Vm {
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
    frames: Vec<CallFrame>,
    /// Upvalues that still point at live stack slots, kept so several closures
    /// capturing the same slot share one upvalue, and so they can be closed when
    /// that slot leaves the stack.
    open_upvalues: Vec<(usize, Rc<RefCell<Upvalue>>)>,
    out: Box<dyn Write>,
    input: Box<dyn Input>,
    /// The position of the instruction being executed, so builtins called
    /// through the [`Caller`] trait can report errors where they happened.
    line: usize,
    column: usize,
}

impl Default for Vm {
    fn default() -> Self {
        Vm::new()
    }
}

impl Output for Vm {
    fn write(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
    }
}

impl Caller for Vm {
    fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, MiruError> {
        self.call_from_host(callee, args)
    }

    fn call_error(&self, message: String) -> MiruError {
        MiruError::with_column(self.line, self.column, message)
    }
}

impl Vm {
    /// Create a VM that writes to standard output.
    pub fn new() -> Vm {
        Vm::with_output(Box::new(std::io::stdout()))
    }

    /// Create a VM that writes to a custom sink, as the capture helpers do.
    pub fn with_output(out: Box<dyn Write>) -> Vm {
        let mut globals = HashMap::new();
        crate::builtins::register_map(&mut globals);
        Vm {
            stack: Vec::new(),
            globals,
            frames: Vec::new(),
            open_upvalues: Vec::new(),
            out,
            input: Box::new(EmptyInput),
            line: 0,
            column: 0,
        }
    }

    /// Replace the input source that `input()` reads from.
    pub fn set_input(&mut self, input: Box<dyn Input>) {
        self.input = input;
    }

    /// Flush any buffered output.
    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }

    /// Execute a compiled program. The whole program is itself a function (an
    /// anonymous script), so running it is just calling that function and reading
    /// back the value it returns.
    ///
    /// Globals persist across calls, which is what lets a session such as the
    /// REPL build up state over many inputs. Everything else is transient: this
    /// returns with the value stack, the frame stack, and the open upvalues
    /// empty, whether the program succeeded or failed. Without that, a program
    /// that fails part way would leave a half-finished frame behind and the next
    /// one would resume into it.
    pub fn interpret(&mut self, script: Rc<CompiledFunction>) -> Result<Value, MiruError> {
        let closure = Rc::new(Closure {
            function: script,
            upvalues: Vec::new(),
        });
        self.frames.push(CallFrame {
            closure,
            ip: 0,
            slot_base: 0,
        });
        let result = self.run_frames(0);
        if result.is_err() {
            self.frames.clear();
            self.stack.clear();
            self.open_upvalues.clear();
        }
        debug_assert!(self.frames.is_empty(), "frames left after a program");
        debug_assert!(self.stack.is_empty(), "stack values left after a program");
        result
    }

    /// Run the bytecode loop until the frame at `base_depth` returns, yielding its
    /// result. `base_depth` is 0 for a whole program; a higher value runs a single
    /// nested call made from a host builtin.
    fn run_frames(&mut self, base_depth: usize) -> Result<Value, MiruError> {
        // Keep the current frame's closure, instruction pointer, and stack base in
        // locals. `closure` is a cloned handle, so `chunk` borrows it rather than
        // `self`, leaving `self` free to mutate as instructions execute. The three
        // are resynced from the frame stack whenever a call or return changes it.
        let mut closure = Rc::clone(&self.frames.last().expect("a call frame").closure);
        let mut ip = self.frames.last().expect("a call frame").ip;
        let mut slot_base = self.frames.last().expect("a call frame").slot_base;
        loop {
            let chunk = &closure.function.chunk;
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
                OpCode::Loop => {
                    let offset = read_u16(chunk, ip);
                    ip = ip + 2 - offset as usize;
                }
                OpCode::Truthy => {
                    let value = self.pop();
                    self.stack.push(Value::Bool(value.is_truthy()));
                }
                OpCode::GetLocal => {
                    let slot = chunk.code[ip] as usize;
                    ip += 1;
                    let value = self.stack[slot_base + slot].clone();
                    self.stack.push(value);
                }
                OpCode::SetLocal => {
                    let slot = chunk.code[ip] as usize;
                    ip += 1;
                    let value = self.pop();
                    self.stack[slot_base + slot] = value;
                }
                OpCode::Array => {
                    let count = chunk.code[ip] as usize;
                    ip += 1;
                    let start = self.stack.len() - count;
                    let items = self.stack.split_off(start);
                    self.stack.push(Value::Array(Rc::new(RefCell::new(items))));
                }
                OpCode::Map => {
                    let count = chunk.code[ip] as usize;
                    ip += 1;
                    let start = self.stack.len() - count * 2;
                    let pairs = self.stack.split_off(start);
                    let mut entries = BTreeMap::new();
                    let mut pairs = pairs.into_iter();
                    while let Some(key) = pairs.next() {
                        let value = pairs.next().expect("a value for every map key");
                        let key = crate::ops::map_key(&key)
                            .map_err(|message| runtime_error(chunk, op_ip, message))?;
                        entries.insert(key, value);
                    }
                    self.stack.push(Value::Map(Rc::new(RefCell::new(entries))));
                }
                OpCode::Index => {
                    // The operand byte holds no value; its position entry is the
                    // target expression's, used when the target is not indexable.
                    let target_ip = ip;
                    ip += 1;
                    let index = self.pop();
                    let target = self.pop();
                    let element = index_get(target, &index, chunk, op_ip, target_ip)?;
                    self.stack.push(element);
                }
                OpCode::SetIndex => {
                    let target_ip = ip;
                    ip += 1;
                    let index = self.pop();
                    let target = self.pop();
                    let value = self.pop();
                    index_set(target, &index, value, chunk, op_ip, target_ip)?;
                }
                OpCode::IterSnapshot => {
                    let value = self.pop();
                    match value {
                        Value::Array(items) => {
                            let snapshot = items.borrow().clone();
                            self.stack
                                .push(Value::Array(Rc::new(RefCell::new(snapshot))));
                        }
                        other => {
                            return Err(runtime_error(
                                chunk,
                                op_ip,
                                format!("cannot iterate over a {}", other.type_name()),
                            ))
                        }
                    }
                }
                OpCode::ForNext => {
                    let seq_slot = slot_base + chunk.code[ip] as usize;
                    let jump = read_u16(chunk, ip + 1);
                    ip += 3;
                    let index = match &self.stack[seq_slot + 1] {
                        Value::Int(n) => *n,
                        _ => unreachable!("for-in index is not an integer"),
                    };
                    let length = match &self.stack[seq_slot] {
                        Value::Array(items) => items.borrow().len() as i64,
                        _ => unreachable!("for-in sequence is not an array"),
                    };
                    if index >= length {
                        ip += jump as usize;
                    } else {
                        let element = match &self.stack[seq_slot] {
                            Value::Array(items) => items.borrow()[index as usize].clone(),
                            _ => unreachable!("for-in sequence is not an array"),
                        };
                        self.stack.push(element);
                        self.stack[seq_slot + 1] = Value::Int(index + 1);
                    }
                }
                OpCode::Closure => {
                    let index = chunk.code[ip] as usize;
                    ip += 1;
                    let function = Rc::clone(&chunk.functions[index]);
                    let upvalue_count = chunk.code[ip] as usize;
                    ip += 1;
                    let mut upvalues = Vec::with_capacity(upvalue_count);
                    for _ in 0..upvalue_count {
                        let is_local = chunk.code[ip] != 0;
                        let operand = chunk.code[ip + 1] as usize;
                        ip += 2;
                        let upvalue = if is_local {
                            self.capture_upvalue(slot_base + operand)
                        } else {
                            Rc::clone(&closure.upvalues[operand])
                        };
                        upvalues.push(upvalue);
                    }
                    self.stack
                        .push(Value::Closure(Rc::new(Closure { function, upvalues })));
                }
                OpCode::GetUpvalue => {
                    let index = chunk.code[ip] as usize;
                    ip += 1;
                    let value = match &*closure.upvalues[index].borrow() {
                        Upvalue::Open(slot) => self.stack[*slot].clone(),
                        Upvalue::Closed(value) => value.clone(),
                    };
                    self.stack.push(value);
                }
                OpCode::SetUpvalue => {
                    let index = chunk.code[ip] as usize;
                    ip += 1;
                    let value = self.pop();
                    let upvalue = Rc::clone(&closure.upvalues[index]);
                    match &mut *upvalue.borrow_mut() {
                        Upvalue::Open(slot) => self.stack[*slot] = value,
                        Upvalue::Closed(cell) => *cell = value,
                    };
                }
                OpCode::CloseUpvalue => {
                    self.close_upvalues_from(self.stack.len() - 1);
                    self.pop();
                }
                OpCode::Call => {
                    let argcount = chunk.code[ip] as usize;
                    ip += 1;
                    // Save where to resume, then enter the callee. A builtin runs
                    // in place and leaves the frame stack untouched.
                    self.frames.last_mut().expect("a call frame").ip = ip;
                    if self.call_at_stack(argcount, chunk, op_ip)? {
                        let frame = self.frames.last().expect("a call frame");
                        closure = Rc::clone(&frame.closure);
                        ip = frame.ip;
                        slot_base = frame.slot_base;
                    }
                    continue;
                }
                OpCode::Pop => {
                    self.pop();
                }
                OpCode::Return => {
                    let result = self.pop();
                    let frame = self.frames.pop().expect("a call frame");
                    // Close any upvalues that captured this frame's locals before
                    // they leave the stack.
                    self.close_upvalues_from(frame.slot_base);
                    if self.frames.len() == base_depth {
                        // Drop the callee and its arguments; the result goes back
                        // to whoever started this loop.
                        self.stack.truncate(frame.slot_base.saturating_sub(1));
                        return Ok(result);
                    }
                    // Drop the callee and its arguments and locals, then leave the
                    // return value where the call expression's result belongs.
                    self.stack.truncate(frame.slot_base - 1);
                    self.stack.push(result);
                    let caller = self.frames.last().expect("a call frame");
                    closure = Rc::clone(&caller.closure);
                    ip = caller.ip;
                    slot_base = caller.slot_base;
                    continue;
                }
            }
        }
    }

    /// Enter a function call: the callee sits just beneath its `argcount`
    /// arguments on the stack. A user function pushes a new frame; a native
    /// builtin runs immediately and leaves its result in place. Returns whether a
    /// frame was pushed, so the run loop knows to switch frames.
    fn call_at_stack(
        &mut self,
        argcount: usize,
        chunk: &Chunk,
        op_ip: usize,
    ) -> Result<bool, MiruError> {
        let callee_slot = self.stack.len() - argcount - 1;
        let callee = self.stack[callee_slot].clone();
        match callee {
            Value::Closure(closure) => {
                let arity = closure.function.arity;
                if argcount != arity {
                    let name = closure.function.name.as_deref().unwrap_or("<anonymous>");
                    return Err(runtime_error(
                        chunk,
                        op_ip,
                        format!(
                            "function {name} expects {arity} argument(s) but received {argcount}"
                        ),
                    ));
                }
                let slot_base = self.stack.len() - argcount;
                self.frames.push(CallFrame {
                    closure,
                    ip: 0,
                    slot_base,
                });
                Ok(true)
            }
            // Builtins run to completion here, so no frame is pushed: the callee
            // and its arguments are replaced by the single result value.
            Value::Builtin(_) | Value::HostBuiltin(_) => {
                let args = self.stack.split_off(callee_slot + 1);
                self.stack.pop();
                let (line, column) = chunk.position(op_ip);
                let result = self.call_native(callee, args, line, column)?;
                self.stack.push(result);
                Ok(false)
            }
            other => Err(runtime_error(
                chunk,
                op_ip,
                format!("a {} is not callable", other.type_name()),
            )),
        }
    }

    /// Run a native builtin, with `line` and `column` as the position its errors
    /// report. Shared by bytecode calls and calls made from host builtins.
    fn call_native(
        &mut self,
        callee: Value,
        args: Vec<Value>,
        line: usize,
        column: usize,
    ) -> Result<Value, MiruError> {
        let (saved_line, saved_column) = (self.line, self.column);
        self.line = line;
        self.column = column;
        let result = match callee {
            Value::Builtin(builtin) => {
                // Move the input reader out so the VM can also be borrowed as the
                // output sink during the call, then restore it.
                let mut input = std::mem::replace(&mut self.input, Box::new(EmptyInput));
                let result = (builtin.func)(self, &mut *input, args);
                self.input = input;
                result.map_err(|message| MiruError::with_column(line, column, message))
            }
            Value::HostBuiltin(builtin) => (builtin.func)(self, args),
            other => Err(MiruError::with_column(
                line,
                column,
                format!("a {} is not callable", other.type_name()),
            )),
        };
        self.line = saved_line;
        self.column = saved_column;
        result
    }

    /// Apply a function value from outside the bytecode loop, as a higher-order
    /// builtin does. A user function runs on a nested loop that stops when its
    /// frame returns, so the result comes back here rather than to the
    /// interrupted frame.
    fn call_from_host(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, MiruError> {
        match callee {
            Value::Closure(closure) => {
                let arity = closure.function.arity;
                if args.len() != arity {
                    let name = closure.function.name.as_deref().unwrap_or("<anonymous>");
                    return Err(self.call_error(format!(
                        "function {name} expects {arity} argument(s) but received {}",
                        args.len()
                    )));
                }
                // Mirror the bytecode layout: the callee sits beneath its args.
                let callee_slot = self.stack.len();
                self.stack.push(Value::Closure(Rc::clone(&closure)));
                let slot_base = self.stack.len();
                for arg in args {
                    self.stack.push(arg);
                }
                let depth = self.frames.len();
                self.frames.push(CallFrame {
                    closure,
                    ip: 0,
                    slot_base,
                });
                let result = self.run_frames(depth);
                if result.is_err() {
                    // Unwind the partial call so the stack stays consistent.
                    self.frames.truncate(depth);
                    self.stack.truncate(callee_slot);
                }
                result
            }
            other => {
                let (line, column) = (self.line, self.column);
                self.call_native(other, args, line, column)
            }
        }
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

    /// Return the open upvalue over `slot`, creating one if none exists yet, so
    /// that every closure capturing the same slot shares a single upvalue.
    fn capture_upvalue(&mut self, slot: usize) -> Rc<RefCell<Upvalue>> {
        if let Some((_, upvalue)) = self.open_upvalues.iter().find(|(s, _)| *s == slot) {
            return Rc::clone(upvalue);
        }
        let upvalue = Rc::new(RefCell::new(Upvalue::Open(slot)));
        self.open_upvalues.push((slot, Rc::clone(&upvalue)));
        upvalue
    }

    /// Close every open upvalue at or above `from_slot`, moving each captured
    /// value off the stack and into the upvalue so it outlives the frame.
    fn close_upvalues_from(&mut self, from_slot: usize) {
        let mut index = 0;
        while index < self.open_upvalues.len() {
            let slot = self.open_upvalues[index].0;
            if slot >= from_slot {
                let (_, upvalue) = self.open_upvalues.remove(index);
                let value = self.stack[slot].clone();
                *upvalue.borrow_mut() = Upvalue::Closed(value);
            } else {
                index += 1;
            }
        }
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

/// Read `target[index]` for an array or map, or fail for anything else.
///
/// A bad index or key is the index expression's fault and is reported at
/// `index_ip`; an unindexable target is the target's fault and is reported at
/// `target_ip`, so the caret lands under the part actually at fault.
fn index_get(
    target: Value,
    index: &Value,
    chunk: &Chunk,
    index_ip: usize,
    target_ip: usize,
) -> Result<Value, MiruError> {
    match target {
        Value::Array(items) => {
            let len = items.borrow().len();
            let idx = crate::ops::array_index(index, len)
                .map_err(|message| runtime_error(chunk, index_ip, message))?;
            let element = items.borrow()[idx].clone();
            Ok(element)
        }
        Value::Map(entries) => {
            let key = crate::ops::map_key(index)
                .map_err(|message| runtime_error(chunk, index_ip, message))?;
            Ok(entries.borrow().get(&key).cloned().unwrap_or(Value::Nil))
        }
        other => Err(runtime_error(
            chunk,
            target_ip,
            format!("cannot index a {}", other.type_name()),
        )),
    }
}

/// Assign `target[index] = value` for an array or map, or fail for anything
/// else, attributing each error as [`index_get`] does.
fn index_set(
    target: Value,
    index: &Value,
    value: Value,
    chunk: &Chunk,
    index_ip: usize,
    target_ip: usize,
) -> Result<(), MiruError> {
    match target {
        Value::Array(items) => {
            let len = items.borrow().len();
            let idx = crate::ops::array_index(index, len)
                .map_err(|message| runtime_error(chunk, index_ip, message))?;
            items.borrow_mut()[idx] = value;
            Ok(())
        }
        Value::Map(entries) => {
            let key = crate::ops::map_key(index)
                .map_err(|message| runtime_error(chunk, index_ip, message))?;
            entries.borrow_mut().insert(key, value);
            Ok(())
        }
        other => Err(runtime_error(
            chunk,
            target_ip,
            format!("cannot index-assign to a {}", other.type_name()),
        )),
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
        let script = Rc::new(CompiledFunction {
            name: None,
            arity: 0,
            chunk,
        });
        Vm::new().interpret(script)
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
