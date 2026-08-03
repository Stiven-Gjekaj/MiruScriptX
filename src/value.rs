//! Runtime values, plus the output sink that builtins write to.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::chunk::Chunk;

/// What the host gives a program to speak through, and to stop with. The
/// virtual machine implements this, so the very same builtins can target real
/// streams in the binary or in-memory buffers in tests.
///
/// This is not [`System`]. `System` is the capability trait, absent by default,
/// which is why `read_file` refuses in a browser. These three are never absent:
/// every host can take a program's output, and every host can be told that a
/// program has finished and with what result. A browser answers "this program
/// ended with code 2" as honestly as a shell does.
///
/// Widening this trait rather than [`BuiltinFn`] is deliberate. A plain builtin
/// already receives `&mut dyn Output`, so `eprint` and `exit` are ordinary
/// builtins and every other builtin is untouched.
pub trait Output {
    /// The program's result. `print` writes here.
    fn write(&mut self, text: &str);

    /// The program's diagnostics. `eprint` writes here.
    ///
    /// A host with one stream may send these to the same place as
    /// [`Output::write`]. What a host may not do is discard them.
    fn write_error(&mut self, text: &str);

    /// Record that the program asked to stop, and with what code.
    ///
    /// This only records. Stopping is the caller's business: the builtin that
    /// calls this then returns an error to unwind, and that error has to be
    /// fatal or a `try` will swallow the exit and let the program run on with a
    /// code already set.
    fn request_exit(&mut self, code: i32);
}

/// A source of input lines that builtins such as `input` read from. Like
/// [`Output`], this is abstracted so the binary can read real stdin while tests
/// feed a scripted buffer.
pub trait Input {
    /// Read the next line of input, without its trailing newline, or `None` at
    /// end of input.
    fn read_line(&mut self) -> Option<String>;
}

/// An [`Input`] that is always at end of input. This is the default when no
/// input source is supplied, for example in `run_capture`.
pub struct EmptyInput;

impl Input for EmptyInput {
    fn read_line(&mut self) -> Option<String> {
        None
    }
}

/// The shared signature of every native (Rust-implemented) builtin.
pub type BuiltinFn = fn(&mut dyn Output, &mut dyn Input, Vec<Value>) -> Result<Value, String>;

/// The host's file system and command line.
///
/// Separate from [`Output`] and [`Input`] because it is the one capability a
/// host may not have at all. A browser has a screen and a keyboard; it has no
/// files and no command line.
///
/// Absent is the **default**, not the exception. Reading a file the obvious way
/// would compile `std::fs` into the WebAssembly build, which has no file system
/// and no way to say so: the page would show whatever error the platform
/// happened to produce. There is no conditional compilation anywhere in this
/// crate and this is not the place to start one, because the question is not
/// what the target supports but what the embedder permits. So the capability is
/// a value the host supplies, and [`NoSystem`] is what everything gets until
/// something says otherwise.
pub trait System {
    fn read_file(&mut self, path: &str) -> Result<String, String>;
    fn write_file(&mut self, path: &str, contents: &str) -> Result<(), String>;
    fn file_exists(&mut self, path: &str) -> bool;
    /// The arguments the program was given, not counting its own name.
    fn arguments(&self) -> Vec<String>;
}

/// A [`System`] with none of it.
///
/// Every call refuses rather than returning nothing. `input()` at end of input
/// gives `nil` and a program carries on, which suits a missing line; a missing
/// file does not, because a program that reads a file and silently gets nothing
/// goes on to do the wrong thing with it. `import` takes the same view, and
/// `try` makes either recoverable.
pub struct NoSystem;

impl NoSystem {
    /// One sentence, used by each refusal, naming the situation rather than the
    /// operation, because a reader who sees it needs to know the host has no
    /// files at all and not that this one call went wrong.
    const REFUSAL: &'static str = "this program is running where there is no file system";
}

impl System for NoSystem {
    fn read_file(&mut self, _path: &str) -> Result<String, String> {
        Err(NoSystem::REFUSAL.to_string())
    }

    fn write_file(&mut self, _path: &str, _contents: &str) -> Result<(), String> {
        Err(NoSystem::REFUSAL.to_string())
    }

    fn file_exists(&mut self, _path: &str) -> bool {
        false
    }

    fn arguments(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The signature of a builtin that needs the host's file system or command
/// line. It gets [`System`] and nothing else, because none of them writes.
pub type SystemFn = fn(&mut dyn System, Vec<Value>) -> Result<Value, String>;

/// A function compiled to bytecode. The whole program is itself one of these,
/// an anonymous script with no parameters.
pub struct CompiledFunction {
    pub name: Option<String>,
    pub arity: usize,
    pub chunk: Chunk,
    /// The module this function was compiled from, or `None` for the file being
    /// run.
    ///
    /// A module's functions outlive the import that loaded them, so by the time
    /// one is called the loader is long gone and cannot say where it came from.
    /// Without this, a runtime error inside an imported function reported a
    /// line and column belonging to that file against the *importing* file's
    /// source, and drew a caret on whatever happened to be on that line.
    pub file: Option<Rc<String>>,
}

/// A captured variable shared by a closure. While the enclosing function is
/// still running, the upvalue is `Open` and points at a stack slot, so writes on
/// either side are seen by both. Once that function returns, the value is moved
/// into the upvalue (`Closed`) and outlives the stack.
pub enum Upvalue {
    Open(usize),
    Closed(Value),
}

/// A [`CompiledFunction`] paired with the variables it captured from enclosing
/// functions. The captured upvalues are shared (`Rc<RefCell<..>>`) so several
/// closures over the same variable observe each other's changes.
pub struct Closure {
    pub function: Rc<CompiledFunction>,
    pub upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

/// What an array value points at.
///
/// A newtype over the `RefCell` it used to be, so that the array's contents can
/// be released without recursion. A `Value` holding a `Value` holding a `Value`
/// is a chain that the compiler's own destructor walks one frame at a time, and
/// a program can build one longer than the stack. The teardown that fixes that
/// has to live on a type of this crate's own, and `RefCell<Vec<Value>>` is not
/// one.
///
/// It dereferences to the `RefCell`, so `borrow` and `borrow_mut` reach through
/// it exactly as they did before and no reader has to know it is here.
pub struct ArrayBody(RefCell<Vec<Value>>);

impl ArrayBody {
    pub fn new(items: Vec<Value>) -> ArrayBody {
        ArrayBody(RefCell::new(items))
    }
}

impl std::ops::Deref for ArrayBody {
    type Target = RefCell<Vec<Value>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// What a map value points at. [`ArrayBody`] explains why it exists.
pub struct MapBody(RefCell<BTreeMap<String, Value>>);

impl MapBody {
    pub fn new(entries: BTreeMap<String, Value>) -> MapBody {
        MapBody(RefCell::new(entries))
    }
}

impl std::ops::Deref for MapBody {
    type Target = RefCell<BTreeMap<String, Value>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Release a chain of values without one stack frame per link.
///
/// A value can hold a value that holds a value, as deep as a loop cares to go:
///
/// ```text
/// let a = []
/// while .. { a = [a] }
/// ```
///
/// The destructor Rust writes for that walks the chain by recursion, so
/// releasing it overflowed the stack and aborted the process. It happened at
/// the assignment that dropped the last reference, or at the end of the
/// program, where it also lost whatever was still buffered on standard output.
/// Nothing could catch it, because by then the program had finished.
///
/// So the children go on a list instead of on the stack. Each container taken
/// off the list surrenders its own children to the list before it is released,
/// and the loop keeps going until the list is empty. Depth becomes length, and
/// length is heap.
///
/// Two rules matter more than the shape:
///
/// - **Descend only into a body nobody else holds.** `Rc::try_unwrap` succeeds
///   for exactly those. Descending into a shared one would release values that
///   another reference still points at.
/// - **Empty a body before letting it fall out of scope.** Its own destructor
///   runs at that moment, and finding nothing left is what stops it recursing
///   back into the case this function exists to avoid.
fn release(mut work: Vec<Value>) {
    while let Some(value) = work.pop() {
        match value {
            Value::Array(rc) => {
                if let Ok(body) = Rc::try_unwrap(rc) {
                    work.append(&mut body.borrow_mut());
                }
            }
            Value::Map(rc) => {
                if let Ok(body) = Rc::try_unwrap(rc) {
                    let mut entries = body.borrow_mut();
                    work.extend(std::mem::take(&mut *entries).into_values());
                }
            }
            Value::Closure(rc) => {
                if let Ok(mut closure) = Rc::try_unwrap(rc) {
                    // Taken rather than moved out, for two reasons. `Closure`
                    // implements `Drop`, and Rust does not let a field be moved
                    // out of such a type. And leaving the captures in place
                    // would hand them to `Closure::drop`, which calls this
                    // function again: the recursion would be back, one level
                    // further down. Emptied first, that destructor finds
                    // nothing and returns.
                    work.extend(closed_captures(&mut closure.upvalues));
                }
            }
            // Everything else is a leaf. `Value::Error` looks like it might not
            // be, but `MiruError` holds a line, a column, a message, and a trace
            // of plain Rust structs. No value hides in one.
            _ => {}
        }
    }
}

/// Take the values a closure had captured, leaving it holding none.
///
/// Only a capture nobody else shares yields a value. An upvalue still open
/// points at a stack slot and owns nothing, and one another closure also holds
/// is not this closure's to release.
fn closed_captures(upvalues: &mut Vec<Rc<RefCell<Upvalue>>>) -> Vec<Value> {
    std::mem::take(upvalues)
        .into_iter()
        .filter_map(|slot| match Rc::try_unwrap(slot) {
            Ok(upvalue) => match upvalue.into_inner() {
                Upvalue::Closed(inner) => Some(inner),
                Upvalue::Open(_) => None,
            },
            Err(_) => None,
        })
        .collect()
}

impl Drop for ArrayBody {
    fn drop(&mut self) {
        release(std::mem::take(&mut *self.borrow_mut()));
    }
}

impl Drop for MapBody {
    fn drop(&mut self) {
        release(
            std::mem::take(&mut *self.borrow_mut())
                .into_values()
                .collect(),
        );
    }
}

impl Drop for Closure {
    fn drop(&mut self) {
        // A closure holds its captures, and a capture can hold a closure, so
        // rebinding one in a loop builds a chain exactly as an array does:
        //
        //     let f = base
        //     while .. { let g = f  f = fn() { return g() } }
        release(closed_captures(&mut self.upvalues));
    }
}

/// A native function implemented in Rust and exposed to programs.
#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub func: NativeFn,
}

/// What a [`Builtin`] is, which is a question about what it is handed.
///
/// Not a variant of [`Value`], the way [`HostBuiltin`] is. That one is separate
/// because a higher-order builtin *behaves* differently: the engine drives it as
/// a task rather than calling it. These are called exactly like any other
/// builtin and differ only in their argument, so they belong here, where they
/// cost no new arm in `type_name`, in printing, in comparing, or in the
/// playground's list of names to highlight.
#[derive(Clone, Copy)]
pub enum NativeFn {
    /// Takes the output sink and the input source, as most builtins do.
    Plain(BuiltinFn),
    /// Takes the host's file system and command line.
    System(SystemFn),
}

/// The signature of a higher-order builtin: it checks its arguments and returns
/// the task that carries the work out, which the engine then drives.
///
/// It is not handed the engine, because it no longer calls back into it. A task
/// says what it wants applied and is resumed with the answer, so the applying
/// happens on the one dispatch loop rather than on a nested one. Errors are
/// plain strings, as for every other builtin, and the virtual machine attaches
/// the position of the call.
pub type HostFn = fn(Vec<Value>) -> Result<crate::builtins::HostTask, String>;

/// A native builtin that runs as a task, used by the higher-order builtins
/// `map`, `filter`, and `reduce`.
#[derive(Clone)]
pub struct HostBuiltin {
    pub name: &'static str,
    pub func: HostFn,
}

/// A MiruScriptX runtime value. Strings, arrays, and functions are reference
/// counted so they are cheap to pass around and share.
#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<String>),
    Array(Rc<ArrayBody>),
    Map(Rc<MapBody>),
    Closure(Rc<Closure>),
    Builtin(Builtin),
    HostBuiltin(HostBuiltin),
    Nil,
    /// An error that `try` caught, carried as a value.
    ///
    /// This is the error itself rather than a copy of its parts, so re-raising
    /// one prints exactly what the terminal would have printed had it never
    /// been caught, position and call trace and all.
    ///
    /// Nothing constructs one yet. The variant lands before `try` does so that
    /// every path which consumes a value has already been taught what to do
    /// with it.
    Error(Rc<crate::MiruError>),
}

/// How wide a `Value` is, pinned.
///
/// The virtual machine's stack is a `Vec<Value>`, so is a chunk's constant pool,
/// and so is the inside of every array. This number is therefore the unit of
/// almost all the copying the engine does: every `GetLocal` clones one, every
/// push moves one, and every time a `Vec<Value>` grows it copies this many bytes
/// per element.
///
/// It is set by the **largest** variant, not the common one. `Value::Int` needs
/// 8 bytes and gets this many. So the figure is worth watching, and until now
/// nothing watched it: adding a field to a rarely-used variant silently widens
/// the hot path.
///
/// Four pointers wide: 32 bytes where a pointer is 8, and 16 on a 32-bit target
/// such as the WebAssembly build. Three of those four exist to describe a
/// builtin (`&'static str` plus a function pointer and its tag), which is a
/// thing the stack holds approximately never. The discriminant is free: rustc
/// niche-fills it into the non-null pointer inside that name.
///
/// Written against the pointer rather than as a byte count, because the first
/// version of this assertion said 32 and broke the WebAssembly build, where a
/// pointer is half as wide. The invariant was never about bytes.
const _: () = assert!(
    std::mem::size_of::<Value>() == 4 * std::mem::size_of::<usize>(),
    "Value changed size; the VM stack is a Vec<Value>, so measure before accepting it"
);

impl Value {
    /// Build an array value.
    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(ArrayBody::new(items)))
    }

    /// Build a map value.
    pub fn map(entries: BTreeMap<String, Value>) -> Value {
        Value::Map(Rc::new(MapBody::new(entries)))
    }

    /// The name of this value's type, as returned by the `type` builtin.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Array(_) => "array",
            Value::Map(_) => "map",
            Value::Closure(_) | Value::Builtin(_) | Value::HostBuiltin(_) => "function",
            Value::Nil => "nil",
            Value::Error(_) => "error",
        }
    }

    /// Truthiness rule: only `false` and `nil` are falsy.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Bool(false) | Value::Nil)
    }

    /// Truthiness where a caught error must not pass silently, for `if`,
    /// `while`, and the short-circuiting operators.
    ///
    /// An error is deliberately not falsy. Making it falsy would read well
    /// (`if r { .. }`) right up to the first successful `0`, `false`, `nil`, or
    /// empty string, which would then be indistinguishable from an error. So
    /// the program has to say which it means.
    ///
    /// One match rather than [`Value::is_truthy`] plus a separate check, so a
    /// conditional reads the discriminant once as it always did and pays only
    /// for the extra arm.
    pub fn condition(&self) -> Result<bool, String> {
        match self {
            Value::Bool(false) | Value::Nil => Ok(false),
            Value::Error(error) => Err(format!("unhandled error: {}", error.message)),
            _ => Ok(true),
        }
    }

    /// The plain display form used by `print` and `str`: strings appear without
    /// surrounding quotes.
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) => s.as_str().to_string(),
            other => other.repr(),
        }
    }

    /// The inspect form used by the REPL and inside arrays: strings are quoted
    /// and escaped, and floats always carry a decimal point.
    ///
    /// A container that contains itself prints as `[...]` or `{...}` at the
    /// point it comes round again, so `a` holding `a` shows as `[[...]]` rather
    /// than recursing until the process aborts. Nesting past
    /// [`Value::MAX_DEPTH`] truncates the same way, which catches the deep but
    /// acyclic case that no identity check would see.
    ///
    /// Printing truncates where comparing refuses, because printing always has
    /// something true to show and the ellipsis is it: there is more here than
    /// is worth printing.
    pub fn repr(&self) -> String {
        self.repr_within(Value::MAX_DEPTH, &mut Vec::new())
    }

    /// `open` holds the address of every container between the top-level call
    /// and this one. A container already on it is one this call is inside, so
    /// descending would not terminate.
    fn repr_within(&self, depth: usize, open: &mut Vec<usize>) -> String {
        let address = match self {
            Value::Array(items) => Some(Rc::as_ptr(items) as usize),
            Value::Map(entries) => Some(Rc::as_ptr(entries) as usize),
            _ => None,
        };
        if let Some(address) = address {
            if open.contains(&address) || depth == 0 {
                return match self {
                    Value::Array(_) => "[...]".to_string(),
                    Value::Map(_) => "{...}".to_string(),
                    _ => unreachable!("only a container has an address"),
                };
            }
            open.push(address);
        }
        let text = self.repr_parts(depth.saturating_sub(1), open);
        if address.is_some() {
            open.pop();
        }
        text
    }

    fn repr_parts(&self, depth: usize, open: &mut Vec<usize>) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Bool(b) => b.to_string(),
            Value::Nil => "nil".to_string(),
            Value::Str(s) => quoted_string(s),
            Value::Array(items) => {
                let parts: Vec<String> = items
                    .borrow()
                    .iter()
                    .map(|item| item.repr_within(depth, open))
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Map(entries) => {
                let parts: Vec<String> = entries
                    .borrow()
                    .iter()
                    .map(|(key, value)| {
                        format!("{}: {}", quoted_string(key), value.repr_within(depth, open))
                    })
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Closure(closure) => match &closure.function.name {
                Some(name) => format!("<fn {name}>"),
                None => "<fn>".to_string(),
            },
            Value::Builtin(builtin) => format!("<builtin {}>", builtin.name),
            Value::HostBuiltin(builtin) => format!("<builtin {}>", builtin.name),
            // Angle brackets rather than the plain message, for the same reason
            // a function prints as `<fn name>`: what this shows is a thing the
            // program is holding, not text it produced.
            Value::Error(error) => format!("<error: {}>", error.message),
        }
    }

    /// How deep [`Value::equals`] and [`Value::repr`] will walk a nested value
    /// before they stop.
    ///
    /// Arrays and maps can hold themselves, and both of these used to recurse
    /// on one until the process aborted on a Rust stack overflow: no caret, no
    /// trace, and uncatchable by `try`, which is not an outcome a program should
    /// be able to cause. 256 is far above any nesting real data has and far
    /// below the depth that overflows.
    const MAX_DEPTH: usize = 256;

    /// Structural value equality, with numeric promotion so `1 == 1.0`.
    ///
    /// Fails rather than aborting when the values nest deeper than
    /// [`Value::MAX_DEPTH`]. Comparing is a question that can have no answer;
    /// printing always has one, which is why [`Value::repr`] truncates instead.
    pub fn equals(&self, other: &Value) -> Result<bool, String> {
        self.equals_within(other, Value::MAX_DEPTH)
    }

    fn equals_within(&self, other: &Value, depth: usize) -> Result<bool, String> {
        // Identity first, so a value that holds itself compares equal to itself
        // without walking into the cycle at all. This is the common case and it
        // has a right answer.
        match (self, other) {
            (Value::Array(a), Value::Array(b)) if Rc::ptr_eq(a, b) => return Ok(true),
            (Value::Map(a), Value::Map(b)) if Rc::ptr_eq(a, b) => return Ok(true),
            _ => {}
        }
        let Some(depth) = depth.checked_sub(1) else {
            return Err("value is nested too deeply to compare".to_string());
        };
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Ok(a == b),
            (Value::Float(a), Value::Float(b)) => Ok(a == b),
            (Value::Int(a), Value::Float(b)) => Ok((*a as f64) == *b),
            (Value::Float(a), Value::Int(b)) => Ok(*a == (*b as f64)),
            (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
            (Value::Str(a), Value::Str(b)) => Ok(a == b),
            (Value::Nil, Value::Nil) => Ok(true),
            (Value::Array(a), Value::Array(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !x.equals_within(y, depth)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Value::Closure(a), Value::Closure(b)) => Ok(Rc::ptr_eq(a, b)),
            (Value::Builtin(a), Value::Builtin(b)) => Ok(a.name == b.name),
            (Value::HostBuiltin(a), Value::HostBuiltin(b)) => Ok(a.name == b.name),
            (Value::Map(a), Value::Map(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (key, value) in a.iter() {
                    match b.get(key) {
                        Some(other) if value.equals_within(other, depth)? => {}
                        _ => return Ok(false),
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let text = f.to_string();
    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// A string with quotation marks around it and the escapes the lexer reads.
///
/// One function for two jobs that must not drift: what `miru fmt` writes into a
/// source file, and what a program prints for a string inside an array or a
/// map. Both are text for a person to read, so both spell a character a person
/// cannot read rather than writing it.
///
/// What counts as unreadable is [`char::is_control`], which is the Unicode
/// category rather than a range written out here. That covers `00` to `1F`,
/// `7F`, and `80` to `9F`. The four with a short spelling are matched first and
/// keep it.
pub fn quoted_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            // Written raw before 1.4, which made `miru fmt` put a byte into a
            // source file that no editor shows and a copy and paste loses.
            // `\u{...}`, added in 1.3, is what makes writing it back possible.
            c if c.is_control() => out.push_str(&format!("\\u{{{:X}}}", c as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Teardown descends only into a body no one else holds. If it descended
    /// into a shared one it would release values another reference still points
    /// at, which is a use-after-free rather than a crash: nothing observable
    /// goes wrong until much later, and no test of depth would catch it.
    #[test]
    fn releasing_one_holder_leaves_a_shared_body_intact() {
        let shared = Rc::new(ArrayBody::new(vec![Value::Int(1), Value::Int(2)]));
        let first = Value::Array(Rc::clone(&shared));
        let second = Value::Array(Rc::clone(&shared));
        assert_eq!(Rc::strong_count(&shared), 3);

        drop(first);
        assert_eq!(Rc::strong_count(&shared), 2);
        assert_eq!(shared.borrow().len(), 2, "the contents survived");

        drop(second);
        assert_eq!(Rc::strong_count(&shared), 1);
        assert_eq!(shared.borrow().len(), 2, "and survive while we hold it");
    }

    /// The last holder does release the contents. A teardown that never
    /// descends would pass the test above and leak everything.
    #[test]
    fn releasing_the_last_holder_releases_the_contents() {
        let inner = Rc::new(ArrayBody::new(vec![Value::Int(7)]));
        let outer = Value::array(vec![Value::Array(Rc::clone(&inner))]);
        assert_eq!(Rc::strong_count(&inner), 2);

        drop(outer);
        assert_eq!(
            Rc::strong_count(&inner),
            1,
            "the outer array gave up its reference to the inner one"
        );
    }

    /// A chain longer than the stack, built and released in Rust rather than
    /// through a program, so the failure is attributed here rather than to the
    /// interpreter. This overflowed before teardown became iterative.
    #[test]
    fn a_chain_longer_than_the_stack_is_released() {
        let mut chain = Value::array(Vec::new());
        for _ in 0..200_000 {
            chain = Value::array(vec![chain]);
        }
        drop(chain);
    }

    #[test]
    fn an_error_value_names_its_type_and_shows_what_it_holds() {
        let value = Value::Error(Rc::new(crate::MiruError::with_column(
            3,
            5,
            "division by zero",
        )));
        // `type` is the check a program makes, and no other value answers this.
        assert_eq!(value.type_name(), "error");
        // Angle brackets like a function, because this is a thing being held
        // rather than text the program produced.
        assert_eq!(value.repr(), "<error: division by zero>");
        assert_eq!(value.display(), "<error: division by zero>");
        // The error is carried whole, not taken apart, so re-raising one can
        // report the position it originally had.
        match &value {
            Value::Error(error) => {
                assert_eq!((error.line, error.column), (3, 5));
            }
            other => panic!("expected an error, found {}", other.type_name()),
        }
    }

    #[test]
    fn a_control_character_is_spelled_rather_than_written() {
        // Written raw before 1.4, which put a byte into whatever read this
        // that no editor shows and a copy and paste loses.
        let bell = Value::Str(Rc::new("\u{7}".to_string()));
        assert_eq!(bell.repr(), "\"\\u{7}\"");

        // The five with a short spelling keep it.
        assert_eq!(
            Value::Str(Rc::new("a\nb\tc\rd\0e\"f".to_string())).repr(),
            "\"a\\nb\\tc\\rd\\0e\\\"f\""
        );

        // `is_control` is the Unicode category rather than a range written out
        // by hand, so the C1 block counts as well as the C0 block and DEL.
        assert_eq!(
            Value::Str(Rc::new("\u{1B}\u{7F}\u{85}".to_string())).repr(),
            "\"\\u{1B}\\u{7F}\\u{85}\""
        );

        // A key goes through the same function and answers the same way.
        assert_eq!(map(&[("\u{7}", Value::Int(1))]).repr(), "{\"\\u{7}\": 1}");

        // `print` is unchanged. That is the program's own output rather than
        // text about a value, and a program that means to ring a bell rings it.
        assert_eq!(bell.display(), "\u{7}");
    }

    #[test]
    fn the_boundary_of_what_is_spelled_is_the_control_category() {
        // Each pair is a character inside the category beside its neighbour
        // outside it. Without a boundary from the accepted side, spelling
        // every character above ASCII would satisfy the test before this one
        // and would ruin every emoji.
        for (control, ordinary) in [
            ('\u{1F}', '\u{20}'),
            ('\u{7F}', '\u{7E}'),
            ('\u{9F}', '\u{A0}'),
        ] {
            assert!(control.is_control(), "{control:?} should be a control");
            assert!(!ordinary.is_control(), "{ordinary:?} should not be");
            let value = Value::Str(Rc::new(format!("{control}{ordinary}")));
            assert_eq!(
                value.repr(),
                format!("\"\\u{{{:X}}}{ordinary}\"", control as u32)
            );
        }
    }

    #[test]
    fn a_character_that_is_not_ascii_prints_as_itself() {
        // A string inside an array is shown with quotation marks and escapes,
        // and the escapes are what `quoted_string` writes. An emoji is not one
        // of them and comes out whole, which is what `print` already does with
        // the same string on its own.
        let emoji = Value::Str(Rc::new("\u{1F600}".to_string()));
        assert_eq!(emoji.display(), "\u{1F600}");
        assert_eq!(emoji.repr(), "\"\u{1F600}\"");
        assert_eq!(Value::array(vec![emoji.clone()]).repr(), "[\"\u{1F600}\"]");
        // A key is escaped by the same function, so it answers the same way.
        assert_eq!(
            map(&[("\u{1F600}", Value::Int(1))]).repr(),
            "{\"\u{1F600}\": 1}"
        );
    }

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut entries = BTreeMap::new();
        for (key, value) in pairs {
            entries.insert((*key).to_string(), value.clone());
        }
        Value::map(entries)
    }

    #[test]
    fn map_repr_is_sorted_and_quoted() {
        let m = map(&[
            ("name", Value::Str(Rc::new("Aiko".to_string()))),
            ("age", Value::Int(3)),
        ]);
        assert_eq!(m.repr(), "{\"age\": 3, \"name\": \"Aiko\"}");
    }

    #[test]
    fn map_type_name_and_truthiness() {
        let m = map(&[]);
        assert_eq!(m.type_name(), "map");
        assert!(m.is_truthy());
    }

    #[test]
    fn maps_compare_by_entries() {
        let a = map(&[("x", Value::Int(1))]);
        let b = map(&[("x", Value::Int(1))]);
        let c = map(&[("x", Value::Int(2))]);
        assert!(a.equals(&b).unwrap());
        assert!(!a.equals(&c).unwrap());
    }
}
