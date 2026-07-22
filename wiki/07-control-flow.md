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
