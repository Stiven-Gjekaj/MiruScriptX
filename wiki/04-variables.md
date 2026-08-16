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

## Changing a variable in place

Writing the name twice gets old, and it is somewhere the two halves can drift
apart: `ball_x = ball_y + 1` is a bug that reads like working code. The five
arithmetic operators have a shorter form:

```
let count = 3
count += 1    // the same as count = count + 1
count -= 2
count *= 10
count /= 5
count %= 3
print(count)  // 2
```

They work on an element or a field too:

```
let scores = [10, 20]
scores[0] += 5

let player = {"hp": 100}
player.hp -= 30
```

**The target is worked out once.** If the index is a function call, that call
happens a single time:

```
a[next()] += 1   // next() is called once, not twice
```

That is the one thing the short form guarantees which writing it out by hand
does not, and it is why `a[next()] += 1` is not just a shorter way to type
`a[next()] = a[next()] + 1`, because the long version calls `next` twice and would
read one element while writing another.

These are statements, so you cannot use one as a value. `let y = (x += 1)` is
an error.

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
