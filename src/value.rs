//! Runtime values, plus the output sink that builtins write to.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Stmt;
use crate::environment::Env;

/// A sink that side-effecting builtins such as `print` write to. The
/// interpreter implements this, so the very same builtins can target real
/// stdout in the binary or an in-memory buffer in tests.
pub trait Output {
    fn write(&mut self, text: &str);
}

/// The shared signature of every native (Rust-implemented) builtin.
pub type BuiltinFn = fn(&mut dyn Output, Vec<Value>) -> Result<Value, String>;

/// A user-defined function together with the environment it closed over. The
/// captured `closure` is what makes closures and recursion work.
pub struct Function {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub closure: Env,
}

/// A native function implemented in Rust and exposed to programs.
#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub func: BuiltinFn,
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
    Function(Rc<Function>),
    Builtin(Builtin),
    Nil,
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
            Value::Function(_) | Value::Builtin(_) => "function",
            Value::Nil => "nil",
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
            Value::Function(func) => match &func.name {
                Some(name) => format!("<fn {name}>"),
                None => "<fn>".to_string(),
            },
            Value::Builtin(builtin) => format!("<builtin {}>", builtin.name),
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
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a.name == b.name,
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
