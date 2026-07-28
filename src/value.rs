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
    Array(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<BTreeMap<String, Value>>>),
    Closure(Rc<Closure>),
    Builtin(Builtin),
    HostBuiltin(HostBuiltin),
    Nil,
    /// A failure that `try` caught, carried as a value.
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
    pub fn repr(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Bool(b) => b.to_string(),
            Value::Nil => "nil".to_string(),
            Value::Str(s) => format!("\"{}\"", escape_string(s)),
            Value::Array(items) => {
                let parts: Vec<String> = items.borrow().iter().map(Value::repr).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Map(entries) => {
                let parts: Vec<String> = entries
                    .borrow()
                    .iter()
                    .map(|(key, value)| format!("\"{}\": {}", escape_string(key), value.repr()))
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

    /// Structural value equality, with numeric promotion so `1 == 1.0`.
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Array(a), Value::Array(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Closure(a), Value::Closure(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a.name == b.name,
            (Value::HostBuiltin(a), Value::HostBuiltin(b)) => a.name == b.name,
            (Value::Map(a), Value::Map(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len()
                    && a.iter().all(|(key, value)| match b.get(key) {
                        Some(other) => value.equals(other),
                        None => false,
                    })
            }
            _ => false,
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
    fn an_error_value_names_its_type_and_shows_the_failure_it_holds() {
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
        // The failure is carried whole, not taken apart, so re-raising one can
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
        Value::Map(Rc::new(RefCell::new(entries)))
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
        assert!(a.equals(&b));
        assert!(!a.equals(&c));
    }
}
