//! Native builtins that every program can call.
//!
//! Each builtin matches [`crate::value::BuiltinFn`]. Errors are returned as
//! plain strings; the interpreter attaches the current source line before
//! surfacing them.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use crate::environment::{self, Env};
use crate::value::{Builtin, BuiltinFn, Input, Output, Value};

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
    define(env, "pop", pop);
    define(env, "index_of", index_of);
    define(env, "slice", slice);
    define(env, "sort", sort);
    define(env, "reverse", reverse);
    define(env, "abs", abs);
    define(env, "min", min);
    define(env, "max", max);
    define(env, "floor", floor);
    define(env, "ceil", ceil);
    define(env, "round", round);
    define(env, "sqrt", sqrt);
    define(env, "pow", pow);
    define(env, "int", int);
    define(env, "float", float);
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
fn print(out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    let parts: Vec<String> = args.iter().map(Value::display).collect();
    out.write(&parts.join(" "));
    out.write("\n");
    Ok(Value::Nil)
}

/// `len(value)` returns the length of a string, array, or map.
fn len(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn push(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn to_str(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("str", &args, 1)?;
    Ok(Value::Str(Rc::new(args[0].display())))
}

/// `type(value)` returns the name of a value's type.
fn type_of(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("type", &args, 1)?;
    Ok(Value::Str(Rc::new(args[0].type_name().to_string())))
}

/// `range(end)` or `range(start, end)` returns an array of integers in the
/// half-open interval, so the end value is not included.
fn range(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn keys(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn values(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
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
fn has(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn upper(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn lower(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn trim(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn replace(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
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
fn split(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn join(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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
fn contains(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
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
fn find(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
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

/// `pop(array)` removes and returns the last element; an empty array is an error.
fn pop(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("pop", &args, 1)?;
    match &args[0] {
        Value::Array(items) => items
            .borrow_mut()
            .pop()
            .ok_or_else(|| "pop from an empty array".to_string()),
        other => Err(format!(
            "pop expects an array but got a {}",
            other.type_name()
        )),
    }
}

/// `index_of(array, value)` returns the index of the first element equal to
/// `value`, or -1 when there is none.
fn index_of(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("index_of", &args, 2)?;
    match &args[0] {
        Value::Array(items) => {
            let index = items
                .borrow()
                .iter()
                .position(|item| item.equals(&args[1]))
                .map(|p| p as i64)
                .unwrap_or(-1);
            Ok(Value::Int(index))
        }
        other => Err(format!(
            "index_of expects an array but got a {}",
            other.type_name()
        )),
    }
}

/// Clamp a half-open `[start, end)` range to `0..=len`, keeping `end >= start`.
fn clamp_range(start: i64, end: i64, len: usize) -> (usize, usize) {
    let len = len as i64;
    let lo = start.clamp(0, len) as usize;
    let hi = end.clamp(0, len) as usize;
    (lo, hi.max(lo))
}

/// `slice(seq, start, end)` returns the half-open `[start, end)` slice of an
/// array or string. Indices are character based for strings and are clamped to
/// the sequence bounds.
fn slice(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("slice", &args, 3)?;
    let start = match &args[1] {
        Value::Int(n) => *n,
        other => {
            return Err(format!(
                "slice expects an integer start but got a {}",
                other.type_name()
            ))
        }
    };
    let end = match &args[2] {
        Value::Int(n) => *n,
        other => {
            return Err(format!(
                "slice expects an integer end but got a {}",
                other.type_name()
            ))
        }
    };
    match &args[0] {
        Value::Array(items) => {
            let items = items.borrow();
            let (lo, hi) = clamp_range(start, end, items.len());
            Ok(Value::Array(Rc::new(RefCell::new(items[lo..hi].to_vec()))))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let (lo, hi) = clamp_range(start, end, chars.len());
            Ok(Value::Str(Rc::new(chars[lo..hi].iter().collect())))
        }
        other => Err(format!(
            "slice expects an array or string but got a {}",
            other.type_name()
        )),
    }
}

/// The numeric value of an int or float, for ordering. Non-numbers map to 0.
fn number_as_f64(value: &Value) -> f64 {
    match value {
        Value::Int(n) => *n as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

/// `sort(array)` returns a sorted copy. The array must hold all numbers or all
/// strings; anything else is an error.
fn sort(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("sort", &args, 1)?;
    let mut sorted = match &args[0] {
        Value::Array(items) => items.borrow().clone(),
        other => {
            return Err(format!(
                "sort expects an array but got a {}",
                other.type_name()
            ))
        }
    };
    if sorted.iter().all(|v| matches!(v, Value::Int(_))) {
        sorted.sort_by_key(|v| match v {
            Value::Int(n) => *n,
            _ => 0,
        });
    } else if sorted.iter().all(|v| matches!(v, Value::Str(_))) {
        sorted.sort_by(|a, b| match (a, b) {
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            _ => Ordering::Equal,
        });
    } else if sorted
        .iter()
        .all(|v| matches!(v, Value::Int(_) | Value::Float(_)))
    {
        if sorted
            .iter()
            .any(|v| matches!(v, Value::Float(f) if f.is_nan()))
        {
            return Err("sort cannot order NaN".to_string());
        }
        sorted.sort_by(|a, b| {
            number_as_f64(a)
                .partial_cmp(&number_as_f64(b))
                .unwrap_or(Ordering::Equal)
        });
    } else {
        return Err("sort expects an array of all numbers or all strings".to_string());
    }
    Ok(Value::Array(Rc::new(RefCell::new(sorted))))
}

/// `reverse(seq)` returns a reversed copy of an array or string.
fn reverse(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("reverse", &args, 1)?;
    match &args[0] {
        Value::Array(items) => {
            let mut copy = items.borrow().clone();
            copy.reverse();
            Ok(Value::Array(Rc::new(RefCell::new(copy))))
        }
        Value::Str(s) => Ok(Value::Str(Rc::new(s.chars().rev().collect()))),
        other => Err(format!(
            "reverse expects an array or string but got a {}",
            other.type_name()
        )),
    }
}

/// `abs(x)` returns the absolute value of an int or float.
fn abs(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("abs", &args, 1)?;
    match &args[0] {
        Value::Int(n) => n
            .checked_abs()
            .map(Value::Int)
            .ok_or_else(|| "integer overflow in abs".to_string()),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(format!(
            "abs expects a number but got a {}",
            other.type_name()
        )),
    }
}

/// Shared implementation of `min` and `max`: pick the argument that compares as
/// `want` against the running best. Preserves the winning value's own type.
fn extreme(name: &str, args: Vec<Value>, want: Ordering) -> Result<Value, String> {
    if args.is_empty() {
        return Err(format!("{name} expects at least one argument"));
    }
    for value in &args {
        if !matches!(value, Value::Int(_) | Value::Float(_)) {
            return Err(format!(
                "{name} expects numbers but got a {}",
                value.type_name()
            ));
        }
        if matches!(value, Value::Float(f) if f.is_nan()) {
            return Err(format!("{name} cannot compare NaN"));
        }
    }
    let mut best = args[0].clone();
    for value in &args[1..] {
        let ordering = number_as_f64(value)
            .partial_cmp(&number_as_f64(&best))
            .unwrap_or(Ordering::Equal);
        if ordering == want {
            best = value.clone();
        }
    }
    Ok(best)
}

/// `min(...)` returns the smallest of its numeric arguments.
fn min(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    extreme("min", args, Ordering::Less)
}

/// `max(...)` returns the largest of its numeric arguments.
fn max(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    extreme("max", args, Ordering::Greater)
}

/// Shared implementation of `floor`, `ceil`, and `round`. Ints pass through
/// unchanged; a float is rounded by `apply` and returned as an int.
fn round_like(name: &str, args: Vec<Value>, apply: fn(f64) -> f64) -> Result<Value, String> {
    check_arity(name, &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(x) => {
            let rounded = apply(*x);
            if rounded.is_finite() {
                Ok(Value::Int(rounded as i64))
            } else {
                Err(format!("{name} of a non-finite number"))
            }
        }
        other => Err(format!(
            "{name} expects a number but got a {}",
            other.type_name()
        )),
    }
}

/// `floor(x)` returns the largest integer not greater than `x`.
fn floor(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    round_like("floor", args, f64::floor)
}

/// `ceil(x)` returns the smallest integer not less than `x`.
fn ceil(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    round_like("ceil", args, f64::ceil)
}

/// `round(x)` returns `x` rounded to the nearest integer, halves away from zero.
fn round(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    round_like("round", args, f64::round)
}

/// The f64 value of a numeric argument, or an error naming the builtin.
fn number_arg(name: &str, value: &Value) -> Result<f64, String> {
    match value {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(format!(
            "{name} expects numbers but got a {}",
            other.type_name()
        )),
    }
}

/// `sqrt(x)` returns the square root of a non-negative number, as a float.
fn sqrt(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("sqrt", &args, 1)?;
    let x = number_arg("sqrt", &args[0])?;
    if x < 0.0 {
        return Err("sqrt of a negative number".to_string());
    }
    Ok(Value::Float(x.sqrt()))
}

/// `pow(base, exp)` returns `base` raised to `exp`. With two integers and a
/// non-negative exponent the result is an integer; otherwise it is a float.
fn pow(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("pow", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Int(base), Value::Int(exp)) if *exp >= 0 => {
            let exp = u32::try_from(*exp).map_err(|_| "pow exponent is too large".to_string())?;
            base.checked_pow(exp)
                .map(Value::Int)
                .ok_or_else(|| "integer overflow in pow".to_string())
        }
        _ => {
            let base = number_arg("pow", &args[0])?;
            let exp = number_arg("pow", &args[1])?;
            Ok(Value::Float(base.powf(exp)))
        }
    }
}

/// `int(x)` converts a float (truncating toward zero) or a numeric string to an
/// integer; an integer is returned unchanged.
fn int(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("int", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => {
            if f.is_finite() {
                Ok(Value::Int(*f as i64))
            } else {
                Err("int of a non-finite number".to_string())
            }
        }
        Value::Str(s) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("cannot convert \"{s}\" to an int")),
        other => Err(format!("int cannot convert a {}", other.type_name())),
    }
}

/// `float(x)` converts an integer or a numeric string to a float; a float is
/// returned unchanged.
fn float(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("float", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Str(s) => s
            .trim()
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("cannot convert \"{s}\" to a float")),
        other => Err(format!("float cannot convert a {}", other.type_name())),
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

    #[test]
    fn pop_removes_and_returns_last() {
        assert_eq!(out("let a = [1, 2, 3]\nprint(pop(a), a)"), "3 [1, 2]\n");
    }

    #[test]
    fn pop_on_empty_array_errors() {
        assert!(err("pop([])").contains("empty array"));
    }

    #[test]
    fn index_of_finds_first_match_or_negative_one() {
        assert_eq!(
            out("print(index_of([10, 20, 30], 20), index_of([1], 9))"),
            "1 -1\n"
        );
    }

    #[test]
    fn slice_on_arrays_and_strings() {
        assert_eq!(out("print(slice([1, 2, 3, 4], 1, 3))"), "[2, 3]\n");
        assert_eq!(out("print(slice(\"hello\", 1, 4))"), "ell\n");
    }

    #[test]
    fn slice_clamps_out_of_range_bounds() {
        assert_eq!(
            out("print(slice([1, 2], 0, 99), slice([1, 2], 5, 9))"),
            "[1, 2] []\n"
        );
    }

    #[test]
    fn sort_orders_numbers_and_strings() {
        assert_eq!(out("print(sort([3, 1, 2]))"), "[1, 2, 3]\n");
        assert_eq!(
            out("print(sort([\"c\", \"a\", \"b\"]))"),
            "[\"a\", \"b\", \"c\"]\n"
        );
    }

    #[test]
    fn sort_rejects_mixed_types() {
        assert!(err("sort([1, \"a\"])").contains("all numbers or all strings"));
    }

    #[test]
    fn reverse_flips_arrays_and_strings() {
        assert_eq!(
            out("print(reverse([1, 2, 3]), reverse(\"abc\"))"),
            "[3, 2, 1] cba\n"
        );
    }

    #[test]
    fn abs_of_ints_and_floats() {
        assert_eq!(out("print(abs(-3), abs(-2.5), abs(4))"), "3 2.5 4\n");
    }

    #[test]
    fn min_and_max_over_numbers() {
        assert_eq!(out("print(min(3, 1, 2), max(3, 1, 2))"), "1 3\n");
        assert_eq!(out("print(min(2, 1.5), max(2, 1.5))"), "1.5 2\n");
    }

    #[test]
    fn min_requires_arguments() {
        assert!(err("min()").contains("at least one"));
    }

    #[test]
    fn floor_ceil_round_return_ints() {
        assert_eq!(
            out("print(floor(2.7), ceil(2.1), round(2.5), round(2.4))"),
            "2 3 3 2\n"
        );
    }

    #[test]
    fn rounding_passes_ints_through() {
        assert_eq!(out("print(floor(5), ceil(5), round(5))"), "5 5 5\n");
    }

    #[test]
    fn sqrt_returns_a_float() {
        assert_eq!(out("print(sqrt(9), sqrt(16.0))"), "3.0 4.0\n");
    }

    #[test]
    fn sqrt_rejects_negatives() {
        assert!(err("sqrt(-1)").contains("negative"));
    }

    #[test]
    fn pow_is_int_for_int_base_and_nonneg_exp() {
        assert_eq!(
            out("print(pow(2, 10), pow(2, -1), pow(2.0, 3))"),
            "1024 0.5 8.0\n"
        );
    }

    #[test]
    fn int_converts_floats_and_strings() {
        assert_eq!(out("print(int(2.9), int(\"42\"), int(7))"), "2 42 7\n");
    }

    #[test]
    fn float_converts_ints_and_strings() {
        assert_eq!(out("print(float(3), float(\"1.5\"))"), "3.0 1.5\n");
    }

    #[test]
    fn int_rejects_unparseable_strings() {
        assert!(err("int(\"abc\")").contains("cannot convert"));
    }
}
