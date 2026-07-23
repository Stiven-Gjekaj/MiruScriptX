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
    define(env, "keys", keys);
    define(env, "values", values);
    define(env, "has", has);
    define(env, "upper", upper);
    define(env, "lower", lower);
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

/// `len(value)` returns the length of a string, array, or map.
fn len(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("len", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Array(items) => Ok(Value::Int(items.borrow().len() as i64)),
        Value::Map(entries) => Ok(Value::Int(entries.borrow().len() as i64)),
        other => Err(format!(
            "len expects a string, array, or map but got a {}",
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

/// `keys(map)` returns an array of the map's keys, in sorted order.
fn keys(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("keys", &args, 1)?;
    match &args[0] {
        Value::Map(entries) => {
            let items: Vec<Value> = entries
                .borrow()
                .keys()
                .map(|key| Value::Str(Rc::new(key.clone())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(items))))
        }
        other => Err(format!(
            "keys expects a map but got a {}",
            other.type_name()
        )),
    }
}

/// `values(map)` returns an array of the map's values, in key order.
fn values(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("values", &args, 1)?;
    match &args[0] {
        Value::Map(entries) => {
            let items: Vec<Value> = entries.borrow().values().cloned().collect();
            Ok(Value::Array(Rc::new(RefCell::new(items))))
        }
        other => Err(format!(
            "values expects a map but got a {}",
            other.type_name()
        )),
    }
}

/// `has(map, key)` reports whether the map contains the given string key.
fn has(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("has", &args, 2)?;
    let key = match &args[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return Err(format!(
                "has expects a string key but got a {}",
                other.type_name()
            ))
        }
    };
    match &args[0] {
        Value::Map(entries) => Ok(Value::Bool(entries.borrow().contains_key(&key))),
        other => Err(format!("has expects a map but got a {}", other.type_name())),
    }
}

/// `upper(s)` returns the string with every letter upper-cased.
fn upper(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("upper", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(Rc::new(s.to_uppercase()))),
        other => Err(format!(
            "upper expects a string but got a {}",
            other.type_name()
        )),
    }
}

/// `lower(s)` returns the string with every letter lower-cased.
fn lower(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("lower", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(Rc::new(s.to_lowercase()))),
        other => Err(format!(
            "lower expects a string but got a {}",
            other.type_name()
        )),
    }
}

#[cfg(test)]
mod tests {
    use crate::run_capture;

    fn out(source: &str) -> String {
        run_capture(source).expect("program should run")
    }

    fn err(source: &str) -> String {
        run_capture(source)
            .expect_err("program should fail")
            .message
    }

    #[test]
    fn upper_and_lower() {
        assert_eq!(out("print(upper(\"aBc\"), lower(\"aBc\"))"), "ABC abc\n");
    }

    #[test]
    fn upper_rejects_non_strings() {
        assert!(err("upper(1)").contains("expects a string"));
    }
}
