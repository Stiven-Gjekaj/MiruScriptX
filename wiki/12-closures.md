# Closures

Functions in MiruScriptX are values. You can store them in variables, pass them
to other functions, and return them from functions.

## Functions as values

An anonymous function is written `fn(params) { ... }`, with no name:

```
let double = fn(x) {
  return x * 2
}

print(double(21))   // 42
```

## Passing a function

Because functions are values, you can pass one in as an argument:

```
fn apply_twice(f, x) {
  return f(f(x))
}

fn inc(n) {
  return n + 1
}

print(apply_twice(inc, 10))   // 12
```

## Capturing the environment

A function remembers the variables that were in scope where it was created. That
is what makes it a closure:

```
fn make_adder(x) {
  return fn(y) {
    return x + y
  }
}

let add5 = make_adder(5)
print(add5(3))    // 8
print(add5(10))   // 15
```

`add5` keeps its own `x` of 5, even though `make_adder` has already returned.

---
Previous: [Functions](11-functions.md) | Next: [Builtins](13-builtins.md)
