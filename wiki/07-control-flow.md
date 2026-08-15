# Control flow

## if and else

`if` runs a block when its condition is true. Conditions are not wrapped in
parentheses, and the block braces are required:

```
let temp = 30
if temp > 25 {
  print("warm")
} else {
  print("cool")
}
```

## else if

Chain conditions with `else if`. Keep `else` on the same line as the closing
brace of the block before it:

```
let score = 82
if score >= 90 {
  print("A")
} else if score >= 80 {
  print("B")
} else {
  print("C")
}
```

## Choosing a value

An `if` can also stand where a value goes, and then it gives you the arm that
runs:

```
let step = if target > 3 { "right" } else { "left" }
```

Without it, that takes four lines and a name you have to change afterwards:

```
let step = "left"
if target > 3 {
  step = "right"
}
```

**Used this way it needs an `else`**, because there has to be a value either
way. Without one you get an error rather than a surprise `nil`:

```
let step = if target > 3 { "right" }    // error: an 'if' used as a value
                                        // needs an 'else'
```

**Each arm holds one expression**, not a block of statements. That covers what
this form is for; when you want several statements, use the `if` statement
above.

Chains work, and read as one choice:

```
let size = if n > 100 { "huge" } else if n > 3 { "big" } else { "small" }
```

It fits anywhere a value fits, which is what makes it worth having:

```
out += if living(cells, x, y) { "#" } else { "." }
print(if score > best { "record" } else { "keep going" })
```

## while

`while` repeats a block as long as its condition stays true:

```
let n = 3
while n > 0 {
  print(n)
  n = n - 1
}
```

Output:

```
3
2
1
```

---
Previous: [Operators](06-operators.md) | Next: [Loops](08-loops.md)
