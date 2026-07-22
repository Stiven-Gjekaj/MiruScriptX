# Data types

MiruScriptX has a small set of built-in types. Use the `type` builtin to ask
what something is:

```
print(type(42))     // int
print(type(3.14))   // float
print(type(true))   // bool
print(type("hi"))   // string
print(type([1, 2])) // array
print(type(nil))    // nil
```

## Numbers

There are two numeric types: integers (`int`, a 64-bit whole number) and floats
(`float`). A number written with a decimal point is a float:

```
let whole = 10
let ratio = 2.5
```

When you mix them in arithmetic, the result is a float. See
[Operators](06-operators.md) for the details.

## Booleans

Just `true` and `false`.

## Strings

Text in double quotes. Strings support escape sequences such as `\n` (newline),
`\t` (tab), `\"` (a quote), and `\\` (a backslash):

```
print("line one\nline two")
```

Join strings with `+`:

```
print("Miru" + "ScriptX")   // MiruScriptX
```

## Arrays

An ordered list of values, written with square brackets. An array can hold any
mix of types:

```
let things = [1, "two", true, [3, 4]]
```

Arrays get their own page: [Arrays](09-arrays.md).

---
Previous: [Variables](04-variables.md) | Next: [Operators](06-operators.md)
