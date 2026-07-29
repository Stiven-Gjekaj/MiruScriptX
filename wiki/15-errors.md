# When a program stops

When something goes wrong, MiruScriptX stops and prints a single, precise error.
Every error names the line and column where it happened and underlines (`^`) the
exact piece of your source it blames, so you can find the problem quickly. The
underline covers the whole of whatever it is pointing at, so a one-character
operator gets one mark and a name gets one per character.

## Syntax errors

A syntax error means the source could not be parsed. It is caught before any code
runs.

```
print(1 +)
```

Running that reports:

```
error (line 1, column 10): expected an expression but found ')'
    print(1 +)
             ^
```

The caret points at the `)` that appeared where an expression was expected.

## Runtime errors

A runtime error happens while the program runs: dividing by zero, using a name
that was never defined, or indexing past the end of an array. These carry a caret
too, pointing at the expression that failed.

```
let xs = [1, 2, 3]
print(xs[7])
```

```
error (line 2, column 10): index 7 is out of range for an array of length 3
    print(xs[7])
             ^
```

Here the underline sits under the index `7`. A division by zero marks the
operator, which is one character wide:

```
error (line 1, column 3): division by zero
    1 / 0
      ^
```

An undefined variable marks the name, and there the width does some work: you
can see which name is the problem without counting columns.

```
let total = 0
print(subtotal)
```

```
error (line 2, column 7): undefined variable 'subtotal'
    print(subtotal)
          ^^^^^^^^
```

## Where an error came from

An error inside a function also shows the path of calls that reached it, so you
can see not just where the program broke but how it got there.

```
fn double(n) {
  return n * 2
}
fn total(xs) {
  let sum = 0
  for x in xs {
    sum = sum + double(x)
  }
  return sum
}
print(total([1, 2, nil]))
```

```
error (line 2, column 12): cannot multiply a nil and a int
      return n * 2
               ^
  in double, called from line 7
  in total, called from line 11
```

Read it from the top down. The caret says the multiplication failed because `n`
was `nil`. The trace then says `double` was called from line 7, inside `total`,
and `total` was called from line 11. The `nil` came from the array on that last
line, which is the thing to fix.

Each line names a function and the line its call was written on, innermost
first. A program that fails outside any function has no trace, because there is
no call path to report.

Very deep traces are shortened in the middle rather than printed in full, so
runaway recursion reports its error rather than burying it under ten thousand
identical lines.

## Exit codes

Run a file with `miru run` and a successful program exits with status 0. Any
error (a missing file, a syntax error, or a runtime error) exits with a non-zero
status, so scripts and continuous integration can tell success from failure.

---
Previous: [Modules](14-modules.md) | Next: [Handling errors](16-handling-errors.md)
