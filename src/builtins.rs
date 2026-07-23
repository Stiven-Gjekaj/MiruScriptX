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
    define(env, "trim", trim);
    define(env, "replace", replace);
    define(env, "split", split);
    define(env, "join", join);
    define(env, "contains", contains);
    define(env, "find", find);
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

/// `trim(s)` returns the string with leading and trailing whitespace removed.
fn trim(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("trim", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(Rc::new(s.trim().to_string()))),
        other => Err(format!(
            "trim expects a string but got a {}",
            other.type_name()
        )),
    }
}

/// `replace(s, from, to)` returns `s` with every occurrence of `from` replaced
/// by `to`.
fn replace(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("replace", &args, 3)?;
    match (&args[0], &args[1], &args[2]) {
        (Value::Str(s), Value::Str(from), Value::Str(to)) => {
            Ok(Value::Str(Rc::new(s.replace(from.as_str(), to.as_str()))))
        }
        _ => Err("replace expects three string arguments".to_string()),
    }
}

/// `split(s, sep)` returns an array of the pieces of `s` between each `sep`.
/// An empty separator splits the string into its individual characters.
fn split(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("split", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(sep)) => {
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars()
                    .map(|c| Value::Str(Rc::new(c.to_string())))
                    .collect()
            } else {
                s.split(sep.as_str())
                    .map(|part| Value::Str(Rc::new(part.to_string())))
                    .collect()
            };
            Ok(Value::Array(Rc::new(RefCell::new(parts))))
        }
        _ => Err("split expects two string arguments".to_string()),
    }
}

/// `join(array, sep)` returns the array's elements, displayed and joined by
/// `sep` into a single string.
fn join(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("join", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Array(items), Value::Str(sep)) => {
            let parts: Vec<String> = items.borrow().iter().map(Value::display).collect();
            Ok(Value::Str(Rc::new(parts.join(sep.as_str()))))
        }
        _ => Err("join expects an array and a string separator".to_string()),
    }
}

/// `contains(seq, value)` reports whether a string contains a substring or an
/// array contains an element equal to `value`.
fn contains(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("contains", &args, 2)?;
    match &args[0] {
        Value::Str(s) => match &args[1] {
            Value::Str(sub) => Ok(Value::Bool(s.contains(sub.as_str()))),
            other => Err(format!(
                "contains on a string expects a string but got a {}",
                other.type_name()
            )),
        },
        Value::Array(items) => Ok(Value::Bool(
            items.borrow().iter().any(|item| item.equals(&args[1])),
        )),
        other => Err(format!(
            "contains expects a string or array but got a {}",
            other.type_name()
        )),
    }
}

/// `find(s, sub)` returns the character index of the first `sub` in `s`, or -1
/// when it is not present.
fn find(_out: &mut dyn Output, args: Vec<Value>) -> Result<Value, String> {
    check_arity("find", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(sub)) => {
            let index = match s.find(sub.as_str()) {
                Some(byte_index) => s[..byte_index].chars().count() as i64,
                None => -1,
            };
            Ok(Value::Int(index))
        }
        _ => Err("find expects two string arguments".to_string()),
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

    #[test]
    fn trim_strips_surrounding_whitespace() {
        assert_eq!(out("print(trim(\"  hi \"))"), "hi\n");
    }

    #[test]
    fn replace_swaps_every_occurrence() {
        assert_eq!(out("print(replace(\"a.b.c\", \".\", \"-\"))"), "a-b-c\n");
    }

    #[test]
    fn split_breaks_on_separator() {
        assert_eq!(
            out("print(split(\"a,b,c\", \",\"))"),
            "[\"a\", \"b\", \"c\"]\n"
        );
    }

    #[test]
    fn split_on_empty_separator_yields_characters() {
        assert_eq!(out("print(split(\"hi\", \"\"))"), "[\"h\", \"i\"]\n");
    }

    #[test]
    fn join_concatenates_with_separator() {
        assert_eq!(out("print(join([1, 2, 3], \"-\"))"), "1-2-3\n");
    }

    #[test]
    fn contains_checks_substrings_and_membership() {
        assert_eq!(
            out("print(contains(\"hello\", \"ell\"), contains([1, 2], 2), contains([1, 2], 9))"),
            "true true false\n"
        );
    }

    #[test]
    fn find_returns_char_index_or_negative_one() {
        assert_eq!(
            out("print(find(\"hello\", \"l\"), find(\"hello\", \"z\"))"),
            "2 -1\n"
        );
    }
}
