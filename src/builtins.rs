//! Native builtins that every program can call.
//!
//! Each builtin matches [`crate::value::BuiltinFn`]. Errors are returned as
//! plain strings; the virtual machine attaches the source line and column of
//! the call before surfacing them.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::globals::Globals;
use crate::value::{
    Ambient, AmbientFn, Builtin, BuiltinFn, HostBuiltin, HostFn, Input, NativeFn, Output, System,
    SystemFn, Value,
};

/// Register every builtin into a program's globals.
pub fn register(globals: &mut Globals) {
    define(globals, "print", print);
    define(globals, "eprint", eprint);
    define(globals, "exit", exit);
    define(globals, "len", len);
    define(globals, "push", push);
    define(globals, "insert", insert);
    define(globals, "str", to_str);
    define(globals, "type", type_of);
    define(globals, "is_error", is_error);
    define(globals, "range", range);
    define(globals, "keys", keys);
    define(globals, "values", values);
    define(globals, "has", has);
    define(globals, "remove", remove);
    define(globals, "upper", upper);
    define(globals, "lower", lower);
    define(globals, "ord", ord);
    define(globals, "chr", chr);
    define(globals, "trim", trim);
    define(globals, "replace", replace);
    define(globals, "split", split);
    define(globals, "join", join);
    define(globals, "contains", contains);
    define(globals, "find", find);
    define(globals, "starts_with", starts_with);
    define(globals, "ends_with", ends_with);
    define(globals, "pop", pop);
    define(globals, "index_of", index_of);
    define(globals, "slice", slice);
    define(globals, "reverse", reverse);
    define(globals, "abs", abs);
    define(globals, "min", min);
    define(globals, "max", max);
    define(globals, "floor", floor);
    define(globals, "ceil", ceil);
    define(globals, "round", round);
    define(globals, "sqrt", sqrt);
    define(globals, "pow", pow);
    define(globals, "sum", sum);
    define(globals, "product", product);
    define(globals, "int", int);
    define(globals, "float", float);
    define(globals, "input", input);
    define_system(globals, "read_file", read_file);
    define_system(globals, "write_file", write_file);
    define_system(globals, "file_exists", file_exists);
    define_system(globals, "args", args);
    define_ambient(globals, "now", now);
    define_ambient(globals, "sleep", sleep);
    define_ambient(globals, "random", random);
    define_ambient(globals, "random_int", random_int);
    define_ambient(globals, "seed", seed);
    define_ambient(globals, "read_key", read_key);
    define_ambient(globals, "key_ready", key_ready);
    define_ambient(globals, "clear", clear);
    define_ambient(globals, "move_to", move_to);
    define_ambient(globals, "hide_cursor", hide_cursor);
    define_ambient(globals, "show_cursor", show_cursor);
    define_ambient(globals, "term_size", term_size);
    define_host(globals, "sort", sort);
    define_host(globals, "map", map);
    define_host(globals, "filter", filter);
    define_host(globals, "reduce", reduce);
}

/// Whether a builtin may be handed a caught error.
///
/// Every other builtin refuses one, because passing an error on as though it
/// were a result is the mistake this milestone exists to prevent. Asking what
/// type a value is has to be the exception: it is how a program finds out that
/// it is holding an error in the first place, and a check that cannot be made
/// without tripping the guard is no check at all.
pub fn accepts_error(name: &str) -> bool {
    matches!(name, "type" | "is_error")
}

fn define(globals: &mut Globals, name: &'static str, func: BuiltinFn) {
    let slot = globals
        .slot_for_builtin(name)
        .expect("room for the builtins");
    let func = NativeFn::Plain(func);
    globals.define(slot, Value::Builtin(Builtin { name, func }));
}

/// Register a builtin that needs the host's file system or command line.
///
/// Called like any other builtin, and differing only in what it is handed,
/// which is why it is a kind of [`NativeFn`] rather than a kind of [`Value`].
fn define_system(globals: &mut Globals, name: &'static str, func: SystemFn) {
    let slot = globals
        .slot_for_builtin(name)
        .expect("room for the builtins");
    let func = NativeFn::System(func);
    globals.define(slot, Value::Builtin(Builtin { name, func }));
}

/// Register a builtin that reads something its arguments do not contain: the
/// clock, or the random number generator.
///
/// Called like any other builtin, as [`define_system`] is, and separate for the
/// same reason: what it is handed differs, and nothing else does.
fn define_ambient(globals: &mut Globals, name: &'static str, func: AmbientFn) {
    let slot = globals
        .slot_for_builtin(name)
        .expect("room for the builtins");
    let func = NativeFn::Ambient(func);
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

/// `eprint(...)` is `print` to the diagnostic stream instead of the result
/// stream.
///
/// Deliberately the same in every other way: same separator, same trailing
/// newline, same acceptance of any number of arguments. A program that changes
/// `print` to `eprint` should change where the text goes and nothing else, so
/// there is one rule to learn rather than two.
///
/// This is what lets a script say something went wrong without putting it in
/// the output a caller is parsing. Until now the choice was to print a
/// complaint into the middle of the result, or to raise an error and say
/// nothing else at all.
fn eprint(out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    let parts: Vec<String> = args.iter().map(Value::display).collect();
    out.write_error(&parts.join(" "));
    out.write_error("\n");
    Ok(Value::Nil)
}

/// `exit(code)` stops the program and gives the code to whoever ran it.
///
/// This only asks. Stopping is done by returning an error, which unwinds the
/// way any other error does, and the virtual machine marks that error **fatal**
/// once a code has been recorded so a `try` cannot swallow it. Without that a
/// program would carry on running with an exit code already set, and the caller
/// would be told a lie about a program that never stopped.
///
/// The message below is not normally seen: whoever runs the program reads the
/// code first and reports nothing. It is written to be honest for the one case
/// where it does surface, an embedder calling [`crate::vm::Vm::run`] directly
/// and never asking for the code.
///
/// **0 through 255.** That is what a process may return on the platforms this
/// runs on; 256 is not a smaller number than 255 to an operating system, it is
/// zero, and a program silently reporting success because it asked for 256 is
/// the worst answer available. So it is refused, and the range is in section 9
/// of the specification with the other limits.
fn exit(out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("exit", &args, 1)?;
    let code = match &args[0] {
        Value::Int(n) => *n,
        other => {
            return Err(format!(
                "exit expects an int but got a {}",
                other.type_name()
            ))
        }
    };
    if !(0..=255).contains(&code) {
        return Err(format!("exit code must be from 0 to 255 but got {code}"));
    }
    out.request_exit(code as i32);
    Err("the program called exit".to_string())
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

/// `insert(array, index, value)` puts a value at a position, in place, and
/// returns the array.
///
/// The companion of `push`, which can only append. Putting something at the
/// front is the common case — a queue, or the new head of a snake — and without
/// this it takes a fresh array and a loop.
///
/// **The index must be from 0 to the length, and anything else is an error.**
/// `insert(a, len(a), v)` appends, which is the one past-the-end position that
/// means something: it is where a new last element goes.
///
/// This follows indexing rather than `slice`. `slice` clamps, because a range
/// asking for more than exists can sensibly give what exists. A position is not
/// a range: `insert(a, 99, v)` on an array of three is a mistake in the caller's
/// arithmetic, and quietly appending would hide it. `a[99]` is an error for the
/// same reason, and the message here is shaped like that one.
fn insert(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("insert", &args, 3)?;
    let index = match &args[1] {
        Value::Int(n) => *n,
        other => {
            return Err(format!(
                "insert expects an integer index but got a {}",
                other.type_name()
            ))
        }
    };
    match &args[0] {
        Value::Array(items) => {
            let mut items = items.borrow_mut();
            if index < 0 {
                return Err(format!("index {index} is out of range (negative)"));
            }
            // Equal to the length is allowed and appends; past it is not.
            if index as usize > items.len() {
                return Err(format!(
                    "index {index} is out of range for inserting into an array of length {}",
                    items.len()
                ));
            }
            items.insert(index as usize, args[2].clone());
            drop(items);
            Ok(args[0].clone())
        }
        other => Err(format!(
            "insert expects an array as its first argument but got a {}",
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

/// Whether a value is an error caught by `try`.
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
    Ok(Value::array(items))
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
            Ok(Value::array(items))
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
            Ok(Value::array(items))
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

/// `remove(map, key)` takes the key out of the map and gives the value it held,
/// or `nil` when the map held no such key.
///
/// This is the inverse of assignment, which a map had no way to undo. Setting a
/// key to `nil` does not remove it: the key stays, `len` still counts it, and
/// `keys` still lists it. Arrays have had `pop` since v0.2 and maps had
/// nothing, so a program that built a map could not filter one without
/// rebuilding it by hand.
///
/// **An absent key gives `nil` rather than an error**, which is how reading one
/// already behaves: `m["absent"]` is `nil` and not a failure. That makes
/// "remove it if it is there" one call instead of a `has` and then a `remove`,
/// which is the shape most programs want.
///
/// The cost is real and is written down in the specification: a key holding
/// `nil` and a key that is not there give the same answer. `has(map, key)`
/// tells them apart, and it has to be asked before the removal rather than
/// after.
///
/// The key is borrowed rather than copied into a `String`. `BTreeMap` keyed by
/// `String` accepts a `&str` through `Borrow`, which is the same allocation
/// v1.1 removed from reading a map.
fn remove(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("remove", &args, 2)?;
    let key = match &args[1] {
        Value::Str(s) => s.as_str(),
        other => {
            return Err(format!(
                "remove expects a string key but got a {}",
                other.type_name()
            ))
        }
    };
    match &args[0] {
        Value::Map(entries) => Ok(entries.borrow_mut().remove(key).unwrap_or(Value::Nil)),
        other => Err(format!(
            "remove expects a map but got a {}",
            other.type_name()
        )),
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

/// `ord(s)` gives the code point of a one-character string.
///
/// **Exactly one character, not the first of several.** Python's `ord` refuses
/// a longer string and JavaScript's `charCodeAt` takes the first; refusing is
/// what matches this project, where `term_size` refuses rather than inventing
/// a width and `sqrt` refuses rather than returning `nan`. `ord("hello")`
/// quietly meaning 104 is a bug that runs.
///
/// Characters, not bytes, like everything else here, so this is the code point
/// and not the first of the four bytes an emoji is stored in.
fn ord(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("ord", &args, 1)?;
    match &args[0] {
        Value::Str(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(only), None) => Ok(Value::Int(only as i64)),
                // Counted rather than described, so the message says which
                // mistake was made: an empty string and a whole word are
                // different errors with the same cause.
                _ => Err(format!(
                    "ord expects a string of one character but got one of {}",
                    s.chars().count()
                )),
            }
        }
        other => Err(format!(
            "ord expects a string but got a {}",
            other.type_name()
        )),
    }
}

/// `chr(n)` gives the one-character string for a code point.
///
/// **The refusals are the `\u{...}` escape's, because they are the same
/// question.** `char::from_u32` is what the lexer consults at `read_unicode_
/// escape`, and it is the whole rule: everything above `10FFFF` is refused,
/// and so is a surrogate from `D800` to `DFFF`. Calling it here rather than
/// writing a second range check is what keeps the two from drifting apart, and
/// the message is the escape's word for word, so `chr(0xD800)` and
/// `"\u{D800}"` do not read as two different problems.
///
/// `chr(ord(c)) == c` for every character the language admits, which is the
/// property the golden corpus pins.
fn chr(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    check_arity("chr", &args, 1)?;
    let Value::Int(n) = &args[0] else {
        return Err(format!(
            "chr expects an int but got a {}",
            args[0].type_name()
        ));
    };
    // Spelled in hex, matching the escape this agrees with, even though the
    // argument is usually written in decimal. A number too big or too small to
    // be a code point at all is reported as itself instead: rendering -1 in
    // hex gives '\u{FFFFFFFFFFFFFFFF}', which describes the bits rather than
    // the mistake.
    let refuse = || match u32::try_from(*n) {
        Ok(code) => format!("'\\u{{{code:X}}}' is not a character"),
        Err(_) => format!("{n} is not a character"),
    };
    let code = u32::try_from(*n).map_err(|_| refuse())?;
    match char::from_u32(code) {
        Some(found) => Ok(Value::Str(Rc::new(found.to_string()))),
        None => Err(refuse()),
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
            Ok(Value::array(parts))
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
        Value::Array(items) => Ok(Value::Bool({
            let items = items.borrow();
            let mut found = false;
            for item in items.iter() {
                if item.equals(&args[1])? {
                    found = true;
                    break;
                }
            }
            found
        })),
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

/// `starts_with(s, prefix)` reports whether `s` begins with `prefix`.
///
/// The comparison is by byte, which is not a way around the rule that every
/// string builtin here counts characters. UTF-8 is self synchronising and both
/// arguments are whole strings, so a needle that matches the leading bytes
/// matches the leading characters as well: a byte prefix cannot stop part way
/// through one. `find` counts because it gives back an index. This gives back
/// yes or no, and there the two measures agree.
fn starts_with(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("starts_with", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(prefix)) => Ok(Value::Bool(s.starts_with(prefix.as_str()))),
        _ => Err("starts_with expects two string arguments".to_string()),
    }
}

/// `ends_with(s, suffix)` reports whether `s` ends with `suffix`.
///
/// [`starts_with`] gives the reason a byte comparison answers a question about
/// characters. It holds at the end of a string for the same reason it holds at
/// the start.
fn ends_with(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    check_arity("ends_with", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(s), Value::Str(suffix)) => Ok(Value::Bool(s.ends_with(suffix.as_str()))),
        _ => Err("ends_with expects two string arguments".to_string()),
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
            // A loop rather than `position`, because a comparison can now refuse
            // and a closure cannot carry that out of an iterator adaptor. The
            // refusal has to travel: swallowing it would answer -1, which reads
            // as "not present" and is the wrong answer rather than no answer.
            let items = items.borrow();
            for (index, item) in items.iter().enumerate() {
                if item.equals(&args[1])? {
                    return Ok(Value::Int(index as i64));
                }
            }
            Ok(Value::Int(-1))
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
            Ok(Value::array(items[lo..hi].to_vec()))
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

/// How a set of values is ordered, decided once by looking at all of them.
///
/// Extracted so that the two forms of `sort` cannot come to disagree. Both ask
/// this same question: `sort(a)` asks it about the elements and `sort(a, key)`
/// asks it about the keys. Two copies of this decision is how one form would
/// end up refusing `NaN` while the other quietly ordered it.
#[derive(Clone, Copy)]
enum Ordered {
    Ints,
    Strings,
    /// A mix of integers and floats, compared as floats. Section 5.2 of the
    /// specification gives the promotion rule this follows.
    Numbers,
}

impl Ordered {
    /// Decide how to order these values, or say why they cannot be ordered.
    fn of(values: &[Value]) -> Result<Ordered, String> {
        if values.iter().all(|v| matches!(v, Value::Int(_))) {
            Ok(Ordered::Ints)
        } else if values.iter().all(|v| matches!(v, Value::Str(_))) {
            Ok(Ordered::Strings)
        } else if values
            .iter()
            .all(|v| matches!(v, Value::Int(_) | Value::Float(_)))
        {
            // NaN is refused rather than ordered arbitrarily. It compares equal
            // to nothing, including itself, so a sort that admitted it would
            // give a different arrangement depending on where it started.
            if values
                .iter()
                .any(|v| matches!(v, Value::Float(f) if f.is_nan()))
            {
                Err("sort cannot order NaN".to_string())
            } else {
                Ok(Ordered::Numbers)
            }
        } else {
            Err("sort expects an array of all numbers or all strings".to_string())
        }
    }

    fn compare(self, a: &Value, b: &Value) -> Ordering {
        match (self, a, b) {
            (Ordered::Ints, Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Ordered::Strings, Value::Str(x), Value::Str(y)) => x.cmp(y),
            // `of` has already refused NaN, so the comparison always answers.
            (Ordered::Numbers, _, _) => number_as_f64(a)
                .partial_cmp(&number_as_f64(b))
                .unwrap_or(Ordering::Equal),
            // Unreachable: `of` checked every value against the kind it chose.
            _ => Ordering::Equal,
        }
    }
}

/// Order `values` in place, or say why they cannot be ordered.
fn sort_in_place(values: &mut [Value]) -> Result<(), String> {
    let ordering = Ordered::of(values)?;
    values.sort_by(|a, b| ordering.compare(a, b));
    Ok(())
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
            Ok(Value::array(copy))
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

/// Shared implementation of `sum` and `product`: fold an array of numbers.
///
/// `identity` is the answer for an empty array, 0 for a sum and 1 for a
/// product. Those are the values that let the builtins compose: summing the
/// halves of a split array gives the sum of the whole however the split fell,
/// including when one half is empty.
///
/// The fold holds an integer until a float arrives and a float afterwards,
/// which is section 5.2's promotion rule applied one element at a time. The
/// integer step is checked, because every integer operation in this language is
/// checked and none of them wraps.
///
/// `NaN` is not refused, unlike in `min` and `max`. Those compare, and there is
/// no answer to a comparison with `NaN`. This only adds or multiplies, where
/// `NaN` propagates and is a value the language already holds.
fn fold_numbers(
    name: &str,
    args: Vec<Value>,
    identity: i64,
    step_int: fn(i64, i64) -> Option<i64>,
    step_float: fn(f64, f64) -> f64,
) -> Result<Value, String> {
    check_arity(name, &args, 1)?;
    let items = match &args[0] {
        Value::Array(items) => items,
        other => {
            return Err(format!(
                "{name} expects an array but got a {}",
                other.type_name()
            ))
        }
    };

    let mut whole = identity;
    let mut fraction: Option<f64> = None;
    let items = items.borrow();
    for item in items.iter() {
        match (item, fraction) {
            (Value::Int(n), None) => {
                whole = step_int(whole, *n).ok_or_else(|| format!("integer overflow in {name}"))?;
            }
            (Value::Int(n), Some(f)) => fraction = Some(step_float(f, *n as f64)),
            (Value::Float(x), None) => fraction = Some(step_float(whole as f64, *x)),
            (Value::Float(x), Some(f)) => fraction = Some(step_float(f, *x)),
            (other, _) => {
                return Err(format!(
                    "{name} expects an array of numbers but got a {}",
                    other.type_name()
                ))
            }
        }
    }

    Ok(match fraction {
        Some(f) => Value::Float(f),
        None => Value::Int(whole),
    })
}

/// `sum(a)` adds the numbers in an array. An empty array gives `0`.
fn sum(_out: &mut dyn Output, _input: &mut dyn Input, args: Vec<Value>) -> Result<Value, String> {
    fold_numbers("sum", args, 0, i64::checked_add, |a, b| a + b)
}

/// `product(a)` multiplies the numbers in an array. An empty array gives `1`.
fn product(
    _out: &mut dyn Output,
    _input: &mut dyn Input,
    args: Vec<Value>,
) -> Result<Value, String> {
    fold_numbers("product", args, 1, i64::checked_mul, |a, b| a * b)
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

/// `args()` gives the arguments the program was given, as an array of strings.
///
/// The program's own path is not among them. It is not an argument to the
/// program, and a caller that wants it already knows it.
///
/// Empty rather than an error where there is no command line, because a program
/// that was given no arguments and a program that could not have been given any
/// both have none, and the loop over them is the same either way.
fn args(system: &mut dyn System, args: Vec<Value>) -> Result<Value, String> {
    check_arity("args", &args, 0)?;
    let items = system
        .arguments()
        .into_iter()
        .map(|arg| Value::Str(Rc::new(arg)))
        .collect();
    Ok(Value::array(items))
}

/// `read_file(path)` gives the whole file as a string.
///
/// A relative path is resolved against the working directory, which is not what
/// `import` does. Section 8 of the specification states the difference and why.
fn read_file(system: &mut dyn System, args: Vec<Value>) -> Result<Value, String> {
    check_arity("read_file", &args, 1)?;
    match &args[0] {
        Value::Str(path) => system.read_file(path).map(|text| Value::Str(Rc::new(text))),
        other => Err(format!(
            "read_file expects a string path but got a {}",
            other.type_name()
        )),
    }
}

/// `write_file(path, contents)` writes the string, replacing what was there.
fn write_file(system: &mut dyn System, args: Vec<Value>) -> Result<Value, String> {
    check_arity("write_file", &args, 2)?;
    match (&args[0], &args[1]) {
        (Value::Str(path), Value::Str(contents)) => {
            system.write_file(path, contents).map(|()| Value::Nil)
        }
        (Value::Str(_), other) => Err(format!(
            "write_file expects a string to write but got a {}",
            other.type_name()
        )),
        (other, _) => Err(format!(
            "write_file expects a string path but got a {}",
            other.type_name()
        )),
    }
}

/// `file_exists(path)` says whether there is a file at the path.
///
/// This one answers rather than refusing where there is no file system, because
/// the honest answer to "is there a file there" is then no. A program that asks
/// before reading gets a useful `false` instead of an error it has to catch, and
/// the read itself still refuses if it is tried anyway.
fn file_exists(system: &mut dyn System, args: Vec<Value>) -> Result<Value, String> {
    check_arity("file_exists", &args, 1)?;
    match &args[0] {
        Value::Str(path) => Ok(Value::Bool(system.file_exists(path))),
        other => Err(format!(
            "file_exists expects a string path but got a {}",
            other.type_name()
        )),
    }
}

/// `now()` gives the milliseconds since 1970-01-01T00:00:00Z.
///
/// An integer rather than a float, because a float loses whole milliseconds
/// somewhere in the year 287396 and an integer does not, and because the two
/// things a program does with this are subtract one from another and print it.
///
/// It refuses where the host has no clock, rather than answering 0. Section 8
/// of the specification says so, and `try` catches it.
fn now(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("now", &args, 0)?;
    ambient.now_millis().map(Value::Int)
}

/// `sleep(ms)` does nothing for that many milliseconds.
///
/// This is what lets a loop hold a frame rate. Without it a loop runs as fast as
/// the machine allows — measured at just under eight million turns a second on
/// an ordinary one — which is a pegged processor and a game whose speed depends
/// on what it is running on.
///
/// **A negative duration is an error rather than a no-op.** Nobody means to wait
/// for less than no time, so it is a mistake somewhere further up, usually a
/// subtraction that went the wrong way while working out how much of a frame is
/// left. Returning at once would hide it and leave the loop running flat out,
/// which is the exact fault this builtin exists to fix.
///
/// Zero is not an error, because `sleep(deadline - now())` legitimately reaches
/// zero on the frame where the work took exactly as long as the frame.
///
/// It refuses where the host cannot pause, which is any page: see
/// `BrowserClock::sleep_millis`. `try` catches it.
fn sleep(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("sleep", &args, 1)?;
    let millis = match &args[0] {
        Value::Int(n) => *n,
        other => {
            return Err(format!(
                "sleep expects an int number of milliseconds but got a {}",
                other.type_name()
            ))
        }
    };
    if millis < 0 {
        return Err(format!(
            "sleep expects a number of milliseconds that is not negative but got {millis}"
        ));
    }
    ambient.sleep_millis(millis as u64).map(|()| Value::Nil)
}

/// `random()` gives a float from 0 up to but not including 1.
///
/// The generator seeds itself from the clock the first time a program asks, so
/// two runs differ. `seed` pins it when they must not.
fn random(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("random", &args, 0)?;
    Ok(Value::Float(ambient.rng().unit()))
}

/// `random_int(low, high)` gives an integer from `low` to `high`, both
/// included.
///
/// Both ends are in the range because that is what a program asking for a die
/// or a card means, and because a half-open range would make `random_int(1, 6)`
/// the wrong spelling of the most common use of it.
///
/// Floats are refused rather than truncated. `random_int(1, 6.5)` is a program
/// that has not decided what it wants, and either answer this could give would
/// be a guess about which.
fn random_int(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("random_int", &args, 2)?;
    let bound = |value: &Value, which: &str| match value {
        Value::Int(n) => Ok(*n),
        other => Err(format!(
            "random_int expects an int {which} bound but got a {}",
            other.type_name()
        )),
    };
    let low = bound(&args[0], "low")?;
    let high = bound(&args[1], "high")?;
    if low > high {
        return Err(format!(
            "random_int expects a low bound not above the high bound but got {low} and {high}"
        ));
    }
    Ok(Value::Int(ambient.rng().int_in(low, high)))
}

/// `seed(n)` starts the generator again from `n`.
///
/// This is what makes a program that uses random numbers testable. Two runs
/// that start from the same seed draw the same numbers, so a program can be
/// asserted against exact output and still be about chance.
///
/// Any integer is a seed, including one a program has already used. There is no
/// value to refuse: the generator has 2^64 states and every integer names one.
fn seed(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("seed", &args, 1)?;
    match &args[0] {
        Value::Int(n) => {
            ambient.set_seed(*n);
            Ok(Value::Nil)
        }
        other => Err(format!(
            "seed expects an int but got a {}",
            other.type_name()
        )),
    }
}

/// `read_key()` gives the next key, without waiting for a line.
///
/// A string, which is the character itself for a key that produced one and a
/// name from section 8.11 of the specification otherwise. `nil` at end of
/// input, matching what `input()` gives there.
///
/// It refuses where the host has no keyboard, as `now` refuses where there is
/// no clock, and `try` catches that.
fn read_key(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("read_key", &args, 0)?;
    Ok(match ambient.read_key()? {
        Some(key) => Value::Str(Rc::new(key)),
        None => Value::Nil,
    })
}

/// `key_ready()` says whether `read_key()` would answer without waiting.
///
/// **This is what lets a loop go round when nobody has pressed anything.**
/// `read_key` on its own waits, so a loop built from it only advances once per
/// key: a program cannot move something across the screen while the person
/// holds still, because it never gets control back to do it. Guarding the read
/// turns that inside out, and the world advances whether a key came or not.
///
/// ```text
/// while true {
///   while key_ready() { handle(read_key()) }   // take everything pressed
///   advance()                                  // whether or not any was
///   draw()
///   sleep(50)
/// }
/// ```
///
/// The inner loop drains rather than reading one, so a burst of keys pressed
/// inside one frame is all seen in that frame instead of arriving one per
/// frame for the next several.
///
/// **It is `true` at end of input**, because a read there answers at once. That
/// is what stops the drain loop above from spinning forever on a closed stream:
/// the read gives `nil` and the program can stop. It reports whether a read
/// will wait, not whether a key exists, and the two differ exactly there.
fn key_ready(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("key_ready", &args, 0)?;
    ambient.key_ready().map(Value::Bool)
}

/// `clear()` empties the screen and puts the cursor at the top left.
///
/// **This is what turns printing into drawing.** Without it a program that
/// prints a picture every frame produces a column of pictures scrolling
/// upwards; with it, each one replaces the last and the thing appears to move.
///
/// The whole frame should be built as one string and printed once. Printing it
/// a row at a time works and can be seen doing it, because the terminal draws
/// each row as it arrives and the eye catches the sweep.
///
/// Where standard output is not a terminal — a pipe, a file — this does
/// nothing, because there is no screen to clear. That is what keeps a game's
/// output readable when it is redirected, and comparable in a test.
fn clear(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("clear", &args, 0)?;
    ambient.screen().clear().map(|()| Value::Nil)
}

/// `move_to(column, row)` puts the cursor there, counting from zero.
///
/// From zero because the language indexes arrays from zero, and a program
/// drawing a grid is indexing one: `move_to(0, 0)` is the top left, matching
/// `grid[0][0]`. The escape sequence underneath counts from one, which is the
/// implementation's business.
fn move_to(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("move_to", &args, 2)?;
    let coordinate = |value: &Value, which: &str| match value {
        Value::Int(n) => Ok(*n),
        other => Err(format!(
            "move_to expects an int {which} but got a {}",
            other.type_name()
        )),
    };
    let column = coordinate(&args[0], "column")?;
    let row = coordinate(&args[1], "row")?;
    ambient.screen().move_to(column, row).map(|()| Value::Nil)
}

/// `hide_cursor()` stops the terminal drawing the cursor.
///
/// Worth doing in anything that animates: the cursor sits wherever the last
/// character was written, which in a redrawn frame is the bottom right, and it
/// blinks there through the whole game.
///
/// **The cursor comes back by itself when the program ends**, however it ends —
/// normally, on an error, or through `exit`. A program does not have to pair
/// this with `show_cursor`, for the same reason it does not have to put the
/// terminal back after `read_key`.
fn hide_cursor(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("hide_cursor", &args, 0)?;
    ambient.screen().hide_cursor().map(|()| Value::Nil)
}

/// `show_cursor()` draws the cursor again.
fn show_cursor(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("show_cursor", &args, 0)?;
    ambient.screen().show_cursor().map(|()| Value::Nil)
}

/// `term_size()` gives `[columns, rows]` for the terminal.
///
/// **It refuses where there is no terminal**, rather than answering with the
/// traditional 80 by 24. That would be a wrong answer and not an absent one,
/// and a program handed it draws a board that does not fit — the same reasoning
/// that makes `now()` refuse rather than answering 1970.
///
/// The consequence is worth stating: a program that calls this cannot run under
/// a pipe unless it catches the refusal. The bundled games fix their own size
/// instead, which is why they can be tested by piping keys into them.
fn term_size(ambient: &mut Ambient, args: Vec<Value>) -> Result<Value, String> {
    check_arity("term_size", &args, 0)?;
    let (columns, rows) = ambient.screen().size()?;
    Ok(Value::array(vec![Value::Int(columns), Value::Int(rows)]))
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
    /// Removing it means changing [`BuiltinFn`] for the forty builtins
    /// registered with `define`, several of which move values out of the vector
    /// they are handed, which is a larger change than it looks and is left for
    /// later.
    ///
    /// Forty-one is the count of `define` calls, not the count of builtins. The
    /// two were the same figure until 1.1 and are not any more: there are
    /// sixty-one builtins, of which four take a `SystemFn`, twelve take an
    /// `AmbientFn`, and four take a `HostFn`, and none of those twenty would
    /// be touched by such a change.
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
    Sort {
        /// The key function, or `Value::Nil` when `sort` was given one
        /// argument. That spelling is unambiguous because the constructor
        /// refuses a second argument that is not callable, so a program cannot
        /// reach here by writing `sort(a, nil)`.
        func: Value,
        items: Vec<Value>,
        next: usize,
        /// The element whose key is being computed, kept for the same reason
        /// `Filter` keeps one: it is moved into the result rather than cloned
        /// a second time when the answer arrives.
        pending: Option<Value>,
        /// The keys, and the elements they belong to, at matching indexes.
        ///
        /// Two vectors rather than one of pairs, so that deciding how to order
        /// the keys can look at them as a slice without cloning every one of
        /// them out of a tuple first.
        keys: Vec<Value>,
        kept: Vec<Value>,
    },
}

impl HostTask {
    /// The function this task applies. It is the same for every step, which is
    /// why [`Step::Call`] does not repeat it.
    pub fn callback(&self) -> &Value {
        match self {
            HostTask::Map { func, .. }
            | HostTask::Filter { func, .. }
            | HostTask::Reduce { func, .. }
            | HostTask::Sort { func, .. } => func,
        }
    }

    /// Advance the builtin one step. `last` is the value the previously
    /// requested call returned, or `None` on the first step.
    ///
    /// Fallible because `filter` asks its callback's answer whether it is true,
    /// and a caught error refuses to answer that. `map` and `reduce` only ever
    /// store what they are given, which is what assigning an error already does
    /// everywhere else, so they cannot fail here.
    pub fn resume(&mut self, last: Option<Value>) -> Result<Step, String> {
        match self {
            HostTask::Map {
                items, next, out, ..
            } => {
                if let Some(value) = last {
                    out.push(value);
                }
                match take_next(items, next) {
                    None => Ok(Step::Done(array(std::mem::take(out)))),
                    Some(item) => Ok(Step::Call(Args::One(item))),
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
                    // `condition`, not `is_truthy`: this is a conditional, and a
                    // caught error must refuse to be one here exactly as it does
                    // in `if` and `while`. Reading it as true kept every element
                    // and said nothing.
                    if value.condition()? {
                        out.push(item);
                    }
                }
                match take_next(items, next) {
                    None => Ok(Step::Done(array(std::mem::take(out)))),
                    Some(item) => {
                        *pending = Some(item.clone());
                        Ok(Step::Call(Args::One(item)))
                    }
                }
            }
            HostTask::Sort {
                func,
                items,
                next,
                pending,
                keys,
                kept,
            } => {
                // One argument. The whole sort happens on this first resume and
                // the task never asks for a call, so `callback` is never
                // consulted and the `nil` held there is never reached.
                if matches!(func, Value::Nil) {
                    let mut sorted = std::mem::take(items);
                    sort_in_place(&mut sorted)?;
                    return Ok(Step::Done(array(sorted)));
                }
                if let Some(key) = last {
                    keys.push(key);
                    kept.push(pending.take().expect("an element under decision"));
                }
                match take_next(items, next) {
                    None => {
                        let ordering = Ordered::of(keys)?;
                        // A permutation is sorted rather than the elements, so
                        // no key and no element is cloned to get them into one
                        // place. `sort_by` is stable, which is what makes two
                        // passes a two-key sort and is promised in section 8.4
                        // of the specification.
                        let mut order: Vec<usize> = (0..keys.len()).collect();
                        order.sort_by(|&a, &b| ordering.compare(&keys[a], &keys[b]));
                        let out = order
                            .into_iter()
                            .map(|i| std::mem::replace(&mut kept[i], Value::Nil))
                            .collect();
                        Ok(Step::Done(array(out)))
                    }
                    Some(item) => {
                        *pending = Some(item.clone());
                        Ok(Step::Call(Args::One(item)))
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
                    None => Ok(Step::Done(std::mem::replace(acc, Value::Nil))),
                    Some(item) => {
                        // Move the accumulator out rather than cloning it. The
                        // call's result puts one back on the next resume.
                        let carried = std::mem::replace(acc, Value::Nil);
                        Ok(Step::Call(Args::Two(carried, item)))
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
    Value::array(items)
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

/// `sort(array)` gives a sorted copy. `sort(array, key)` gives one ordered by
/// `key(x)` for each element, rather than by the elements themselves.
///
/// A key function and not a comparator. A key is asked for once per element,
/// which is a walk from start to end and so fits the shape every other
/// higher-order builtin already has; a comparator would mean driving a sort as
/// a state machine that hands out one comparison at a time. Decreasing order is
/// `reverse(sort(a, key))`, and a two-key sort is two passes, least significant
/// first, which works because the sort is stable.
fn sort(args: Vec<Value>) -> Result<HostTask, String> {
    if args.is_empty() || args.len() > 2 {
        return Err(format!(
            "sort expects 1 or 2 argument(s) but got {}",
            args.len()
        ));
    }
    let mut args = args.into_iter();
    let items = match args.next().expect("at least one argument") {
        Value::Array(items) => items.borrow().clone(),
        other => {
            return Err(format!(
                "sort expects an array but got a {}",
                other.type_name()
            ))
        }
    };
    // Checked here rather than left to fail at the call, which is what `map`
    // and `filter` do. This one has to know the difference between "no key
    // function" and "a second argument that is not one", because the first is
    // spelled as `nil` in the task.
    let func = match args.next() {
        None => Value::Nil,
        Some(func @ (Value::Closure(_) | Value::Builtin(_) | Value::HostBuiltin(_))) => func,
        Some(other) => {
            return Err(format!(
                "sort expects a function but got a {}",
                other.type_name()
            ))
        }
    };
    Ok(HostTask::Sort {
        keys: Vec::with_capacity(items.len()),
        kept: Vec::with_capacity(items.len()),
        func,
        items,
        next: 0,
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
    ///
    /// Takes the whole `Result` so a refusal reads as a value here rather than
    /// unwrapping at every call site. Only `filter` can produce one.
    fn asked_for(step: &Result<Step, String>) -> String {
        match step {
            Ok(Step::Call(Args::One(v))) => v.repr(),
            Ok(Step::Call(Args::Two(a, b))) => format!("two: {} {}", a.repr(), b.repr()),
            Ok(Step::Done(v)) => format!("done: {}", v.repr()),
            Err(message) => format!("refused: {message}"),
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

    /// `eprint` writes to the diagnostic stream and not to the result stream.
    ///
    /// Asserting on both is the point. A capture that merged them, or a builtin
    /// that quietly called `write` instead of `write_error`, would pass a test
    /// that only checked the text had appeared somewhere.
    #[test]
    fn eprint_writes_to_the_diagnostic_stream() {
        let captured =
            crate::run_capture_all("print(\"result\")\neprint(\"warning\")").expect("it runs");
        assert_eq!(captured.out, "result\n");
        assert_eq!(captured.err, "warning\n");
        assert_eq!(captured.code, 0);
    }

    /// Whatever `print` does with its arguments, `eprint` does too. One rule.
    #[test]
    fn eprint_formats_exactly_as_print_does() {
        for source in [
            "eprint()",
            "eprint(1)",
            "eprint(1, \"a\", true, nil)",
            "eprint([1, 2], {\"k\": 1})",
        ] {
            let printed = crate::run_capture_all(&source.replace("eprint", "print")).expect("runs");
            let complained = crate::run_capture_all(source).expect("runs");
            assert_eq!(
                complained.err, printed.out,
                "eprint and print disagree on {source}"
            );
        }
    }

    /// A program that stops on purpose keeps the output it already produced,
    /// and reports the code rather than an error.
    #[test]
    fn exit_carries_a_code_and_keeps_what_was_printed() {
        let captured =
            crate::run_capture_all("print(\"done\")\neprint(\"why\")\nexit(2)").expect("it runs");
        assert_eq!(captured.code, 2);
        assert_eq!(captured.out, "done\n");
        assert_eq!(captured.err, "why\n");
    }

    /// An ordinary end is code 0 without anybody asking.
    #[test]
    fn a_program_that_never_calls_exit_reports_zero() {
        assert_eq!(crate::run_capture_all("print(1)").expect("runs").code, 0);
    }

    #[test]
    fn exit_refuses_a_code_an_operating_system_cannot_carry() {
        assert_eq!(
            err("exit(256)"),
            "exit code must be from 0 to 255 but got 256"
        );
        assert_eq!(
            err("exit(-1)"),
            "exit code must be from 0 to 255 but got -1"
        );
        assert_eq!(err("exit(1.5)"), "exit expects an int but got a float");
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
    fn sum_and_product_fold_an_array_of_numbers() {
        assert_eq!(out("print(sum([1, 2, 3]), product([2, 3, 4]))"), "6 24\n");
        // The identity values, which are what make the two compose: the sum of
        // the halves of a split array is the sum of the whole, whichever way it
        // was split.
        assert_eq!(out("print(sum([]), product([]))"), "0 1\n");
        assert_eq!(out("print(sum([7]), product([7]))"), "7 7\n");
    }

    #[test]
    fn sum_and_product_promote_to_a_float_when_one_element_is_one() {
        assert_eq!(out("print(sum([1, 2.5]), product([2, 1.5]))"), "3.5 3.0\n");
        // The float can arrive first, last, or alone, and the answer is a float
        // in each. The identity has to carry across the change of type, which
        // is where a fold like this usually goes wrong.
        assert_eq!(out("print(sum([2.5, 1]), sum([2.0]))"), "3.5 2.0\n");
        assert_eq!(out("print(product([2.0]), product([1.5, 2]))"), "2.0 3.0\n");
    }

    #[test]
    fn sum_and_product_refuse_overflow_rather_than_wrapping() {
        // Every integer operation in this language is checked, and these are no
        // different from `+` and `*` written out.
        assert!(err("print(sum([9223372036854775807, 1]))").contains("integer overflow in sum"));
        assert!(
            err("print(product([4294967296, 4294967296]))").contains("integer overflow in product")
        );
    }

    #[test]
    fn sum_and_product_refuse_what_is_not_an_array_of_numbers() {
        assert!(err("print(sum([1, \"two\"]))").contains("an array of numbers"));
        assert!(err("print(product(5))").contains("expects an array"));
    }

    #[test]
    fn sum_does_not_refuse_nan_the_way_min_and_max_do() {
        // `min` and `max` refuse it because there is no answer to a comparison
        // with NaN. These do not compare, so NaN propagates as it does through
        // `+` written out, and the language already holds the value.
        assert_eq!(out("print(sum([float(\"nan\"), 1.0]))"), "nan\n");
        assert!(err("print(min(float(\"nan\"), 1.0))").contains("NaN"));
    }

    #[test]
    fn starts_with_and_ends_with_answer_about_a_prefix_and_a_suffix() {
        assert_eq!(
            out("print(starts_with(\"hello.miru\", \"hello\"), ends_with(\"hello.miru\", \".miru\"))"),
            "true true\n"
        );
        assert_eq!(
            out("print(starts_with(\"hello\", \"jelly\"), ends_with(\"hello\", \"jelly\"))"),
            "false false\n"
        );

        // The dull cases, which are the ones a caller trips over. An empty
        // needle is a prefix and a suffix of everything, including of nothing.
        assert_eq!(
            out("print(starts_with(\"a\", \"\"), ends_with(\"a\", \"\"), starts_with(\"\", \"\"))"),
            "true true true\n"
        );
        // A needle longer than the string cannot be either.
        assert_eq!(
            out("print(starts_with(\"ab\", \"abc\"), ends_with(\"ab\", \"zab\"))"),
            "false false\n"
        );
    }

    #[test]
    fn a_prefix_of_a_multi_byte_string_is_measured_the_same_as_a_character_one() {
        // The claim in the doc comment, checked rather than asserted in prose.
        // Both of these split inside what would be a byte prefix if the
        // comparison could stop part way through a character. `é` is two bytes
        // and the emoji is four.
        assert_eq!(
            out("print(starts_with(\"héllo\", \"hé\"), ends_with(\"héllo\", \"llo\"))"),
            "true true\n"
        );
        assert_eq!(
            out("print(starts_with(\"héllo\", \"h\\u{e8}\"), ends_with(\"a\\u{1F600}\", \"\\u{1F600}\"))"),
            "false true\n"
        );
    }

    #[test]
    fn starts_with_and_ends_with_refuse_an_argument_that_is_not_a_string() {
        assert!(err("print(starts_with(\"a\", 1))").contains("two string arguments"));
        assert!(err("print(ends_with(1, \"a\"))").contains("two string arguments"));
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
    fn insert_places_a_value_at_a_position() {
        assert_eq!(
            out("let a = [2, 3]\ninsert(a, 0, 1)\nprint(a)"),
            "[1, 2, 3]\n"
        );
        assert_eq!(
            out("let a = [1, 3]\ninsert(a, 1, 2)\nprint(a)"),
            "[1, 2, 3]\n"
        );
    }

    /// The one past-the-end position that means something: where a new last
    /// element goes. `insert(a, len(a), v)` is `push(a, v)`.
    #[test]
    fn insert_at_the_length_appends() {
        assert_eq!(
            out("let a = [1, 2]\ninsert(a, 2, 3)\nprint(a)"),
            "[1, 2, 3]\n"
        );
        assert_eq!(out("let a = []\ninsert(a, 0, 1)\nprint(a)"), "[1]\n");
    }

    /// **`insert` follows indexing, not `slice`.**
    ///
    /// `slice` clamps, because a range that asks for more than exists can
    /// sensibly give what exists. A position is not a range: an index past the
    /// end is arithmetic that went wrong, and appending quietly would hide it.
    #[test]
    fn insert_past_the_end_is_an_error_rather_than_an_append() {
        assert_eq!(
            err("insert([1], 99, 0)"),
            "index 99 is out of range for inserting into an array of length 1"
        );
        assert_eq!(
            err("insert([1], -1, 0)"),
            "index -1 is out of range (negative)"
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

#[cfg(test)]
mod count {
    use super::*;

    /// The pinned list and what `register` actually does are the same set.
    ///
    /// Two comments in this codebase carried a hand-counted total, and both
    /// drifted: one said forty and one said thirty-seven, and neither was
    /// checked by anything. The specification and the stability guarantee both
    /// promise behaviour for "every builtin", so the membership has to be a
    /// fact rather than a recollection.
    ///
    /// Checked in **both** directions on purpose. A name in the list but not
    /// registered is a promise the language does not keep. A builtin registered
    /// but not in the list is worse, because `tests/specification.rs` generates
    /// its checks from this list, so an omission here means the specification is
    /// never asked about that builtin at all.
    ///
    /// The length is not asserted against a literal. `BUILTIN_NAMES` declares
    /// its own size, so a literal here would be a second place to update and a
    /// third to get wrong: exactly the drift this test exists to stop.
    #[test]
    fn the_pinned_list_holds_every_builtin_that_is_registered() {
        let mut globals = Globals::new();
        register(&mut globals);
        for name in BUILTIN_NAMES {
            assert!(
                globals.contains(name),
                "'{name}' is listed but not registered"
            );
        }
        assert_eq!(
            globals.builtin_count(),
            BUILTIN_NAMES.len(),
            "register defines {} builtins but the list names {}",
            globals.builtin_count(),
            BUILTIN_NAMES.len()
        );
    }

    /// Hold the kind counts to the numbers written in prose about them.
    ///
    /// There is no single "number of builtins" that every sentence means. A
    /// claim about `BuiltinFn` counts the `define` calls, a claim about the
    /// caught-error guard counts everything reaching `call_native`, and a claim
    /// about the language counts all of them. Those were one figure until 1.1
    /// and are three now.
    ///
    /// 1.5 shows how little they move together: it added a builtin and the
    /// first of those three did not change, because `now` is not a `define`.
    ///
    /// This was nearly the cause of real damage: two comments saying
    /// "thirty-seven" were read as stale and lined up to be "corrected" to
    /// forty-four, which would have made two right sentences wrong. Counting
    /// first is what caught it, so the count is a test now.
    ///
    /// **When this fails, go and read the comments named below.** One of them
    /// has just become false. Changing the number here without reading them is
    /// how the drift gets back in.
    #[test]
    fn builtin_kind_counts_match_the_comments_that_quote_them() {
        let mut globals = Globals::new();
        register(&mut globals);

        let (mut plain, mut system, mut ambient, mut host) = (0, 0, 0, 0);
        for slot in 0..globals.builtin_count() {
            match globals.get(slot as u16) {
                Some(Value::Builtin(builtin)) => match builtin.func {
                    NativeFn::Plain(_) => plain += 1,
                    NativeFn::System(_) => system += 1,
                    NativeFn::Ambient(_) => ambient += 1,
                },
                Some(Value::HostBuiltin(_)) => host += 1,
                Some(other) => {
                    panic!(
                        "slot {slot} holds a {} rather than a builtin",
                        other.type_name()
                    )
                }
                None => panic!("slot {slot} is empty but is inside the builtin range"),
            }
        }

        assert_eq!(
            plain, 43,
            "{plain} builtins take a BuiltinFn, not 43. Quoted by `Args::into_vec` \
             above and by the trampoline section of docs/architecture.md."
        );
        assert_eq!(
            ambient, 12,
            "{ambient} builtins take an AmbientFn, not 12. This kind arrived in \
             1.5 and is quoted by `Args::into_vec` above."
        );
        assert_eq!(
            plain + system + ambient,
            59,
            "{} builtins reach `call_native`, not 59. Quoted by the caught-error \
             guard in `Vm::call_native`.",
            plain + system + ambient
        );
        assert_eq!(
            host, 4,
            "{host} builtins are higher-order, not 4. Quoted by the same two \
             places, which say the other twenty take a different signature."
        );
        assert_eq!(
            plain + system + ambient + host,
            BUILTIN_NAMES.len(),
            "the four kinds do not add up to the pinned list"
        );
    }
}

/// Every builtin, in registration order. Pinned so the count cannot drift and
/// so the specification has one list to be generated from rather than a second
/// hand-written one that can disagree.
pub const BUILTIN_NAMES: [&str; 63] = [
    "print",
    "eprint",
    "exit",
    "len",
    "push",
    "insert",
    "str",
    "type",
    "is_error",
    "range",
    "keys",
    "values",
    "has",
    "remove",
    "upper",
    "lower",
    "ord",
    "chr",
    "trim",
    "replace",
    "split",
    "join",
    "contains",
    "find",
    "starts_with",
    "ends_with",
    "pop",
    "index_of",
    "slice",
    "reverse",
    "abs",
    "min",
    "max",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    "sum",
    "product",
    "int",
    "float",
    "input",
    "read_file",
    "write_file",
    "file_exists",
    "args",
    "now",
    "sleep",
    "random",
    "random_int",
    "seed",
    "read_key",
    "key_ready",
    "clear",
    "move_to",
    "hide_cursor",
    "show_cursor",
    "term_size",
    "sort",
    "map",
    "filter",
    "reduce",
];
