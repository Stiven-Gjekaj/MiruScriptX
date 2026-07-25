//! The bytecode virtual machine: it executes a compiled [`Chunk`] on a value
//! stack.
//!
//! This is how MiruScriptX runs programs. Rather than walking the syntax tree,
//! it executes a flat stream of bytecode produced by [`crate::compiler`], which
//! avoids re-walking the tree and re-resolving names on every evaluation.
//!
//! Locals live in stack slots, a call pushes a frame that windows into the same
//! stack, and a closure captures variables as upvalues: shared cells that start
//! out pointing at a live slot and are closed into owned values when that slot
//! goes away. Operators and indexing go through [`crate::ops`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::chunk::{Chunk, OpCode};
use crate::globals::Globals;
use crate::value::{Closure, CompiledFunction, EmptyInput, Input, Output, Upvalue, Value};
use crate::MiruError;

/// How deep calls may nest before the VM gives up.
///
/// Frames live on the heap, so runaway recursion does not overflow the machine
/// stack the way a tree walker would; left alone it simply grows until memory
/// runs out, which is a worse failure than an error. This cap turns it into an
/// ordinary runtime error with a line, a column, and a caret. It is far beyond
/// what real code nests, so only genuinely unbounded recursion reaches it.
const MAX_CALL_DEPTH: usize = 10_000;

/// How deep calls made *from a builtin* may nest.
///
/// A user function called by a builtin (as `map` calls the function it is given)
/// runs on a nested bytecode loop, which is a real Rust call, so each level
/// costs machine stack that [`MAX_CALL_DEPTH`] does not account for. Recursion
/// that goes back through a builtin every time would exhaust that stack long
/// before ten thousand frames accumulate, so it needs its own, much lower cap.
///
/// The number is chosen for the *smallest* stack this may run on, not the
/// roomiest. A level costs roughly eleven kilobytes in a debug build, and a Rust
/// test thread gets two megabytes, where nesting fails somewhere past 180. Sixty
/// four leaves close to threefold margin there and far more in a release binary
/// on the main thread. Legitimate code does not nest higher-order builtins
/// anywhere near this deep.
const MAX_HOST_CALL_DEPTH: usize = 64;

/// A single active function call: which closure is running, where its
/// instruction pointer sits, and where its window of stack slots begins.
struct CallFrame {
    closure: Rc<Closure>,
    ip: usize,
    slot_base: usize,
}

/// How many bytes a `Call` instruction occupies: the opcode and its argument
/// count. [`CallFrame::call_site`] steps back over exactly this much.
const CALL_INSTRUCTION_LEN: usize = 2;

impl CallFrame {
    /// The source position of the call this frame is suspended at.
    ///
    /// `ip` is where the frame will *resume*, which the `Call` arm sets after
    /// reading the operand, so it points just past the call rather than at it.
    /// A trace wants the position a reader can find in their source, so step
    /// back over the instruction to reach the opcode byte, whose position entry
    /// is the call expression's.
    ///
    /// Returns `None` for a frame that is not suspended at a call: the script
    /// frame before it has executed anything, and any frame whose `ip` has not
    /// advanced past a call. Such a frame contributes no line to a trace rather
    /// than a wrong one.
    fn call_site(&self) -> Option<(usize, usize)> {
        let offset = self.ip.checked_sub(CALL_INSTRUCTION_LEN)?;
        let chunk = &self.closure.function.chunk;
        if OpCode::from_u8(*chunk.code.get(offset)?) != Some(OpCode::Call) {
            return None;
        }
        Some(chunk.position(offset))
    }
}

/// A stack-based bytecode interpreter.
pub struct Vm {
    stack: Vec<Value>,
    globals: Globals,
    frames: Vec<CallFrame>,
    /// Upvalues that still point at live stack slots, kept so several closures
    /// capturing the same slot share one upvalue, and so they can be closed when
    /// that slot leaves the stack.
    open_upvalues: Vec<(usize, Rc<RefCell<Upvalue>>)>,
    out: Box<dyn Write>,
    input: Box<dyn Input>,
    /// The position of the instruction being executed, so a builtin can report
    /// an error where the call to it happened.
    line: usize,
    column: usize,
    /// How many nested bytecode loops are running, one per call made from inside
    /// a builtin. Each costs machine stack, so it is capped separately.
    host_depth: usize,
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

impl Vm {
    /// Create a VM that writes to standard output.
    pub fn new() -> Vm {
        Vm::with_output(Box::new(std::io::stdout()))
    }

    /// Create a VM that writes to a custom sink, as the capture helpers do.
    pub fn with_output(out: Box<dyn Write>) -> Vm {
        let mut globals = Globals::new();
        crate::builtins::register(&mut globals);
        Vm {
            stack: Vec::new(),
            globals,
            frames: Vec::new(),
            open_upvalues: Vec::new(),
            out,
            input: Box::new(EmptyInput),
            line: 0,
            column: 0,
            host_depth: 0,
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

    /// Compile a program and run it, returning the value of its final
    /// expression.
    ///
    /// Compiling happens here, inside the VM, rather than in the caller, because
    /// the compiler and the VM share state that has to outlive a single program:
    /// a session compiles one input at a time, and the second input must resolve
    /// names the first defined.
    pub fn run(&mut self, program: &[crate::ast::Stmt]) -> Result<Value, MiruError> {
        let script = crate::compiler::Compiler::compile(program, &mut self.globals)?;
        self.interpret(script)
    }

    /// Execute an already compiled program. The whole program is itself a
    /// function (an anonymous script), so running it is just calling that
    /// function and reading back the value it returns.
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
            self.host_depth = 0;
        }
        debug_assert!(self.frames.is_empty(), "frames left after a program");
        debug_assert!(self.stack.is_empty(), "stack values left after a program");
        result
    }

    /// Run the bytecode loop until the frame at `base_depth` returns, yielding
    /// its result, attaching the call path to any error on the way out.
    ///
    /// This has to happen here rather than where the caller finally receives the
    /// error, because by then the frames are gone: [`Vm::interpret`] clears them
    /// and [`Vm::call_from_host`] truncates them, both before their error
    /// propagates. Here they are still intact.
    ///
    /// The `is_empty` guard is what makes a call made from inside a builtin come
    /// out right. Such a call runs on its own nested `run_frames`, which
    /// captures the full path while its frames are alive; this outer one then
    /// sees an error that already has a trace and leaves it alone rather than
    /// replacing it with the shallower view it can still see.
    fn run_frames(&mut self, base_depth: usize) -> Result<Value, MiruError> {
        let mut result = self.run_frames_inner(base_depth);
        if let Err(error) = &mut result {
            if error.trace.is_empty() {
                error.trace = self.capture_trace();
            }
        }
        result
    }

    /// The call path currently on the frame stack, innermost first.
    ///
    /// Each entry names a function that was entered and the line the call to it
    /// was written on, so it reads as "in add, called from line 6". That pairs
    /// each frame's name with its *caller's* call site, not its own: a frame
    /// knows where it is suspended, which is where it calls onward, so the line
    /// a function was reached by lives one frame out.
    ///
    /// The outermost frame is the script, which nothing called, so the walk
    /// stops before it.
    fn capture_trace(&self) -> Vec<crate::TraceEntry> {
        (1..self.frames.len())
            .rev()
            .filter_map(|index| {
                let (line, _) = self.frames[index - 1].call_site()?;
                Some(crate::TraceEntry {
                    function: self.frames[index].closure.function.name.clone(),
                    line,
                })
            })
            .collect()
    }

    /// The bytecode loop itself. See [`Vm::run_frames`], which wraps it.
    fn run_frames_inner(&mut self, base_depth: usize) -> Result<Value, MiruError> {
        // Two loops, one per frame and one per instruction. The outer loop keeps
        // the current frame's closure, instruction pointer, stack base, and chunk
        // in locals; the inner one runs until a call or return changes frames and
        // jumps back out to pick up the new ones. `closure` is a cloned handle, so
        // `chunk` borrows it rather than `self`, leaving `self` free to mutate as
        // instructions execute.
        //
        // The split is what makes `chunk` a local. Reaching it means following an
        // Rc to the closure and another to its function, and the inner loop would
        // otherwise pay for both on every instruction to arrive at something that
        // only changes when the frame does.
        'frames: loop {
            let frame = self.frames.last().expect("a call frame");
            let closure = Rc::clone(&frame.closure);
            let mut ip = frame.ip;
            let slot_base = frame.slot_base;
            let chunk = &closure.function.chunk;
            loop {
                let op_ip = ip;
                let op = OpCode::decode(chunk.code[ip]);
                ip += 1;
                match op {
                    OpCode::Constant => {
                        let index = chunk.code[ip] as usize;
                        ip += 1;
                        self.stack.push(chunk.constants[index].clone());
                    }
                    OpCode::ConstantLong => {
                        let index = read_u16(chunk, ip) as usize;
                        ip += 2;
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
                    OpCode::BinaryConst => {
                        let operator = binary_op(OpCode::decode(chunk.code[ip]));
                        let index = chunk.code[ip + 1] as usize;
                        ip += 2;
                        self.binary_const(operator, index, chunk, op_ip)?;
                    }
                    OpCode::DefineGlobal => {
                        let slot = read_u16(chunk, ip);
                        ip += 2;
                        let value = self.pop();
                        self.globals.define(slot, value);
                    }
                    OpCode::GetGlobal => {
                        let slot = read_u16(chunk, ip);
                        ip += 2;
                        match self.globals.get(slot) {
                            Some(value) => self.stack.push(value.clone()),
                            None => {
                                let name = self.globals.name(slot);
                                return Err(runtime_error(
                                    chunk,
                                    op_ip,
                                    format!("undefined variable '{name}'"),
                                ));
                            }
                        }
                    }
                    OpCode::SetGlobal => {
                        let slot = read_u16(chunk, ip);
                        ip += 2;
                        let value = self.pop();
                        if !self.globals.assign(slot, value) {
                            let name = self.globals.name(slot);
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
                        let count = read_u16(chunk, ip) as usize;
                        ip += 2;
                        let start = self.stack.len() - count;
                        let items = self.stack.split_off(start);
                        self.stack.push(Value::Array(Rc::new(RefCell::new(items))));
                    }
                    OpCode::Map => {
                        let count = read_u16(chunk, ip) as usize;
                        ip += 2;
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
                        let index = read_u16(chunk, ip) as usize;
                        ip += 2;
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
                            continue 'frames;
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
                        continue 'frames;
                    }
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
                if self.frames.len() >= MAX_CALL_DEPTH {
                    return Err(runtime_error(
                        chunk,
                        op_ip,
                        format!("call depth limit of {MAX_CALL_DEPTH} exceeded"),
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

    /// Apply a function value to arguments from outside the bytecode loop. This
    /// is what a higher-order builtin such as `map` calls.
    pub fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, MiruError> {
        self.call_from_host(callee, args)
    }

    /// Build an error at the position of the call being executed, for a builtin
    /// to report against.
    pub fn call_error(&self, message: String) -> MiruError {
        MiruError::with_column(self.line, self.column, message)
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
                if self.frames.len() >= MAX_CALL_DEPTH {
                    return Err(
                        self.call_error(format!("call depth limit of {MAX_CALL_DEPTH} exceeded"))
                    );
                }
                if self.host_depth >= MAX_HOST_CALL_DEPTH {
                    return Err(self.call_error(format!(
                        "call depth limit of {MAX_HOST_CALL_DEPTH} exceeded through a builtin"
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
                self.host_depth += 1;
                let result = self.run_frames(depth);
                self.host_depth -= 1;
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

    #[inline]
    fn binary(&mut self, op: BinaryOp, chunk: &Chunk, offset: usize) -> Result<(), MiruError> {
        let right = self.pop();
        let left = self.pop();
        // Two integers is overwhelmingly the common case in a loop, so handle it
        // here rather than through the general dispatch. Anything the fast path
        // declines, including an overflow, falls through to the shared rules so
        // there is exactly one definition of what each operator means.
        if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
            if let Some(result) = int_binary(op, *a, *b) {
                self.stack.push(result);
                return Ok(());
            }
        }
        let result =
            crate::ops::binary(op, left, right).map_err(|m| runtime_error(chunk, offset, m))?;
        self.stack.push(result);
        Ok(())
    }

    /// Apply a binary operator whose right operand is `constants[index]`.
    ///
    /// The same work as [`Vm::binary`] against a constant that was never pushed.
    /// Reading it out of the pool rather than off the stack is what the fused
    /// instruction buys, and it lets the integer fast path see the constant
    /// without cloning it first.
    #[inline]
    fn binary_const(
        &mut self,
        op: BinaryOp,
        index: usize,
        chunk: &Chunk,
        offset: usize,
    ) -> Result<(), MiruError> {
        let left = self.pop();
        if let (Value::Int(a), Value::Int(b)) = (&left, &chunk.constants[index]) {
            if let Some(result) = int_binary(op, *a, *b) {
                self.stack.push(result);
                return Ok(());
            }
        }
        let right = chunk.constants[index].clone();
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

/// Apply a binary operator to two integers, or decline.
///
/// This exists only to skip the general operator dispatch on the case a loop
/// spends its time in. It declines rather than guesses: overflow, division and
/// modulo by zero, and anything not covered here all return `None`, and the
/// caller falls back to [`crate::ops::binary`], which stays the single
/// definition of what every operator means and which error it raises.
#[inline]
fn int_binary(op: BinaryOp, a: i64, b: i64) -> Option<Value> {
    let value = match op {
        BinaryOp::Add => Value::Int(a.checked_add(b)?),
        BinaryOp::Subtract => Value::Int(a.checked_sub(b)?),
        BinaryOp::Multiply => Value::Int(a.checked_mul(b)?),
        // Zero divisors are an error, not a result, so leave them to the
        // general path rather than reproducing the message here.
        BinaryOp::Divide if b != 0 => Value::Int(a.checked_div(b)?),
        BinaryOp::Modulo if b != 0 => Value::Int(a.checked_rem(b)?),
        BinaryOp::Divide | BinaryOp::Modulo => return None,
        BinaryOp::Equal => Value::Bool(a == b),
        BinaryOp::NotEqual => Value::Bool(a != b),
        BinaryOp::Less => Value::Bool(a < b),
        BinaryOp::Greater => Value::Bool(a > b),
        BinaryOp::LessEqual => Value::Bool(a <= b),
        BinaryOp::GreaterEqual => Value::Bool(a >= b),
    };
    Some(value)
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
    fn a_frame_reports_the_call_it_is_suspended_at() {
        // Hand-assembled so the layout is known exactly: a wrong offset cannot
        // pass by landing on the right position by accident.
        let mut chunk = Chunk::new();
        chunk.write_op(OpCode::Nil, 1, 1);
        chunk.write_op(OpCode::Nil, 1, 1);
        let call = chunk.code.len();
        chunk.write_op(OpCode::Call, 7, 3);
        chunk.write(0, 7, 3);
        chunk.write_op(OpCode::Return, 8, 1);

        let function = Rc::new(CompiledFunction {
            name: Some("outer".to_string()),
            arity: 0,
            chunk,
        });
        let closure = Rc::new(Closure {
            function,
            upvalues: Vec::new(),
        });

        // Suspended at the call. `ip` points past it, at where the frame will
        // resume, and the reported position is still the call's own.
        let frame = CallFrame {
            closure: Rc::clone(&closure),
            ip: call + CALL_INSTRUCTION_LEN,
            slot_base: 0,
        };
        assert_eq!(frame.call_site(), Some((7, 3)));

        // Not suspended at a call: report nothing rather than a wrong line.
        let fresh = CallFrame {
            closure,
            ip: 0,
            slot_base: 0,
        };
        assert_eq!(fresh.call_site(), None);
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
