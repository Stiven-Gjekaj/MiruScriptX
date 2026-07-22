//! Native builtins that every program can call.
//!
//! Each builtin matches [`crate::value::BuiltinFn`]. Errors are returned as
//! plain strings; the interpreter attaches the current source line before
//! surfacing them.

use std::cell::RefCell;
use std::rc::Rc;

use crate::environment::{self, Env};
use crate::value::{Builtin, BuiltinFn, Output, Value};

/// Register every builtin into the given (global) scope.
pub fn register(env: &Env) {
    define(env, "print", print);
    define(env, "len", len);
    define(env, "push", push);
    define(env, "str", to_str);
    define(env, "type", type_of);
    define(env, "range", range);
}

fn define(env: &Env, name: &'static str, func: BuiltinFn) {
    environment::define(env, name, Value::Builtin(Builtin { name, func }));
}

fn check_arity(name: &str, args: &[Value], expected: usize) -> Result<(), String> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "{name} expects {expected} argument(s) but got {}",
            args.len()
        ))
    }
}

/// `print(...)` writes its arguments separated by spaces, then a newline.
fn print(out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    let parts: Vec<String> = args.iter().map(Value::display).collect();
    out.write(&parts.join(" "));
    out.write("\n");
    Ok(Value::Nil)
}

/// `len(value)` returns the length of a string or array.
fn len(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("len", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Array(items) => Ok(Value::Int(items.borrow().len() as i64)),
        other => Err(format!(
            "len expects a string or array but got a {}",
            other.type_name()
        )),
    }
}

/// `push(array, value)` appends to an array in place and returns the array.
fn push(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("push", &args, 2)?;
    match &args[0] {
        Value::Array(items) => {
            items.borrow_mut().push(args[1].clone());
            Ok(args[0].clone())
        }
        other => Err(format!(
            "push expects an array as its first argument but got a {}",
            other.type_name()
        )),
    }
}

/// `str(value)` converts any value to its display string.
fn to_str(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("str", &args, 1)?;
    Ok(Value::Str(Rc::new(args[0].display())))
}

/// `type(value)` returns the name of a value's type.
fn type_of(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("type", &args, 1)?;
    Ok(Value::Str(Rc::new(args[0].type_name().to_string())))
}

/// `range(end)` or `range(start, end)` returns an array of integers in the
/// half-open interval, so the end value is not included.
fn range(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    let (start, end) = match args.as_slice() {
        [Value::Int(end)] => (0i64, *end),
        [Value::Int(start), Value::Int(end)] => (*start, *end),
        [_] | [_, _] => return Err("range expects integer arguments".to_string()),
        _ => {
            return Err(format!(
                "range expects 1 or 2 arguments but got {}",
                args.len()
            ))
        }
    };
    let mut items = Vec::new();
    let mut current = start;
    while current < end {
        items.push(Value::Int(current));
        current += 1;
    }
    Ok(Value::Array(Rc::new(RefCell::new(items))))
}
