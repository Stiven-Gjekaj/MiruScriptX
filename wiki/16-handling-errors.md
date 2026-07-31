# Handling errors

The previous lesson showed what an error looks like. Every one of them stops the
program. That is the right default, and it is not always what you want: a
program that reads input, opens files, or converts text has to survive the day
its input is not what it expected.

`try` is how a program says "this might fail, and I will deal with it".

## try

Put `try` in front of an expression. If it fails, the error becomes the value
of that expression instead of ending the program:

```
let r = try 10 / 0
print(type(r))   // error
```

If nothing fails, `try` does nothing at all:

```
let n = try 6 * 7
print(n)   // 42
```

So `try` marks the places that can fail, and everywhere without it behaves
exactly as it did before.

## Checking

`is_error` answers whether you are holding an error:

```
fn average(row) {
  let total = reduce(row, fn(acc, n) { return acc + n }, 0)
  return total / len(row)
}

let result = try average([])
if is_error(result) {
  print("could not average that:", result.message)
} else {
  print("average:", result)
}
```

`type(result) == "error"` says the same thing. `is_error` is the one to reach
for, because a misspelled string comparison quietly answers `false` forever.

## What an error knows

An error carries five things, read with a dot:

| Field | What it holds |
| ----- | ------------- |
| `message` | What went wrong, as a string |
| `line` | The line it happened on |
| `column` | The column it happened at |
| `file` | The module it came from, or `nil` for the file being run |
| `trace` | The calls it came through, as an array of strings |

`trace` is the useful one. An error remembers the path it came through, so you
can tell where it came from and not just that it happened:

```
fn half(n) { return n / 0 }
fn apply(x) { return half(x) }

let r = try apply(4)
print(r.trace)   // ["in half, called from line 2", "in apply, called from line 3"]
```

A name that is not one of the five is an error rather than `nil`, the same
bargain field access makes everywhere else, so a misspelling fails where it is
written.

## An error is not an ordinary value

You may check an error, ask its type, and read its fields. Anything else stops
the program:

```
let count = try 10 / 0
print(count + 1)
```

```
error (line 2, column 13): unhandled error: division by zero
    print(count + 1)
                ^
```

The message names the original error, and the position is where you *used* it,
because that is the mistake: the program had the error in hand and did
something else with it. The error's own position is still on the value, in
`.line` and `.column`.

This is deliberate. The usual complaint about errors as values is that they get
ignored, flow onward as data, and surface somewhere unrelated. Here they cannot:
either you check one or the program stops at the line that misused it.

For the same reason an error is *not* falsy. `if r { .. }` does not mean "if it
worked", because a successful `0`, `false`, `nil`, or `""` would be
indistinguishable from an error. Ask with `is_error`.

## What try cannot catch

Two things refuse to become a value.

**The call depth limit.**

```
fn boom(n) { return boom(n + 1) }
let r = try boom(0)
```

```
error (line 1, column 21): call depth limit of 10000 exceeded
```

Runaway recursion is a bug in the program rather than a condition to recover
from, and a `try` that swallowed it would hide the only thing worth knowing.

**A call to `exit`.**

```
let r = try exit(3)
print("never reached", r)
```

The program stops with code 3 and the second line never runs. A program that
calls `exit` has finished, and a `try` that caught one would let it carry on
with a code its caller is going to be told about but that no longer describes
what happened.

A refused code is different. `exit(999)` never stops anything, because the code
is out of range, so it stays an ordinary error that `try` catches like any
other:

```
print(is_error(try exit(999)))   // true
```

## How much try covers

`try` takes the whole expression after it, not just the next thing:

```
let a = try 10 / 0 + 5    // the division is covered
let b = (try 10) / 0      // it is not; this stops the program
```

Parentheses narrow it when that is what you mean.

## A worked example

[examples/recover.miru](../examples/recover.miru) averages four rows, one of
which is empty. Run it with `miru run examples/recover.miru`:

```
average: 6
skipping a row: division by zero
average: 15
average: 7
rows handled: 3 of 4
```

Without `try`, that program prints one line and stops.

---
Previous: [When a program stops](15-errors.md) | Next: [Next steps](17-next-steps.md)
