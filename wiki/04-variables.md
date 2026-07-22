# Variables

## Declaring

Use `let` to introduce a variable:

```
let greeting = "hello"
let count = 3
```

## Reassigning

Assign to an existing variable without `let`:

```
let count = 3
count = count + 1
print(count)   // 4
```

Assigning to a name that was never declared is an error, which helps catch
typos early.

## nil

`nil` is the value that means "nothing". A function that does not return a value
produces `nil`:

```
let result = nil
print(result)   // nil
```

## Truthiness

Only `false` and `nil` count as false for `if`, `while`, and the logical
operators. Everything else, including `0` and the empty string, counts as true.

---
Previous: [Syntax basics](03-syntax-basics.md) | Next: [Data types](05-data-types.md)
