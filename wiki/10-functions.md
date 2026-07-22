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
function with the wrong number of arguments is an error.

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
Previous: [Arrays](09-arrays.md) | Next: [Closures](11-closures.md)
