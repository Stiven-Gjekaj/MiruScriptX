//! Runtime values, plus the output sink that builtins write to.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::chunk::Chunk;

/// A sink that side-effecting builtins such as `print` write to. The virtual
/// machine implements this, so the very same builtins can target real stdout
/// in the binary or an in-memory buffer in tests.
pub trait Output {
    fn write(&mut self, text: &str);
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

/// A native function implemented in Rust and exposed to programs.
#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub func: BuiltinFn,
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
            Value::Str(s) => format!("\"{}\"", escape_string(s)),
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
                        format!(
                            "\"{}\": {}",
                            escape_string(key),
                            value.repr_within(depth, open)
                        )
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

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
