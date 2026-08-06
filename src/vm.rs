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
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins::{Args, HostTask, Step};
use crate::chunk::{Chunk, OpCode};
use crate::globals::{Globals, ModuleId, ROOT_MODULE};
use crate::random::Rng;
use crate::suggest::with_suggestion;
use crate::value::{
    Ambient, Clock, Closure, CompiledFunction, EmptyInput, Input, Keyboard, NativeFn, NoClock,
    NoKeyboard, NoSystem, Output, System, Upvalue, Value,
};
use crate::MiruError;

/// How deep calls may nest before the VM gives up.
///
/// Frames live on the heap, so runaway recursion does not overflow the machine
/// stack the way a tree walker would; left alone it simply grows until memory
/// runs out, which is a worse failure than an error. This cap turns it into an
/// ordinary runtime error with a line, a column, and a caret. It is far beyond
/// what real code nests, so only genuinely unbounded recursion reaches it.
const MAX_CALL_DEPTH: usize = 10_000;

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

/// What entering a call did, so the dispatch loop knows how to carry on.
///
/// This says more than the boolean it replaces because a third outcome is
/// coming: a higher-order builtin suspends rather than finishing, and the loop
/// has to tell that apart from a builtin that ran to completion.
enum CallOutcome {
    /// A frame was pushed. The loop must pick up the new one.
    Frame,
    /// A native builtin ran to completion and left its result on the stack.
    /// The loop carries on with the current frame.
    Value,
    /// A higher-order builtin suspended as a task. The loop must drive it.
    Task,
}

/// Where a task's finished value belongs.
enum TaskSink {
    /// On the value stack, where the call expression's result goes. This is the
    /// ordinary case: bytecode called `map`.
    Stack,
    /// To the task beneath this one, which asked for this call. Reached when a
    /// higher-order builtin is another one's callback, as in
    /// `reduce([abs, abs], map, ..)`, where `reduce` passes two arguments and
    /// two is exactly `map`'s arity.
    Parent,
}

/// A higher-order builtin suspended part way through, waiting for a call it
/// asked for to come back.
struct PendingTask {
    task: HostTask,
    /// The frame depth at which a returning frame belongs to this task. A task
    /// is only ever created while a frame is running, so this is at least one,
    /// which is what lets a `resume_depth` of zero mean "no task pending".
    resume_at: usize,
    sink: TaskSink,
    /// Where the call that started this task was written, so an error it raises
    /// points at the `map(..)` rather than at whatever ran last.
    line: usize,
    column: usize,
}

/// What starting a call on a task's behalf did.
enum TaskCall {
    /// A frame was pushed and is running it.
    Pushed,
    /// An ordinary native builtin ran in place; here is its result.
    Value(Value),
    /// The callee was itself a higher-order builtin, so a nested task was
    /// pushed and wants driving.
    Nested,
}

/// A `try` waiting to see whether the expression under it fails.
///
/// Everything here is a mark to rewind to. An error can be raised many frames
/// deeper than the `try` that catches it, so the VM has to be able to put the
/// four things that grew back the way they were: the frame stack, the value
/// stack, the pending higher-order builtins, and the upvalues still pointing at
/// stack slots that are about to go away.
struct Handler {
    frames: usize,
    stack: usize,
    tasks: usize,
    /// Where the frame that installed this resumes: the landing, past the
    /// guarded expression.
    ip: usize,
}

/// What `import` has loaded, and what it is part way through loading.
#[derive(Default)]
struct Loader {
    /// Canonical path to the module's exported map, so a file imported from two
    /// places runs once. Without this a module with a side effect runs twice and
    /// nobody can explain why.
    cache: HashMap<PathBuf, Value>,
    /// The chain currently being loaded, innermost last, as canonical paths
    /// paired with the spec each was reached by. Turns a cycle into an ordinary
    /// error naming the chain rather than a stack overflow.
    loading: Vec<(PathBuf, String)>,
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
    /// Where diagnostics go. Standard error in the binary, a buffer in tests.
    ///
    /// A separate sink rather than a flag on `out`, because the point of the
    /// stream is that a caller can redirect one and not the other. A test that
    /// cannot tell them apart is not testing anything.
    err: Box<dyn Write>,
    input: Box<dyn Input>,
    /// The host's file system and command line, if it has one. Absent unless
    /// the embedder supplies it, so nothing gains file access by accident.
    system: Box<dyn System>,
    /// The host's wall clock, on the same terms as `system`: absent until an
    /// embedder supplies one, so no program reads a time the host never gave.
    clock: Box<dyn Clock>,
    /// The host's keyboard, on the same terms again: absent until an embedder
    /// supplies one, so no program reads a key the host never had.
    keyboard: Box<dyn Keyboard>,
    /// The random number generator, or `None` until a program asks for a
    /// number or sets a seed.
    ///
    /// Not a capability of the host, unlike the two above it. The algorithm
    /// belongs to the language, so that a seed gives the same numbers wherever
    /// the program runs. What it starts from is a host question, which is why
    /// it is seeded lazily, from the clock, by `Ambient::rng`.
    rng: Option<Rng>,
    /// The code a program asked to stop with, once it has asked.
    ///
    /// `None` until something calls [`Output::request_exit`]. Whoever runs the
    /// program reads this **before** reporting the error that unwound it,
    /// because a program that ended on purpose has not failed and must not be
    /// reported as though it had.
    exit: Option<i32>,
    /// The position of the instruction being executed, so a builtin can report
    /// an error where the call to it happened.
    line: usize,
    column: usize,
    /// Higher-order builtins suspended waiting for a call, innermost last.
    tasks: Vec<PendingTask>,
    /// `try` expressions currently being evaluated, innermost last.
    handlers: Vec<Handler>,
    loader: Loader,
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

    fn write_error(&mut self, text: &str) {
        let _ = self.err.write_all(text.as_bytes());
    }

    fn request_exit(&mut self, code: i32) {
        self.exit = Some(code);
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
            err: Box::new(std::io::stderr()),
            input: Box::new(EmptyInput),
            system: Box::new(NoSystem),
            clock: Box::new(NoClock),
            keyboard: Box::new(NoKeyboard),
            rng: None,
            exit: None,
            line: 0,
            column: 0,
            tasks: Vec::new(),
            handlers: Vec::new(),
            loader: Loader::default(),
        }
    }

    /// Replace the input source that `input()` reads from.
    pub fn set_input(&mut self, input: Box<dyn Input>) {
        self.input = input;
    }

    /// Replace the sink that diagnostics go to. Standard error by default.
    ///
    /// Separate from [`Vm::with_output`] because most callers want the default
    /// and only a test that reads back what a program complained about needs to
    /// change it.
    pub fn set_error_output(&mut self, err: Box<dyn Write>) {
        self.err = err;
    }

    /// The code the program asked to stop with, or `None` if it never asked.
    ///
    /// Read this before reporting the error that ended a run. An exit unwinds
    /// by raising one, so a program that stopped on purpose looks exactly like
    /// a program that failed until this is consulted.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit
    }

    /// Give this VM a file system and a command line.
    ///
    /// Nothing calls this except `miru` itself. A VM that is never given one
    /// refuses every file operation, which is what the playground and any
    /// embedder gets until it decides otherwise.
    pub fn set_system(&mut self, system: Box<dyn System>) {
        self.system = system;
    }

    /// Give this VM a clock.
    ///
    /// On the same terms as [`Vm::set_system`]: a VM that is never given one
    /// refuses `now`, which is what an embedder gets until it decides
    /// otherwise. Unlike a file system, this is a capability the browser
    /// playground does supply, since a page has `Date.now`.
    pub fn set_clock(&mut self, clock: Box<dyn Clock>) {
        self.clock = clock;
    }

    /// Give this VM a keyboard, which `read_key` reads.
    ///
    /// On the same terms as [`Vm::set_clock`]. Unlike a clock, this one the
    /// browser playground does not supply: a page has no keyboard buffer to
    /// take a key from.
    pub fn set_keyboard(&mut self, keyboard: Box<dyn Keyboard>) {
        self.keyboard = keyboard;
    }

    /// Flush any buffered output.
    pub fn flush(&mut self) {
        let _ = self.out.flush();
        let _ = self.err.flush();
    }

    /// Compile a program and run it, returning the value of its final
    /// expression.
    ///
    /// Compiling happens here, inside the VM, rather than in the caller, because
    /// the compiler and the VM share state that has to outlive a single program:
    /// a session compiles one input at a time, and the second input must resolve
    /// names the first defined.
    pub fn run(&mut self, program: &[crate::ast::Stmt]) -> Result<Value, MiruError> {
        self.run_from(program, None)
    }

    /// Run a program that came from `path`, which is what an `import` resolves
    /// against.
    ///
    /// Imports are settled here, before the program compiles, so an alias
    /// already holds its module by the time a `GetField` on it runs. `None`
    /// means the program came from a string rather than a file, and an import
    /// then has nothing to resolve against.
    pub fn run_from(
        &mut self,
        program: &[crate::ast::Stmt],
        path: Option<&Path>,
    ) -> Result<Value, MiruError> {
        self.resolve_imports(program, path, ROOT_MODULE)?;
        let script = crate::compiler::Compiler::compile(program, &mut self.globals)?;
        self.interpret(script)
    }

    /// Bind every top-level import in `program`.
    ///
    /// The error for a program with no file lives here rather than in the
    /// compiler, because this is where a path would have been. The compiler
    /// knows nothing about files.
    fn resolve_imports(
        &mut self,
        program: &[crate::ast::Stmt],
        path: Option<&Path>,
        module: ModuleId,
    ) -> Result<(), MiruError> {
        for stmt in program {
            let crate::ast::StmtKind::Import {
                spec,
                alias,
                column,
            } = &stmt.kind
            else {
                continue;
            };
            let Some(base) = path else {
                return Err(MiruError::with_column(
                    stmt.line,
                    *column,
                    "cannot import: this program was not loaded from a file",
                ));
            };
            let value = self.load_module(spec, base, stmt.line, *column)?;
            let slot = self.globals.slot_for(module, alias).ok_or_else(|| {
                MiruError::with_column(
                    stmt.line,
                    *column,
                    "too many global variables in one program",
                )
            })?;
            self.globals.define(slot, value);
        }
        Ok(())
    }

    /// Load the module `spec` names, relative to the file at `base`, and return
    /// its exports as a map.
    fn load_module(
        &mut self,
        spec: &str,
        base: &Path,
        line: usize,
        column: usize,
    ) -> Result<Value, MiruError> {
        let at = |message: String| MiruError::with_column(line, column, message);
        let joined = base.parent().unwrap_or_else(|| Path::new(".")).join(spec);
        // Report a missing file as missing. Canonicalising one that does not
        // exist fails too, but with a message about the wrong thing.
        if !joined.is_file() {
            return Err(at(format!("cannot import '{spec}': no such file")));
        }
        let canonical = joined
            .canonicalize()
            .map_err(|err| at(format!("cannot import '{spec}': {err}")))?;

        // Two spellings of one path share an entry, which is what canonicalising
        // is for.
        if let Some(cached) = self.loader.cache.get(&canonical) {
            return Ok(cached.clone());
        }
        if let Some(start) = self
            .loader
            .loading
            .iter()
            .position(|(path, _)| path == &canonical)
        {
            let mut chain: Vec<&str> = self.loader.loading[start..]
                .iter()
                .map(|(_, spec)| spec.as_str())
                .collect();
            chain.push(spec);
            return Err(at(format!("import cycle: {}", chain.join(" -> "))));
        }

        let source = std::fs::read_to_string(&canonical)
            .map_err(|err| at(format!("cannot import '{spec}': {err}")))?;
        let program = crate::parse_program(&source).map_err(|err| in_file(err, spec))?;

        self.loader
            .loading
            .push((canonical.clone(), spec.to_string()));
        let module = self.globals.new_module();
        let outcome = self.run_module(&program, &canonical, spec, module);
        self.loader.loading.pop();
        outcome.map_err(|err| in_file(err, spec))?;

        let exports = Value::map(self.globals.exports(module));
        self.loader.cache.insert(canonical, exports.clone());
        Ok(exports)
    }

    /// Resolve a module's own imports, compile it, and run it.
    ///
    /// Split out so the caller can pop its loading entry whether this succeeded
    /// or failed. Nothing here re-enters `interpret`: a module's imports are
    /// settled before it compiles, and it runs to completion before control
    /// returns, so no two runs overlap.
    fn run_module(
        &mut self,
        program: &[crate::ast::Stmt],
        canonical: &Path,
        spec: &str,
        module: ModuleId,
    ) -> Result<(), MiruError> {
        self.resolve_imports(program, Some(canonical), module)?;
        let script = crate::compiler::Compiler::compile_file(
            program,
            &mut self.globals,
            module,
            Some(Rc::new(spec.to_string())),
        )?;
        self.interpret(script)?;
        Ok(())
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
        let result = self.run_frames();
        if result.is_err() {
            self.frames.clear();
            self.stack.clear();
            self.open_upvalues.clear();
            self.tasks.clear();
            self.handlers.clear();
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
    /// before the error propagates. Here they are still intact.
    fn run_frames(&mut self) -> Result<Value, MiruError> {
        debug_assert!(
            self.tasks.is_empty(),
            "a task was pending when the loop was entered"
        );
        loop {
            match self.run_frames_inner() {
                Ok(value) => return Ok(value),
                Err(mut error) => {
                    // Captured before anything is unwound, because the frames
                    // that hold the path are about to be discarded whether this
                    // is caught or not.
                    error.trace = self.capture_trace();
                    // And the file for the same reason. The position on this
                    // error belongs to whichever file the innermost frame was
                    // compiled from, and only that frame still knows which one
                    // that is: the import that loaded it finished long ago.
                    if error.file.is_none() {
                        if let Some(frame) = self.frames.last() {
                            error.file = frame
                                .closure
                                .function
                                .file
                                .as_ref()
                                .map(|file| file.as_str().to_string());
                        }
                    }
                    // Everything the loop keeps in locals is re-read from
                    // `self` when it starts, so going round again picks up the
                    // rewound state.
                    self.catch(error)?;
                }
            }
        }
    }

    /// Hand an error to the innermost `try`, or give it back when no `try` can
    /// take it.
    ///
    /// `Ok` means the error became a value on the stack and the dispatch loop
    /// should carry on from the handler's landing. `Err` hands it back to be
    /// reported.
    ///
    /// A `Result` rather than an `Option` on purpose. With an `Option` whose
    /// `None` meant "caught", `self.handlers.pop()?` reads as the obvious
    /// spelling and means exactly the opposite: no handler would report itself
    /// as a successful catch, and the loop would resume over a stack it never
    /// rewound.
    fn catch(&mut self, error: MiruError) -> Result<(), MiruError> {
        if error.fatal {
            return Err(error);
        }
        let Some(handler) = self.handlers.pop() else {
            return Err(error);
        };
        // Upvalues first: closing one reads the stack slot it points at, and
        // those slots are what the next line throws away.
        self.close_upvalues_from(handler.stack);
        self.frames.truncate(handler.frames);
        self.stack.truncate(handler.stack);
        self.tasks.truncate(handler.tasks);
        // After truncating, the top frame is the one that ran the `try`.
        self.frames
            .last_mut()
            .expect("the frame that began the try")
            .ip = handler.ip;
        self.stack.push(Value::Error(Rc::new(error)));
        Ok(())
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
    fn run_frames_inner(&mut self) -> Result<Value, MiruError> {
        // The depth a returning frame has to reach for something to be waiting
        // for it. Zero means nothing is: the script itself is returning. A task
        // is always created while a frame runs, so its depth is at least one and
        // the two cases cannot be confused.
        //
        // A local rather than a field, so the comparison every `Return` makes
        // stays in a register. It outlives `continue 'frames`, so it is declared
        // out here.
        //
        // Read from the task stack rather than written as zero. Today that is
        // the same number, because nothing is ever pending when this loop is
        // entered: `interpret` pushes one frame and calls straight in. It stops
        // being the same number as soon as the loop can be re-entered part way
        // through a program, which is what catching an error will do.
        let mut resume_depth = self.resume_depth();
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
                                let message = with_suggestion(
                                    format!("undefined variable '{name}'"),
                                    name,
                                    self.globals.names_visible_from(slot),
                                );
                                return Err(runtime_error(chunk, op_ip, message));
                            }
                        }
                    }
                    OpCode::SetGlobal => {
                        let slot = read_u16(chunk, ip);
                        ip += 2;
                        let value = self.pop();
                        if !self.globals.assign(slot, value) {
                            let name = self.globals.name(slot);
                            let refusal = format!("cannot assign to undefined variable '{name}'");
                            // A builtin's slot refuses as well, and there the
                            // name is spelled correctly and is right there. A
                            // suggestion would answer a question nobody asked,
                            // and `eprint` is one edit from `print`.
                            let message = if self.globals.is_builtin_slot(slot) {
                                refusal
                            } else {
                                with_suggestion(
                                    refusal,
                                    name,
                                    self.globals.names_visible_from(slot),
                                )
                            };
                            return Err(runtime_error(chunk, op_ip, message));
                        }
                    }
                    OpCode::Jump => {
                        let offset = read_u16(chunk, ip);
                        ip += 2 + offset as usize;
                    }
                    OpCode::JumpIfFalse => {
                        let offset = read_u16(chunk, ip);
                        ip += 2;
                        let taken = self
                            .peek()
                            .condition()
                            .map_err(|m| runtime_error(chunk, op_ip, m))?;
                        if !taken {
                            ip += offset as usize;
                        }
                    }
                    OpCode::JumpIfTrue => {
                        let offset = read_u16(chunk, ip);
                        ip += 2;
                        let taken = self
                            .peek()
                            .condition()
                            .map_err(|m| runtime_error(chunk, op_ip, m))?;
                        if taken {
                            ip += offset as usize;
                        }
                    }
                    OpCode::Loop => {
                        let offset = read_u16(chunk, ip);
                        ip = ip + 2 - offset as usize;
                    }
                    OpCode::BeginTry => {
                        let offset = read_u16(chunk, ip);
                        ip += 2;
                        self.handlers.push(Handler {
                            frames: self.frames.len(),
                            stack: self.stack.len(),
                            tasks: self.tasks.len(),
                            ip: ip + offset as usize,
                        });
                    }
                    OpCode::EndTry => {
                        self.handlers.pop().expect("a handler to end");
                    }
                    OpCode::Truthy => {
                        let value = self.pop();
                        let truth = value
                            .condition()
                            .map_err(|m| runtime_error(chunk, op_ip, m))?;
                        self.stack.push(Value::Bool(truth));
                    }
                    OpCode::GetLocal => {
                        let slot = chunk.code[ip] as usize;
                        ip += 1;
                        let value = self.stack[slot_base + slot].clone();
                        self.stack.push(value);
                    }
                    OpCode::GetLocalLong => {
                        let slot = read_u16(chunk, ip) as usize;
                        ip += 2;
                        let value = self.stack[slot_base + slot].clone();
                        self.stack.push(value);
                    }
                    OpCode::SetLocal => {
                        let slot = chunk.code[ip] as usize;
                        ip += 1;
                        let value = self.pop();
                        self.stack[slot_base + slot] = value;
                    }
                    OpCode::SetLocalLong => {
                        let slot = read_u16(chunk, ip) as usize;
                        ip += 2;
                        let value = self.pop();
                        self.stack[slot_base + slot] = value;
                    }
                    OpCode::Array => {
                        let count = read_u16(chunk, ip) as usize;
                        ip += 2;
                        let start = self.stack.len() - count;
                        let items = self.stack.split_off(start);
                        self.stack.push(Value::array(items));
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
                        self.stack.push(Value::map(entries));
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
                    OpCode::GetField => {
                        // Same shape as Index: the operand byte holds no value,
                        // only the target expression's position, so "cannot read
                        // a field" lands under the target.
                        let target_ip = ip;
                        ip += 1;
                        let name = self.pop();
                        let target = self.pop();
                        let value = field_get(target, &name, chunk, op_ip, target_ip)?;
                        self.stack.push(value);
                    }
                    OpCode::SetIndex => {
                        let target_ip = ip;
                        ip += 1;
                        let index = self.pop();
                        let target = self.pop();
                        let value = self.pop();
                        index_set(target, &index, value, chunk, op_ip, target_ip)?;
                    }
                    OpCode::SetField => {
                        let target_ip = ip;
                        ip += 1;
                        let name = self.pop();
                        let target = self.pop();
                        let value = self.pop();
                        field_set(target, &name, value, chunk, op_ip, target_ip)?;
                    }
                    OpCode::IterSnapshot => {
                        let value = self.pop();
                        match value {
                            Value::Array(items) => {
                                let snapshot = items.borrow().clone();
                                self.stack.push(Value::array(snapshot));
                            }
                            // Iterating a caught error is a use of it, so it
                            // names the original error like every other use.
                            // Without this arm the wildcard below answered
                            // "cannot iterate over a error": true, generic, and
                            // about the type rather than what actually went
                            // wrong. It was the last consumer path v0.9 missed.
                            Value::Error(error) => {
                                return Err(runtime_error(
                                    chunk,
                                    op_ip,
                                    format!("unhandled error: {}", error.message),
                                ))
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
                        let seq_slot = slot_base + read_u16(chunk, ip) as usize;
                        let jump = read_u16(chunk, ip + 2);
                        ip += 4;
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
                            let operand = read_u16(chunk, ip + 1) as usize;
                            ip += 3;
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
                        match self.call_at_stack(argcount, chunk, op_ip)? {
                            CallOutcome::Frame => continue 'frames,
                            CallOutcome::Value => continue,
                            CallOutcome::Task => {
                                self.drive_tasks(None)?;
                                resume_depth = self.resume_depth();
                                continue 'frames;
                            }
                        }
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
                        if self.frames.len() == resume_depth {
                            // Drop the callee and its arguments. What happens to
                            // the result depends on whether anything is waiting.
                            self.stack.truncate(frame.slot_base.saturating_sub(1));
                            if resume_depth == 0 {
                                return Ok(result);
                            }
                            self.drive_tasks(Some(result))?;
                            resume_depth = self.resume_depth();
                            continue 'frames;
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
    /// builtin runs immediately and leaves its result in place. The outcome
    /// tells the run loop how to carry on.
    fn call_at_stack(
        &mut self,
        argcount: usize,
        chunk: &Chunk,
        op_ip: usize,
    ) -> Result<CallOutcome, MiruError> {
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
                    )
                    .as_fatal());
                }
                let slot_base = self.stack.len() - argcount;
                self.frames.push(CallFrame {
                    closure,
                    ip: 0,
                    slot_base,
                });
                Ok(CallOutcome::Frame)
            }
            // An ordinary builtin runs to completion here, so no frame is
            // pushed: the callee and its arguments are replaced by the result.
            Value::Builtin(_) => {
                let args = self.stack.split_off(callee_slot + 1);
                self.stack.pop();
                let (line, column) = chunk.position(op_ip);
                let result = self.call_native(callee, args, line, column)?;
                self.stack.push(result);
                Ok(CallOutcome::Value)
            }
            // A higher-order builtin does not run here at all. It checks its
            // arguments and becomes a task, which the dispatch loop drives by
            // making the calls it asks for. The callee and its arguments come
            // off the stack the same way, so the slot the result belongs in is
            // the top of the stack for as long as the task lasts.
            Value::HostBuiltin(builtin) => {
                let args = self.stack.split_off(callee_slot + 1);
                self.stack.pop();
                let (line, column) = chunk.position(op_ip);
                let mut task = (builtin.func)(args).map_err(|m| runtime_error(chunk, op_ip, m))?;
                // A callback that is an ordinary builtin never needs a frame,
                // so the task never needs to leave this function. Driving it
                // here skips the task stack, the per-element step dispatch, and
                // the enum that carries a result back out. It is the same state
                // machine, just turned by a tighter handle.
                if matches!(task.callback(), Value::Builtin(_)) {
                    let result = self.run_native_task(&mut task, line, column)?;
                    self.stack.push(result);
                    return Ok(CallOutcome::Value);
                }
                self.tasks.push(PendingTask {
                    task,
                    resume_at: self.frames.len(),
                    sink: TaskSink::Stack,
                    line,
                    column,
                });
                Ok(CallOutcome::Task)
            }
            Value::Error(error) => Err(runtime_error(
                chunk,
                op_ip,
                format!("unhandled error: {}", error.message),
            )),
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
        // No builtin is handed a caught error, bar the few that exist to
        // inspect one. It is what stops `print(r)` from turning an error the
        // program never dealt with into a line of output that looks
        // deliberate.
        //
        // Checking here covers fifty-one at once: every builtin that arrives
        // as a `Value::Builtin`, which is the plain ones, the system ones, and
        // the ambient ones.
        // It does not cover the four higher-order builtins, because those
        // become tasks in `call_at_stack` and never reach this function.
        //
        // Those four are still safe, by a different route rather than by this
        // one: a caught error is not an array and is not callable, so they
        // refuse it when they check the types of their arguments. The message
        // names the type instead of naming the unhandled error, which is worth
        // knowing before assuming this guard is what stopped it.
        //
        // `builtin_kind_counts_match_the_comments_that_quote_them` in
        // `src/builtins.rs` holds that number to the registrations.
        let inspects = match &callee {
            Value::Builtin(builtin) => crate::builtins::accepts_error(builtin.name),
            _ => false,
        };
        if !inspects {
            if let Some(message) = args.iter().find_map(crate::ops::unhandled) {
                return Err(MiruError::with_column(line, column, message));
            }
        }
        let (saved_line, saved_column) = (self.line, self.column);
        self.line = line;
        self.column = column;
        let result = match callee {
            Value::Builtin(builtin) => {
                // Each capability is moved out for the duration of the call and
                // put back after. `self` is what gets passed as the output
                // sink, so it cannot also be borrowed for a field at the same
                // time. A system builtin needs no output, but the same swap
                // applies for the same reason.
                let result = match builtin.func {
                    NativeFn::Plain(func) => {
                        let mut input = std::mem::replace(&mut self.input, Box::new(EmptyInput));
                        let result = func(self, &mut *input, args);
                        self.input = input;
                        result
                    }
                    NativeFn::System(func) => {
                        let mut system = std::mem::replace(&mut self.system, Box::new(NoSystem));
                        let result = func(&mut *system, args);
                        self.system = system;
                        result
                    }
                    // No swap here, and the difference is worth stating so the
                    // next reader does not copy one in. The two arms above pass
                    // `self`, or would borrow a field while `self` is passed.
                    // An ambient builtin is handed neither the output sink nor
                    // the input source, so borrowing the field it does need is
                    // an ordinary field borrow and nothing has to be moved out.
                    NativeFn::Ambient(func) => func(
                        &mut Ambient::new(&mut *self.clock, &mut *self.keyboard, &mut self.rng),
                        args,
                    ),
                };
                result.map_err(|message| MiruError::with_column(line, column, message))
            }
            // A higher-order builtin never arrives here. `call_at_stack` turns
            // one into a task and `begin_task_call` handles one used as a
            // callback, both before this point.
            Value::HostBuiltin(_) => unreachable!("a higher-order builtin runs as a task"),
            other => Err(MiruError::with_column(
                line,
                column,
                format!("a {} is not callable", other.type_name()),
            )),
        };
        self.line = saved_line;
        self.column = saved_column;
        // An error raised after the program asked to exit is the unwinding of
        // that exit, and `try` must not catch it. The condition is the recorded
        // code rather than the message, so this needs no agreement with what
        // `exit` happens to say and cannot be defeated by a program raising the
        // same words.
        //
        // Getting this wrong is silent rather than loud: the program keeps
        // running with an exit code already set, and its caller is told a
        // result the program never reached.
        if self.exit.is_some() {
            return result.map_err(MiruError::as_fatal);
        }
        result
    }

    /// The frame depth at which something is waiting for a returning frame, or
    /// zero when nothing is.
    fn resume_depth(&self) -> usize {
        self.tasks.last().map_or(0, |pending| pending.resume_at)
    }

    /// Build an error at the position of the call being executed, for a builtin
    /// to report against.
    pub fn call_error(&self, message: String) -> MiruError {
        MiruError::with_column(self.line, self.column, message)
    }

    /// Run a task whose callback is an ordinary builtin all the way through.
    ///
    /// Such a callback never needs a frame, so nothing about this task has to
    /// survive across turns of the dispatch loop and it need not go on the task
    /// stack at all. The callee is cloned once here rather than once per
    /// element, which is the other thing the general path cannot do, since
    /// there the task and the engine are borrowed apart.
    fn run_native_task(
        &mut self,
        task: &mut HostTask,
        line: usize,
        column: usize,
    ) -> Result<Value, MiruError> {
        let callee = task.callback().clone();
        let mut last = None;
        loop {
            let step = task
                .resume(last.take())
                .map_err(|message| MiruError::with_column(line, column, message))?;
            match step {
                Step::Done(value) => return Ok(value),
                Step::Call(args) => {
                    last = Some(self.call_native(callee.clone(), args.into_vec(), line, column)?);
                }
            }
        }
    }

    /// Carry a value to the innermost task and keep going until either a frame
    /// has been pushed to work on its behalf or its finished value has landed
    /// on the stack. Either way the caller resumes the dispatch loop.
    ///
    /// This is the whole trampoline. A higher-order builtin used to loop and
    /// call back into a nested copy of the bytecode loop, one real Rust call per
    /// element; now it says what it wants applied and this hands the answer
    /// back, all on the one loop.
    fn drive_tasks(&mut self, mut last: Option<Value>) -> Result<(), MiruError> {
        loop {
            let pending = self.tasks.last_mut().expect("a pending task");
            // The position of the call that started the task, so a refusal
            // points at the `filter(..)` rather than at whatever ran last.
            let (line, column) = (pending.line, pending.column);
            let step = pending
                .task
                .resume(last.take())
                .map_err(|message| MiruError::with_column(line, column, message))?;
            match step {
                Step::Call(args) => {
                    // The callee lives on the task rather than in the step, so
                    // it is read here. One clone per element either way; the
                    // step just got 32 bytes smaller for moving it this way.
                    let callee = self
                        .tasks
                        .last()
                        .expect("a pending task")
                        .task
                        .callback()
                        .clone();
                    match self.begin_task_call(callee, args)? {
                        TaskCall::Pushed => return Ok(()),
                        TaskCall::Value(value) => last = Some(value),
                        // A nested task starts with nothing to be told, so
                        // `last` stays empty and the next turn of the loop
                        // resumes it.
                        TaskCall::Nested => {}
                    }
                }
                Step::Done(value) => {
                    let finished = self.tasks.pop().expect("a pending task");
                    match finished.sink {
                        TaskSink::Stack => {
                            self.stack.push(value);
                            return Ok(());
                        }
                        TaskSink::Parent => last = Some(value),
                    }
                }
            }
        }
    }

    /// Apply a callee on the innermost task's behalf.
    ///
    /// Errors report at the position of the call that started the task, not
    /// wherever the program happens to be, because an error in `map`'s callback
    /// is about the `map(..)` the reader wrote.
    fn begin_task_call(&mut self, callee: Value, args: Args) -> Result<TaskCall, MiruError> {
        let pending = self.tasks.last().expect("a pending task");
        let (line, column) = (pending.line, pending.column);
        match callee {
            Value::Closure(closure) => {
                let arity = closure.function.arity;
                if args.count() != arity {
                    let name = closure.function.name.as_deref().unwrap_or("<anonymous>");
                    return Err(MiruError::with_column(
                        line,
                        column,
                        format!(
                            "function {name} expects {arity} argument(s) but received {}",
                            args.count()
                        ),
                    ));
                }
                if self.frames.len() >= MAX_CALL_DEPTH {
                    return Err(MiruError::with_column(
                        line,
                        column,
                        format!("call depth limit of {MAX_CALL_DEPTH} exceeded"),
                    )
                    .as_fatal());
                }
                // Mirror the bytecode layout: the callee sits beneath its args.
                self.stack.push(Value::Closure(Rc::clone(&closure)));
                let slot_base = self.stack.len();
                args.push_onto(&mut self.stack);
                self.frames.push(CallFrame {
                    closure,
                    ip: 0,
                    slot_base,
                });
                Ok(TaskCall::Pushed)
            }
            // A higher-order builtin as a callback becomes a task of its own,
            // whose value belongs to the task that asked for it rather than to
            // the stack.
            Value::HostBuiltin(builtin) => {
                let task = (builtin.func)(args.into_vec())
                    .map_err(|m| MiruError::with_column(line, column, m))?;
                self.tasks.push(PendingTask {
                    task,
                    resume_at: self.frames.len(),
                    sink: TaskSink::Parent,
                    line,
                    column,
                });
                Ok(TaskCall::Nested)
            }
            other => Ok(TaskCall::Value(self.call_native(
                other,
                args.into_vec(),
                line,
                column,
            )?)),
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

/// Read `target.name` for a map, or fail.
///
/// The difference from [`index_get`] is the whole reason this exists: a field
/// that is not there is an error, where a map key that is not there reads as
/// `nil`. Indexing is a lookup that may miss; field access asserts the field is
/// present, so a mistyped module member fails where it is written rather than
/// turning into a `nil` that surfaces somewhere else.
fn field_get(
    target: Value,
    name: &Value,
    chunk: &Chunk,
    access_ip: usize,
    target_ip: usize,
) -> Result<Value, MiruError> {
    // The compiler always emits a string constant here, so this only guards a
    // hand-built chunk. It is an error rather than a panic because the VM is
    // driven directly by tests and by the disassembler's readers.
    let key = match name {
        Value::Str(key) => key,
        other => {
            return Err(runtime_error(
                chunk,
                access_ip,
                format!("a field name must be a string, not a {}", other.type_name()),
            ))
        }
    };
    match target {
        Value::Map(entries) => {
            let entries = entries.borrow();
            match entries.get(key.as_str()) {
                Some(value) => Ok(value.clone()),
                None => {
                    let message = with_suggestion(
                        format!("no field '{key}'"),
                        key,
                        entries.keys().map(String::as_str),
                    );
                    Err(runtime_error(chunk, access_ip, message))
                }
            }
        }
        // The one thing a program may do with an error besides ask its type.
        // Reading is not using: it is how a program decides what to do, so it
        // has to be allowed or the guard would forbid the very thing it exists
        // to make people do.
        Value::Error(error) => error_field(&error, key, chunk, access_ip),
        other => Err(runtime_error(
            chunk,
            target_ip,
            format!("cannot read a field of a {}", other.type_name()),
        )),
    }
}

/// The fields an error has, in the order the specification lists them.
///
/// Named here so a message can offer one back. The match below is still what
/// reads them, and `an_error_answers_for_every_field_this_list_names` holds the
/// two together.
const ERROR_FIELDS: [&str; 5] = ["message", "line", "column", "file", "trace"];

/// Read one of an error's fields.
///
/// A closed set rather than a map, so a misspelling fails the way `m.nope`
/// does instead of quietly reading `nil`.
fn error_field(
    error: &MiruError,
    key: &str,
    chunk: &Chunk,
    access_ip: usize,
) -> Result<Value, MiruError> {
    let value = match key {
        "message" => Value::Str(Rc::new(error.message.clone())),
        "line" => Value::Int(error.line as i64),
        "column" => Value::Int(error.column as i64),
        // `nil` rather than an empty string when the error came from the file
        // being run, so "no file" is distinguishable from a file named "".
        "file" => match &error.file {
            Some(file) => Value::Str(Rc::new(file.clone())),
            None => Value::Nil,
        },
        // The call path the error came through, innermost first, worded as
        // the terminal prints it. Knowing an error happened is much less
        // useful than knowing where it came from, and a caught error would
        // otherwise be the one place that knowledge is thrown away.
        "trace" => {
            let entries: Vec<Value> = error
                .trace
                .iter()
                .map(|entry| {
                    let name = entry.function.as_deref().unwrap_or("<anonymous>");
                    Value::Str(Rc::new(format!(
                        "in {name}, called from line {}",
                        entry.line
                    )))
                })
                .collect();
            Value::array(entries)
        }
        other => {
            let message = with_suggestion(
                format!("an error has no field '{other}'"),
                other,
                ERROR_FIELDS,
            );
            return Err(runtime_error(chunk, access_ip, message));
        }
    };
    Ok(value)
}

/// Assign `target.name = value` for a map, or fail for anything else.
///
/// Where [`field_get`] insists the name is already there, this does not: a
/// field that does not exist is created, exactly as `target["name"] = value`
/// creates a key. The asymmetry is deliberate. Reading a name that is not there
/// is almost always a misspelling, and assigning to one almost never is.
fn field_set(
    target: Value,
    name: &Value,
    value: Value,
    chunk: &Chunk,
    access_ip: usize,
    target_ip: usize,
) -> Result<(), MiruError> {
    // As in `field_get`, the compiler always emits a string constant here, so
    // this only guards a hand-built chunk.
    let key = match name {
        Value::Str(key) => key,
        other => {
            return Err(runtime_error(
                chunk,
                access_ip,
                format!("a field name must be a string, not a {}", other.type_name()),
            ))
        }
    };
    match target {
        Value::Map(entries) => {
            entries.borrow_mut().insert(key.to_string(), value);
            Ok(())
        }
        Value::Error(error) => Err(runtime_error(
            chunk,
            target_ip,
            format!("unhandled error: {}", error.message),
        )),
        other => Err(runtime_error(
            chunk,
            target_ip,
            format!("cannot assign a field of a {}", other.type_name()),
        )),
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
            // Borrowed, not copied: this only looks the key up, and a `BTreeMap`
            // keyed by `String` accepts a `&str`. Copying here allocated a
            // string per read and dropped it on the next line.
            let key = crate::ops::map_key_ref(index)
                .map_err(|message| runtime_error(chunk, index_ip, message))?;
            Ok(entries.borrow().get(key).cloned().unwrap_or(Value::Nil))
        }
        Value::Error(error) => Err(runtime_error(
            chunk,
            target_ip,
            format!("unhandled error: {}", error.message),
        )),
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
        Value::Error(error) => Err(runtime_error(
            chunk,
            target_ip,
            format!("unhandled error: {}", error.message),
        )),
        other => Err(runtime_error(
            chunk,
            target_ip,
            format!("cannot index-assign to a {}", other.type_name()),
        )),
    }
}

/// Build a runtime error at the source position of the byte at `offset`.
/// Tag an error with the module it came from, unless a deeper one already did.
///
/// The innermost file wins, so an error three imports down names the file it is
/// actually in rather than the one at the top of the chain.
fn in_file(mut error: MiruError, spec: &str) -> MiruError {
    if error.file.is_none() {
        error.file = Some(spec.to_string());
    }
    error
}

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
            file: None,
        });
        Vm::new().interpret(script)
    }

    /// A map value, for the field tests.
    fn map_of(pairs: &[(&str, Value)]) -> Value {
        let mut entries = BTreeMap::new();
        for (key, value) in pairs {
            entries.insert((*key).to_string(), value.clone());
        }
        Value::map(entries)
    }

    /// A caught error, which no program can produce yet. Putting one in the
    /// constant pool is how these tests reach the paths that must refuse it.
    fn caught_error() -> Value {
        Value::Error(Rc::new(MiruError::with_column(1, 9, "division by zero")))
    }

    /// The message every refusal gives, which names the error being held
    /// rather than the type it happens to be.
    const REFUSED: &str = "unhandled error: division by zero";

    #[test]
    fn a_conditional_refuses_a_caught_error_rather_than_reading_it_as_false() {
        // The dangerous one. `if` and `while` ask a value whether it is truthy,
        // and every value answers, so an error would take the else branch and
        // look like an ordinary falsy value.
        for op in [OpCode::JumpIfFalse, OpCode::JumpIfTrue, OpCode::Truthy] {
            let error = run(|chunk| {
                constant(chunk, caught_error());
                chunk.write_op(op, 4, 6);
                if op != OpCode::Truthy {
                    chunk.write(0, 4, 6);
                    chunk.write(0, 4, 6);
                }
            })
            .err()
            .expect("a conditional must refuse an error");
            assert_eq!(error.message, REFUSED, "{op:?} accepted an error");
            // Reported where the misuse is written, not where the error was.
            assert_eq!((error.line, error.column), (4, 6), "{op:?}");
        }
    }

    #[test]
    fn an_error_answers_for_every_field_this_list_names() {
        // `ERROR_FIELDS` is what a message offers back, and the match in
        // `error_field` is what actually reads one. Nothing makes them agree,
        // so a field added to one and not the other would suggest a name that
        // then fails, or fail on a name it never offers.
        for field in ERROR_FIELDS {
            run(|chunk| {
                constant(chunk, caught_error());
                get_field(chunk, field);
            })
            .unwrap_or_else(|error| panic!("'{field}' is listed but not read: {}", error.message));
        }
        assert_eq!(ERROR_FIELDS.len(), 5, "the guarantee promises five fields");
    }

    #[test]
    fn an_error_answers_for_its_own_fields_and_nothing_else() {
        // Reading is not using. A program has to be able to ask what went
        // wrong, so this is the one door the guard leaves open, and it opens
        // exactly as far as a closed set of names.
        let value = run(|chunk| {
            constant(chunk, caught_error());
            get_field(chunk, "message");
        })
        .expect("an error answers for its own fields");
        assert!(value
            .equals(&Value::Str(Rc::new("division by zero".to_string())))
            .unwrap());

        // A misspelling fails rather than reading nil, the same bargain field
        // access makes on a map.
        let error = run(|chunk| {
            constant(chunk, caught_error());
            get_field(chunk, "mesage");
        })
        .err()
        .expect("an unknown field must be refused");
        assert_eq!(
            error.message,
            "an error has no field 'mesage'. Did you mean 'message'?"
        );

        // Indexing is not reading a field, and stays refused: `r["message"]`
        // would answer nil for a name that is not there, which is the whole
        // thing GetField exists to avoid.
        let error = run(|chunk| {
            constant(chunk, caught_error());
            constant(chunk, Value::Int(0));
            chunk.write_op(OpCode::Index, 1, 1);
            chunk.write(0, 1, 1);
        })
        .err()
        .expect("indexing an error must be refused");
        assert_eq!(error.message, REFUSED);
    }

    #[test]
    fn a_caught_error_is_not_callable_and_is_not_an_argument() {
        let error = run(|chunk| {
            constant(chunk, caught_error());
            chunk.write_op(OpCode::Call, 1, 1);
            chunk.write(0, 1, 1);
        })
        .err()
        .expect("calling an error must be refused");
        assert_eq!(error.message, REFUSED);

        // A builtin that accepts anything, which print() and str() do. Without a
        // guard this would have handed on an error the program never dealt
        // with, as though it were a result.
        fn echo(
            _out: &mut dyn Output,
            _input: &mut dyn Input,
            args: Vec<Value>,
        ) -> Result<Value, String> {
            Ok(args.into_iter().next().unwrap_or(Value::Nil))
        }
        let error = run(|chunk| {
            constant(
                chunk,
                Value::Builtin(crate::value::Builtin {
                    name: "echo",
                    func: NativeFn::Plain(echo),
                }),
            );
            constant(chunk, caught_error());
            chunk.write_op(OpCode::Call, 1, 1);
            chunk.write(1, 1, 1);
        })
        .err()
        .expect("a builtin must not be handed an error");
        assert_eq!(error.message, REFUSED);
    }

    /// Emit `target.name`, with the position-only operand byte the opcode
    /// carries.
    fn get_field(chunk: &mut Chunk, name: &str) {
        constant(chunk, Value::Str(Rc::new(name.to_string())));
        chunk.write_op(OpCode::GetField, 1, 1);
        chunk.write(0, 1, 1);
    }

    #[test]
    fn a_field_reads_the_value_stored_under_that_name() {
        let value = run(|chunk| {
            constant(chunk, map_of(&[("a", Value::Int(1)), ("b", Value::Int(2))]));
            get_field(chunk, "b");
        })
        .expect("runs");
        assert_eq!(value.repr(), "2");
    }

    #[test]
    fn a_field_that_is_not_there_is_an_error_where_an_index_would_be_nil() {
        // This one difference is the whole reason the opcode exists beside
        // Index. `m["nope"]` is a lookup that may miss and reads as nil;
        // `m.nope` asserts the field is there.
        let error = run(|chunk| {
            constant(chunk, map_of(&[("a", Value::Int(1))]));
            get_field(chunk, "nope");
        })
        .err()
        .expect("an error");
        assert_eq!(error.message, "no field 'nope'");
    }

    #[test]
    fn a_field_of_a_non_map_names_the_type() {
        let error = run(|chunk| {
            constant(chunk, Value::Int(5));
            get_field(chunk, "a");
        })
        .err()
        .expect("an error");
        assert_eq!(error.message, "cannot read a field of a int");
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
            file: None,
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
        assert!(result.equals(&Value::Int(20)).unwrap());
    }

    #[test]
    fn evaluates_unary_and_comparison() {
        let negated = run(|chunk| {
            constant(chunk, Value::Int(5));
            chunk.write_op(OpCode::Negate, 1, 1);
        })
        .unwrap();
        assert!(negated.equals(&Value::Int(-5)).unwrap());

        let compared = run(|chunk| {
            constant(chunk, Value::Int(1));
            constant(chunk, Value::Int(2));
            chunk.write_op(OpCode::Less, 1, 1);
        })
        .unwrap();
        assert!(compared.equals(&Value::Bool(true)).unwrap());
    }

    #[test]
    fn pushes_literals() {
        let value = run(|chunk| {
            chunk.write_op(OpCode::True, 1, 1);
            chunk.write_op(OpCode::Not, 1, 1);
        })
        .unwrap();
        assert!(value.equals(&Value::Bool(false)).unwrap());
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

    /// The two streams reach two different places.
    ///
    /// This is the whole reason `err` is a separate sink rather than a flag on
    /// `out`. A single buffer holding both would pass any test that only checks
    /// the text is somewhere, and would still be useless to a caller trying to
    /// redirect one and keep the other.
    #[test]
    fn output_and_diagnostics_go_to_different_sinks() {
        let out = Rc::new(RefCell::new(Vec::<u8>::new()));
        let err = Rc::new(RefCell::new(Vec::<u8>::new()));
        let mut vm = Vm::with_output(Box::new(SharedSink(Rc::clone(&out))));
        vm.set_error_output(Box::new(SharedSink(Rc::clone(&err))));

        vm.write("to stdout\n");
        vm.write_error("to stderr\n");
        vm.flush();

        assert_eq!(String::from_utf8_lossy(&out.borrow()), "to stdout\n");
        assert_eq!(String::from_utf8_lossy(&err.borrow()), "to stderr\n");
    }

    /// Nothing has asked to exit until something asks.
    #[test]
    fn an_exit_code_is_absent_until_it_is_requested() {
        let mut vm = Vm::new();
        assert_eq!(vm.exit_code(), None);
        vm.request_exit(3);
        assert_eq!(vm.exit_code(), Some(3));
        // The last request wins rather than the first, which is what a caller
        // would expect if two ever raced. Nothing calls this twice today.
        vm.request_exit(0);
        assert_eq!(vm.exit_code(), Some(0));
    }

    /// A `Write` sink over a shared buffer, so a test can read back what the VM
    /// sent to each stream.
    struct SharedSink(Rc<RefCell<Vec<u8>>>);

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
