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

A long number is easier to read in groups, so `_` is allowed between digits:

```
let budget = 1_000_000
let rate = 1.000_5
```

The underscore is only a mark for you. `1_000` and `1000` are the same number,
and `miru fmt` writes the shorter one, in the same way it writes `1.5` for
`1.50`. It has to sit between two digits, so `1_` and `1__0` are errors. A
name can still start with one: `_1` is a variable, not a number.

## Booleans

Just `true` and `false`.

## Strings

Text in double quotes. Strings support escape sequences such as `\n` (newline),
`\t` (tab), `\"` (a quote), and `\\` (a backslash):

```
print("line one\nline two")
```

For a character you cannot type, write `\u{...}` with its value in hexadecimal:

```
print("\u{41}")       // A
print("\u{1F600}")    // an emoji
print(len("\u{1F600}"))   // 1, because len counts characters
```

One to six digits, in either case. The value has to be a real character: the
largest is `10FFFF`, and `D800` to `DFFF` are reserved and are not characters,
so an escape naming one of those is an error rather than a stray value in your
string.

Note that `miru fmt` writes the character rather than the escape you typed, in
the same way it writes `1.5` for `1.50`. What a string holds is characters, and
`\u{...}` is one way to write them down.

Join strings with `+`:

```
print("Miru" + "ScriptX")   // MiruScriptX
```

Build a message with an `f` in front of the quotes, and put a name in braces:

```
let name = "Aiko"
let score = 12
print(f"{name} scored {score}")   // Aiko scored 12
```

That saves the `str` calls a `+` chain needs, and a forgotten one is a runtime
error rather than a mistake anybody meant to make. Only a name goes in the
braces (not an expression), and `{{` writes a literal brace.

**A plain string is untouched.** `"${n}"` prints the braces, exactly as it
always has, which is why interpolation needs the `f`.

Reach a single character with square brackets, counting from zero:

```
let word = "hello"
print(word[0], word[4])   // h o
```

There is no character type, so `word[0]` is a string one character long. And
since a string is measured in characters rather than bytes, so is the index.
`"a\u{1F600}b"[1]` is the whole emoji, not a piece of it.

An index past the end is an error naming the length, rather than `nil`:

```
print(word[9])   // index 9 is out of range for a string of length 5
```

That is deliberate. A missing map key gives `nil` already, so if this did too
you could not tell "there is no such character" from "there is one, and it is
nothing". A negative index is an error as well, so `word[-1]` does not mean the
last character.

Reading a character is all you can do: `word[0] = "H"` is an error. Build a
new string instead, with `+` or `slice`.

## Arrays

An ordered list of values, written with square brackets. An array can hold any
mix of types:

```
let things = [1, "two", true, [3, 4]]
```

Arrays get their own page: [Arrays](09-arrays.md).

---
Previous: [Variables](04-variables.md) | Next: [Operators](06-operators.md)
