# Functions

## Defining and calling

Declare a function with `fn`, then call it by name:

```
fn square(n) {
  return n * n
}

print(square(5))   // 25
```

## Parameters and return

A function can take several parameters. `return` hands a value back to the
caller and stops the function early:

```
fn max(a, b) {
  if a > b {
    return a
  }
  return b
}

print(max(3, 9))   // 9
```

A function with no `return`, or a bare `return`, produces `nil`. Calling a
function with the wrong number of arguments is an error, and the error says
what the function wanted:

```
function max expects 2 arguments but received 1
```

## Parameters that fill themselves in

Give a parameter a default and a caller can leave it out:

```
fn greet(name, greeting = "Hello") {
  return greeting + ", " + name + "!"
}

print(greet("Aiko"))         // Hello, Aiko!
print(greet("Ken", "Hi"))    // Hi, Ken!
```

Defaults go after the parameters that have none, because a call matches its
arguments by position and there would otherwise be no way to tell which one you
meant.

**A default is worked out afresh on every call that leaves it out**, which is
worth knowing for two cases. A default that reads the clock gives the time of
the call rather than the time the program started. And a default that is a new
array is a new array each time, so this prints `[1] [1]` rather than
`[1] [1, 1]`:

```
fn collect(into = []) {
  push(into, 1)
  return into
}
print(collect(), collect())
```

A default can use a parameter written before it, which is often what you want:

```
fn span(from, to = from + 10) {
  return to - from
}
print(span(1))   // 10
```

## Taking any number of arguments

Put `...` before the last parameter and it collects everything else into an
array:

```
fn log_all(prefix, ...rest) {
  for m in rest {
    print(prefix + m)
  }
}

log_all("> ", "a", "b")   // > a, then > b
log_all("> ")             // nothing: rest is []
```

`rest` is an ordinary array, so `len`, indexing and a `for` loop all work on it.
Nothing can come after it, since it takes everything that is left.

This is how `min`, `max` and `print` have always worked. Until version 2 a
function you wrote could not, and the usual way round it was one array
parameter with the caller writing the brackets.

## Recursion

A function can call itself. Here is factorial:

```
fn factorial(n) {
  if n <= 1 {
    return 1
  }
  return n * factorial(n - 1)
}

print(factorial(5))   // 120
```

---
Previous: [Maps](10-maps.md) | Next: [Closures](12-closures.md)
