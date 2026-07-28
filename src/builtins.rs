//! Native builtins that every program can call.
//!
//! Each builtin matches [`crate::value::BuiltinFn`]. Errors are returned as
//! plain strings; the virtual machine attaches the source line and column of
//! the call before surfacing them.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use crate::globals::Globals;
use crate::value::{Builtin, BuiltinFn, HostBuiltin, HostFn, Input, Output, Value};

/// Register every builtin into a program's globals.
pub fn register(globals: &mut Globals) {
    define(globals, "print", print);
    define(globals, "len", len);
    define(globals, "push", push);
    define(globals, "str", to_str);
    define(globals, "type", type_of);
    define(globals, "is_error", is_error);
    define(globals, "range", range);
    define(globals, "keys", keys);
    define(globals, "values", values);
    define(globals, "has", has);
    define(globals, "upper", upper);
    define(globals, "lower", lower);
    define(globals, "trim", trim);
    define(globals, "replace", replace);
    define(globals, "split", split);
    define(globals, "join", join);
    define(globals, "contains", contains);
    define(globals, "find", find);
    define(globals, "pop", pop);
    define(globals, "index_of", index_of);
    define(globals, "slice", slice);
    define(globals, "sort", sort);
    define(globals, "reverse", reverse);
    define(globals, "abs", abs);
    define(globals, "min", min);
    define(globals, "max", max);
    define(globals, "floor", floor);
    define(globals, "ceil", ceil);
    define(globals, "round", round);
    define(globals, "sqrt", sqrt);
    define(globals, "pow", pow);
    define(globals, "int", int);
    define(globals, "float", float);
    define(globals, "input", input);
    define_host(globals, "map", map);
    define_host(globals, "filter", filter);
    define_host(globals, "reduce", reduce);
}

/// Whether a builtin may be handed a caught failure.
///
/// Every other builtin refuses one, because passing a failure on as though it
/// were a result is the mistake this milestone exists to prevent. Asking what
/// type a value is has to be the exception: it is how a program finds out that
/// it is holding a failure in the first place, and a check that cannot be made
/// without tripping the guard is no check at all.
pub fn accepts_failure(name: &str) -> bool {
    matches!(name, "type" | "is_error")
}

fn define(globals: &mut Globals, name: &'static str, func: BuiltinFn) {
    let slot = globals
        .slot_for_builtin(name)
        .expect("room for the builtins");
    globals.define(slot, Value::Builtin(Builtin { name, func }));
}

/// Register a higher-order builtin, one that receives the running engine so it
/// can apply a function argument.
fn define_host(globals: &mut Globals, name: &'static str, func: HostFn) {
    let slot = globals
        .slot_for_builtin(name)
        .expect("room for the builtins");
    globals.define(slot, Value::HostBuiltin(HostBuiltin { name, func }));
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

/// Whether a value is a failure caught by `try`.
///
/// `type(v) == "error"` says the same thing and needed no builtin, but it puts
/// the check that decides whether a program is about to misuse a value behind a
/// string comparison a typo can silently break. This cannot be misspelled
/// without failing outright.
fn is_error(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("is_error", &args, 1)?;
    Ok(Value::Bool(matches!(args[0], Value::Error(_))))
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

/// `input()` reads one line from the input source and returns it as a string,
/// or `nil` at end of input. `input(prompt)` writes the prompt string first.
fn input(out: &mut dyn Output, input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    match args.as_slice() {
        [] => {}
        [Value::Str(prompt)] => out.write(prompt.as_str()),
        [other] => {
            return Err(format!(
                "input expects a string prompt but got a {}",
                other.type_name()
            ))
        }
        _ => {
            return Err(format!(
                "input expects 0 or 1 arguments but got {}",
                args.len()
            ))
        }
    }
    match input.read_line() {
        Some(line) => Ok(Value::Str(Rc::new(line))),
        None => Ok(Value::Nil),
    }
}

/// The arguments a suspended higher-order builtin wants applied, held inline
/// rather than in a `Vec`.
///
/// This is built once per element, so a `Vec` here would put back the heap
/// allocation the trampoline exists to remove: one was measured at 10.8 ns
/// against a per-element budget of 74 ns. The three higher-order builtins pass
/// at most two arguments; a builtin wanting three would add a variant.
pub enum Args {
    One(Value),
    Two(Value, Value),
}

impl Args {
    /// How many arguments there are, for the callee's arity check. Named
    /// `count` rather than `len` because a `len` invites an `is_empty` beside
    /// it, and these are never empty.
    pub fn count(&self) -> usize {
        match self {
            Args::One(_) => 1,
            Args::Two(_, _) => 2,
        }
    }

    /// Push the arguments in order, leaving the stack as a bytecode call would.
    pub fn push_onto(self, stack: &mut Vec<Value>) {
        match self {
            Args::One(a) => stack.push(a),
            Args::Two(a, b) => {
                stack.push(a);
                stack.push(b);
            }
        }
    }

    /// Collect into a `Vec` for an ordinary native builtin, whose signature
    /// takes one.
    ///
    /// This is the last per-element heap allocation left in the engine, and it
    /// only happens when a *builtin* is the callback, as in `map(xs, abs)`.
    /// Removing it means changing [`BuiltinFn`] for all thirty-seven builtins,
    /// several of which move values out of the vector they are handed, which is
    /// a larger change than it looks and is left for later.
    pub fn into_vec(self) -> Vec<Value> {
        match self {
            Args::One(a) => vec![a],
            Args::Two(a, b) => vec![a, b],
        }
    }
}

/// What a suspended higher-order builtin wants next.
///
/// The values are owned rather than borrowed so that advancing a task does not
/// hold a borrow of the engine. The task lives on the engine's own task stack,
/// and the engine has to touch its value stack to carry the step out, so a
/// [`Step`] that borrowed from the task would make those two borrows overlap.
pub enum Step {
    /// Apply the task's own callback to these arguments, then resume it with
    /// the result.
    ///
    /// The callee is not carried here. It never varies across a task's steps,
    /// so repeating it per element only made this enum bigger, and this value
    /// is built and moved once per element. Naming it cost 32 of the 96 bytes
    /// that move each time; the engine reads it from the task instead.
    Call(Args),
    /// The builtin is finished; this is its value.
    Done(Value),
}

/// A higher-order builtin part way through its work.
///
/// `map`, `filter`, and `reduce` are state machines rather than loops. A loop
/// would have to call back into the engine per element, which costs a nested
/// bytecode loop and a real Rust call each time; instead each step says what to
/// call and the single dispatch loop does the calling.
///
/// Every variant walks `items` by index and moves each element out with
/// [`std::mem::replace`] rather than cloning it. That is not a flourish: it
/// keeps the clone count per element the same as the loops these replaced, and
/// `reduce` in particular would otherwise clone its accumulator on every step.
pub enum HostTask {
    Map {
        func: Value,
        items: Vec<Value>,
        next: usize,
        out: Vec<Value>,
    },
    Filter {
        func: Value,
        items: Vec<Value>,
        next: usize,
        out: Vec<Value>,
        /// The element the outstanding call is deciding on, kept so a truthy
        /// answer can move it into `out` rather than clone it again.
        pending: Option<Value>,
    },
    Reduce {
        func: Value,
        items: Vec<Value>,
        next: usize,
        acc: Value,
    },
}

impl HostTask {
    /// The function this task applies. It is the same for every step, which is
    /// why [`Step::Call`] does not repeat it.
    pub fn callback(&self) -> &Value {
        match self {
            HostTask::Map { func, .. }
            | HostTask::Filter { func, .. }
            | HostTask::Reduce { func, .. } => func,
        }
    }

    /// Advance the builtin one step. `last` is the value the previously
    /// requested call returned, or `None` on the first step.
    pub fn resume(&mut self, last: Option<Value>) -> Step {
        match self {
            HostTask::Map {
                items, next, out, ..
            } => {
                if let Some(value) = last {
                    out.push(value);
                }
                match take_next(items, next) {
                    None => Step::Done(array(std::mem::take(out))),
                    Some(item) => Step::Call(Args::One(item)),
                }
            }
            HostTask::Filter {
                items,
                next,
                out,
                pending,
                ..
            } => {
                // `pending` holds the element the answer is about. Keeping it
                // means a kept element is moved into the result rather than
                // cloned a second time.
                if let Some(value) = last {
                    let item = pending.take().expect("an element under decision");
                    if value.is_truthy() {
                        out.push(item);
                    }
                }
                match take_next(items, next) {
                    None => Step::Done(array(std::mem::take(out))),
                    Some(item) => {
                        *pending = Some(item.clone());
                        Step::Call(Args::One(item))
                    }
                }
            }
            HostTask::Reduce {
                items, next, acc, ..
            } => {
                if let Some(value) = last {
                    *acc = value;
                }
                match take_next(items, next) {
                    None => Step::Done(std::mem::replace(acc, Value::Nil)),
                    Some(item) => {
                        // Move the accumulator out rather than cloning it. The
                        // call's result puts one back on the next resume.
                        let carried = std::mem::replace(acc, Value::Nil);
                        Step::Call(Args::Two(carried, item))
                    }
                }
            }
        }
    }
}

/// Move the element at `next` out of `items` and advance the cursor, or `None`
/// once the walk is done. The vacated slot is left holding `nil`, which nothing
/// reads: the cursor only ever moves forward.
fn take_next(items: &mut [Value], next: &mut usize) -> Option<Value> {
    let slot = items.get_mut(*next)?;
    *next += 1;
    Some(std::mem::replace(slot, Value::Nil))
}

fn array(items: Vec<Value>) -> Value {
    Value::Array(Rc::new(RefCell::new(items)))
}

/// Check the two-argument shape `map` and `filter` share: an array to walk and a
/// function to apply to each of its elements.
///
/// The array is copied here rather than borrowed for the length of the walk.
/// That matches `for x in xs`, which copies through `OpCode::IterSnapshot`, so a
/// callback that pushes to the array being walked does not extend the walk. It
/// also means the task owns its elements and can move them out one at a time
/// instead of cloning them.
fn array_and_function(name: &str, args: Vec<Value>) -> Result<(Vec<Value>, Value), String> {
    if args.len() != 2 {
        return Err(format!(
            "{name} expects 2 argument(s) but got {}",
            args.len()
        ));
    }
    let mut args = args.into_iter();
    let items = match args.next().expect("two arguments") {
        Value::Array(items) => items.borrow().clone(),
        other => {
            return Err(format!(
                "{name} expects an array but got a {}",
                other.type_name()
            ))
        }
    };
    Ok((items, args.next().expect("two arguments")))
}

/// `map(array, f)` returns a new array holding `f(x)` for each element `x`, in
/// order. The function is applied by the engine, so it may be a user-defined
/// function, a closure, or another builtin.
fn map(args: Vec<Value>) -> Result<HostTask, String> {
    let (items, func) = array_and_function("map", args)?;
    Ok(HostTask::Map {
        out: Vec::with_capacity(items.len()),
        func,
        items,
        next: 0,
    })
}

/// `filter(array, f)` returns a new array of the elements for which `f(x)` is
/// truthy, keeping their original order.
fn filter(args: Vec<Value>) -> Result<HostTask, String> {
    let (items, func) = array_and_function("filter", args)?;
    Ok(HostTask::Filter {
        func,
        items,
        next: 0,
        out: Vec::new(),
        pending: None,
    })
}

/// `reduce(array, f, init)` folds the array from the left: starting with the
/// accumulator `init`, it computes `f(acc, x)` for each element in order and
/// returns the final accumulator.
fn reduce(args: Vec<Value>) -> Result<HostTask, String> {
    if args.len() != 3 {
        return Err(format!(
            "reduce expects 3 argument(s) but got {}",
            args.len()
        ));
    }
    let mut args = args.into_iter();
    let items = match args.next().expect("three arguments") {
        Value::Array(items) => items.borrow().clone(),
        other => {
            return Err(format!(
                "reduce expects an array but got a {}",
                other.type_name()
            ))
        }
    };
    Ok(HostTask::Reduce {
        func: args.next().expect("three arguments"),
        items,
        next: 0,
        acc: args.next().expect("three arguments"),
    })
}

#[cfg(test)]
mod task_tests {
    //! The higher-order builtins as state machines, driven by hand.
    //!
    //! No virtual machine here. A task only ever says what it wants called and
    //! is told what came back, so it can be stepped directly, and a fault in
    //! the sequencing shows up as a wrong step rather than as a wrong program
    //! output three layers away.

    use super::{Args, HostTask, Step};
    use crate::value::Value;

    /// The element a step asks for, as a readable string, or a description of
    /// why it was not the single-argument call the caller expected.
    fn asked_for(step: &Step) -> String {
        match step {
            Step::Call(Args::One(v)) => v.repr(),
            Step::Call(Args::Two(a, b)) => format!("two: {} {}", a.repr(), b.repr()),
            Step::Done(v) => format!("done: {}", v.repr()),
        }
    }

    fn ints(values: &[i64]) -> Vec<Value> {
        values.iter().map(|n| Value::Int(*n)).collect()
    }

    /// A callback stands in for the function the engine would apply. The task
    /// never calls it, so any value does.
    fn callback() -> Value {
        Value::Nil
    }

    #[test]
    fn a_map_task_asks_for_each_element_then_yields_the_results() {
        let mut task = HostTask::Map {
            func: callback(),
            items: ints(&[1, 2]),
            next: 0,
            out: Vec::new(),
        };
        assert_eq!(asked_for(&task.resume(None)), "1");
        assert_eq!(asked_for(&task.resume(Some(Value::Int(10)))), "2");
        assert_eq!(
            asked_for(&task.resume(Some(Value::Int(20)))),
            "done: [10, 20]"
        );
    }

    #[test]
    fn a_map_task_over_nothing_is_done_at_once() {
        let mut task = HostTask::Map {
            func: callback(),
            items: Vec::new(),
            next: 0,
            out: Vec::new(),
        };
        assert_eq!(asked_for(&task.resume(None)), "done: []");
    }

    #[test]
    fn a_filter_task_keeps_only_what_the_answer_was_truthy_for() {
        let mut task = HostTask::Filter {
            func: callback(),
            items: ints(&[1, 2, 3]),
            next: 0,
            out: Vec::new(),
            pending: None,
        };
        assert_eq!(asked_for(&task.resume(None)), "1");
        assert_eq!(asked_for(&task.resume(Some(Value::Bool(false)))), "2");
        assert_eq!(asked_for(&task.resume(Some(Value::Bool(true)))), "3");
        // nil is falsy, like false, so the third element is dropped.
        assert_eq!(asked_for(&task.resume(Some(Value::Nil))), "done: [2]");
    }

    #[test]
    fn a_filter_task_yields_the_element_and_not_the_answer() {
        // The distinction that a loop makes for free and a state machine has to
        // be deliberate about: what goes into the result is the element, not
        // whatever the predicate returned about it.
        let mut task = HostTask::Filter {
            func: callback(),
            items: ints(&[7]),
            next: 0,
            out: Vec::new(),
            pending: None,
        };
        assert_eq!(asked_for(&task.resume(None)), "7");
        assert_eq!(asked_for(&task.resume(Some(Value::Int(999)))), "done: [7]");
    }

    #[test]
    fn a_reduce_task_carries_the_accumulator_through() {
        let mut task = HostTask::Reduce {
            func: callback(),
            items: ints(&[1, 2]),
            next: 0,
            acc: Value::Int(0),
        };
        // Each call gets the accumulator first and the element second.
        assert_eq!(asked_for(&task.resume(None)), "two: 0 1");
        assert_eq!(asked_for(&task.resume(Some(Value::Int(1)))), "two: 1 2");
        assert_eq!(asked_for(&task.resume(Some(Value::Int(3)))), "done: 3");
    }

    #[test]
    fn a_reduce_task_over_nothing_yields_its_initial_value() {
        let mut task = HostTask::Reduce {
            func: callback(),
            items: Vec::new(),
            next: 0,
            acc: Value::Int(42),
        };
        assert_eq!(asked_for(&task.resume(None)), "done: 42");
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

    #[test]
    fn input_reads_a_line() {
        let output =
            crate::run_capture_with_input("let name = input()\nprint(\"hi\", name)", &["Aiko"])
                .expect("program should run");
        assert_eq!(output, "hi Aiko\n");
    }

    #[test]
    fn input_writes_the_prompt() {
        let output = crate::run_capture_with_input("let x = input(\"name? \")\nprint(x)", &["Bo"])
            .expect("program should run");
        assert_eq!(output, "name? Bo\n");
    }

    #[test]
    fn input_returns_nil_at_end_of_input() {
        let output =
            crate::run_capture_with_input("print(input())", &[]).expect("program should run");
        assert_eq!(output, "nil\n");
    }

    #[test]
    fn map_applies_a_function_to_each_element() {
        assert_eq!(
            out("print(map([1, 2, 3], fn(x) { return x * 2 }))"),
            "[2, 4, 6]\n"
        );
    }

    #[test]
    fn map_over_an_empty_array_is_empty() {
        assert_eq!(out("print(map([], fn(x) { return x }))"), "[]\n");
    }

    #[test]
    fn map_can_apply_a_closure() {
        assert_eq!(
            out("let n = 10\nlet add = fn(x) { return x + n }\nprint(map([1, 2, 3], add))"),
            "[11, 12, 13]\n"
        );
    }

    #[test]
    fn map_can_apply_a_builtin() {
        assert_eq!(out("print(map([-1, -2, 3], abs))"), "[1, 2, 3]\n");
    }

    #[test]
    fn map_rejects_non_arrays() {
        assert!(err("map(5, fn(x) { return x })").contains("expects an array"));
    }

    #[test]
    fn map_rejects_a_non_callable_function() {
        assert!(err("map([1, 2], 3)").contains("not callable"));
    }

    #[test]
    fn map_checks_arity() {
        assert!(err("map([1, 2])").contains("2 argument(s)"));
    }

    #[test]
    fn filter_keeps_elements_where_the_predicate_is_truthy() {
        assert_eq!(
            out("print(filter([1, 2, 3, 4], fn(x) { return x % 2 == 0 }))"),
            "[2, 4]\n"
        );
    }

    #[test]
    fn filter_can_drop_every_element() {
        assert_eq!(out("print(filter([1, 2], fn(x) { return false }))"), "[]\n");
    }

    #[test]
    fn filter_treats_nil_and_false_as_falsy() {
        assert_eq!(
            out("print(filter([1, 2, 3], fn(x) { if x == 2 { return nil } return true }))"),
            "[1, 3]\n"
        );
    }

    #[test]
    fn filter_rejects_non_arrays() {
        assert!(err("filter(5, fn(x) { return true })").contains("expects an array"));
    }

    #[test]
    fn filter_rejects_a_non_callable_predicate() {
        assert!(err("filter([1, 2], 3)").contains("not callable"));
    }

    #[test]
    fn filter_checks_arity() {
        assert!(err("filter([1, 2])").contains("2 argument(s)"));
    }

    #[test]
    fn reduce_folds_from_the_left() {
        assert_eq!(
            out("print(reduce([1, 2, 3, 4], fn(acc, x) { return acc + x }, 0))"),
            "10\n"
        );
    }

    #[test]
    fn reduce_returns_the_initial_value_for_an_empty_array() {
        assert_eq!(
            out("print(reduce([], fn(acc, x) { return acc + x }, 42))"),
            "42\n"
        );
    }

    #[test]
    fn reduce_can_build_up_a_non_number() {
        assert_eq!(
            out("print(reduce([\"a\", \"b\", \"c\"], fn(acc, x) { return acc + x }, \"\"))"),
            "abc\n"
        );
    }

    #[test]
    fn reduce_rejects_non_arrays() {
        assert!(err("reduce(5, fn(acc, x) { return acc }, 0)").contains("expects an array"));
    }

    #[test]
    fn reduce_rejects_a_non_callable_function() {
        assert!(err("reduce([1, 2], 3, 0)").contains("not callable"));
    }

    #[test]
    fn reduce_checks_arity() {
        assert!(err("reduce([1, 2], fn(acc, x) { return acc })").contains("3 argument(s)"));
    }
}
