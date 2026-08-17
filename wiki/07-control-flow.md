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

## match: branching on one value

An `else if` chain that keeps testing the same thing gets long, and nothing
checks that the arms are about the same value:

```
if pressed == "left" { x = x - 1 }
else if pressed == "right" { x = x + 1 }
else if pressed == "q" { running = false }
```

`match` names the value once:

```
match pressed {
  "left" { x = x - 1 }
  "right" { x = x + 1 }
  "q", "ctrl+c", "escape" { running = false }
  else { }
}
```

An arm can list several values, separated by commas, which is what the last one
above does. Arms are tried top to bottom and only one runs; there is no falling
through into the next.

### A guard on an arm

Put `if` after the cases to add a condition. The arm is taken only when both the
value matches and the guard is true, and when the guard is false the next arm
gets a turn:

```
match pressed {
  "left" if x > 0 { x = x - 1 }
  "left" { print("already at the edge") }
  else { }
}
```

### else, and what happens without it

`else` takes any value no other arm took. It goes last, and it takes no `if`,
because it is the arm that always matches.

**Without an `else`, a value no arm takes is an error:**

```
match 9 {
  1 { print("one") }
}
// no arm of this match takes 9
```

That is deliberate. A forgotten case is a mistake, and stopping is more useful
than carrying on as though nothing happened.

### match where a value goes

Like `if`, a `match` is also an expression, and its value is the arm's:

```
let name = match code {
  1 { "one" }
  2, 3 { "a few" }
  else { "many" }
}
```

In that form each arm holds one expression rather than a block, the same rule
`if` follows.

`break` and `continue` inside a `match` belong to the loop around it. A `match`
is not a loop, and it needs no `break` of its own, because arms do not fall
through.

---
Previous: [Operators](06-operators.md) | Next: [Loops](08-loops.md)
