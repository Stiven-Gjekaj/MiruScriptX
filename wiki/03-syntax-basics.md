# Syntax basics

## Statements and lines

A program is a list of statements. Each statement ends at the end of the line,
so you do not need semicolons. If you like, you can still separate statements
with a semicolon or put several on one line:

```
print("a")
print("b"); print("c")
```

## Comments

Anything after `//` on a line is a comment and is ignored:

```
// This whole line is a comment.
print("hi")  // and this part too
```

## Printing

`print` writes its arguments separated by spaces, then a newline:

```
print("the answer is", 42)
```

Output:

```
the answer is 42
```

## Blocks

Curly braces group statements into a block, used by `if`, `while`, `for`, and
functions:

```
if true {
  print("inside a block")
}
```

A long expression may span several lines when it sits inside parentheses or
brackets:

```
let total = (1 +
             2 +
             3)
print(total)   // 6
```

---
Previous: [Getting started](02-getting-started.md) | Next: [Variables](04-variables.md)
