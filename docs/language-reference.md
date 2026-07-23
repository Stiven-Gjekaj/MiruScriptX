# MiruScriptX Language Reference

This single page is generated from the wiki/ learning stages by
scripts/build_reference.sh. It gathers every lesson in one place so you
can search the whole language at once. To change it, edit the files in
wiki/ and re-run the script.

## Contents

- [Introduction](#introduction)
- [Getting started](#getting-started)
- [Syntax basics](#syntax-basics)
- [Variables](#variables)
- [Data types](#data-types)
- [Operators](#operators)
- [Control flow](#control-flow)
- [Loops](#loops)
- [Arrays](#arrays)
- [Maps](#maps)
- [Functions](#functions)
- [Closures](#closures)
- [Builtins](#builtins)
- [Next steps](#next-steps)

# Introduction

MiruScriptX is a small, general-purpose scripting language. If you have written
a little JavaScript, Python, or Lua, it will feel familiar: dynamic types, a
clean syntax, first-class functions, and arrays.

Programs live in files with a `.msx` extension and run through a tree-walking
interpreter written in Rust.

## Hello, world

```
print("Hello, world!")
```

Output:

```
Hello, world!
```

That is a complete program. `print` is a builtin that writes its arguments
followed by a newline.

## What you will learn

These pages are meant to be read in order, like short lessons. By the end you
will know the whole language: values, variables, operators, control flow,
functions and closures, arrays, and every builtin.

If you prefer a single page you can search, see the
[language reference](../docs/language-reference.md).


# Getting started

## Build the interpreter

You need a recent Rust toolchain. From the project root:

```
cargo build --release
```

This produces the `miru` binary at `target/release/miru`.

## Run a program

Put some code in a file, for example `hello.msx`:

```
print("Hello from a file!")
```

Then run it:

```
miru run hello.msx
```

## Use the REPL

Run `miru` with no arguments to start an interactive session:

```
miru
```

Type an expression and press Enter to see its value:

```
miru> 2 + 3 * 4
14
miru> let name = "Miru"
miru> "Hello, " + name
"Hello, Miru"
```

Definitions stay available for the rest of the session. To type something that
spans several lines, such as a function, just keep typing: the prompt changes to
`...` until the brackets are balanced. Press Ctrl-D to exit.


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


# Data types

MiruScriptX has a small set of built-in types. Use the `type` builtin to ask
what something is:

```
print(type(42))     // int
print(type(3.14))   // float
print(type(true))   // bool
print(type("hi"))   // string
print(type([1, 2])) // array
print(type(nil))    // nil
```

## Numbers

There are two numeric types: integers (`int`, a 64-bit whole number) and floats
(`float`). A number written with a decimal point is a float:

```
let whole = 10
let ratio = 2.5
```

When you mix them in arithmetic, the result is a float. See
[Operators](06-operators.md) for the details.

## Booleans

Just `true` and `false`.

## Strings

Text in double quotes. Strings support escape sequences such as `\n` (newline),
`\t` (tab), `\"` (a quote), and `\\` (a backslash):

```
print("line one\nline two")
```

Join strings with `+`:

```
print("Miru" + "ScriptX")   // MiruScriptX
```

## Arrays

An ordered list of values, written with square brackets. An array can hold any
mix of types:

```
let things = [1, "two", true, [3, 4]]
```

Arrays get their own page: [Arrays](09-arrays.md).


# Operators

## Arithmetic

```
print(7 + 2)    // 9
print(7 - 2)    // 5
print(7 * 2)    // 14
print(7 / 2)    // 3     integer division truncates
print(7 % 2)    // 1     remainder
print(7.0 / 2)  // 3.5   a float operand gives float division
```

Two integers produce an integer. If either operand is a float, the result is a
float. Dividing or taking the remainder by zero is an error.

## Comparison

Comparisons produce a boolean:

```
print(1 < 2)      // true
print(2 <= 2)     // true
print(3 > 5)      // false
print(1 == 1.0)   // true    numbers compare across int and float
print("a" == "a") // true
print(1 != 2)     // true
```

Strings compare in dictionary order, so `"apple" < "banana"` is true.

## Logical

`&&` (and), `||` (or), and `!` (not). Both `&&` and `||` short-circuit, meaning
the right side is only evaluated when it is needed:

```
print(true && false) // false
print(false || true) // true
print(!true)         // false
```

## String concatenation

`+` joins two strings. To join a string with a number, convert the number first
with `str`:

```
let n = 3
print("you have " + str(n) + " messages")
```

## Precedence

From loosest to tightest: `||`, then `&&`, then the comparisons, then `+` and
`-`, then `*`, `/`, and `%`, then the prefix operators `-` and `!`, then calls
and indexing. Use parentheses when in doubt:

```
print(2 + 3 * 4)   // 14
print((2 + 3) * 4) // 20
```


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


# Loops

## for ... in

A `for` loop walks over the items of an array, binding each one to a name in
turn:

```
let colors = ["red", "green", "blue"]
for c in colors {
  print(c)
}
```

## Counting with range

`range` builds an array of integers, which is the usual way to count. With one
argument it starts at 0; with two it starts at the first:

```
for i in range(3) {
  print(i)          // 0, then 1, then 2
}

for i in range(2, 5) {
  print(i)          // 2, then 3, then 4
}
```

The end value is never included, so `range(0, n)` gives you exactly `n` numbers.

## A worked example

Sum the numbers from 1 to 10:

```
let total = 0
for i in range(1, 11) {
  total = total + i
}
print(total)   // 55
```

## Loop control: break and continue

Use `break` to stop a loop early, and `continue` to skip to the next iteration:

```
for n in range(1, 10) {
  if n == 5 { break }        // stop the loop entirely
  if n % 2 == 0 { continue } // skip the even numbers
  print(n)                   // 1, then 3
}
```

Both work in `while` loops too, and they always affect the nearest enclosing
loop.


# Arrays

An array is an ordered list of values.

## Creating and reading

```
let fruits = ["apple", "banana", "cherry"]
print(fruits[0])   // apple
print(fruits[2])   // cherry
```

Indexing is zero-based. Reading past the end, or with a negative index, is an
error.

## Changing an element

Assign to an index to replace an element in place:

```
let nums = [1, 2, 3]
nums[1] = 20
print(nums)   // [1, 20, 3]
```

## Length

`len` returns how many items an array has (and it also works on strings):

```
print(len([10, 20, 30]))   // 3
print(len("hello"))        // 5
```

## Growing an array

`push` adds an item to the end and returns the array:

```
let stack = []
push(stack, 1)
push(stack, 2)
print(stack)   // [1, 2]
```

## Iterating

Combine arrays with a `for` loop to process every item:

```
let names = ["Aiko", "Ken"]
for n in names {
  print("Hello, " + n)
}
```


# Maps

A map (also called a dictionary) holds a set of values, each stored under a
string key. Maps are written with curly braces.

## Creating and reading

```
let person = {"name": "Aiko", "age": 3}
print(person["name"])   // Aiko
print(person["age"])    // 3
```

Keys are strings. Reading a key that is not present gives `nil`:

```
print(person["email"])   // nil
```

## Adding and updating

Assign to a key to insert it, or to change a value that is already there:

```
let scores = {}
scores["ken"] = 10
scores["ken"] = 12
print(scores)   // {"ken": 12}
```

## Checking and counting

Use `has` to test for a key and `len` for the number of entries:

```
let m = {"a": 1, "b": 2}
print(has(m, "a"))   // true
print(has(m, "z"))   // false
print(len(m))        // 2
```

## Going over a map

Get the keys or values as arrays with `keys` and `values`, then loop:

```
let ages = {"Aiko": 3, "Ken": 5}
for name in keys(ages) {
  print(name + " is " + str(ages[name]))
}
```

Keys always come back in sorted order, so the output is stable:

```
Aiko is 3
Ken is 5
```


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


# Builtins

These functions are always available. The set is small but grows with each
release.

## print(...)

Writes its arguments separated by single spaces, then a newline. Returns `nil`.

```
print("x is", 10, "and done")   // x is 10 and done
```

## len(value)

Returns the number of items in an array, or the number of characters in a
string.

```
print(len([1, 2, 3]))   // 3
print(len("hello"))     // 5
```

## push(array, value)

Appends `value` to the end of `array`, changing it in place, and returns the
array.

```
let a = [1]
push(a, 2)
print(a)   // [1, 2]
```

## str(value)

Converts any value to its display string. Useful for building messages.

```
print("total: " + str(42))   // total: 42
```

## type(value)

Returns the name of a value's type, one of `int`, `float`, `bool`, `string`,
`array`, `function`, or `nil`.

```
print(type(3.14))   // float
```

## range(end) or range(start, end)

Returns an array of integers. With one argument it counts from 0; with two it
counts from `start`. The `end` value is never included.

```
print(range(4))      // [0, 1, 2, 3]
print(range(2, 6))   // [2, 3, 4, 5]
```

## keys(map)

Returns an array of the map's keys, in sorted order.

```
print(keys({"b": 2, "a": 1}))   // ["a", "b"]
```

## values(map)

Returns an array of the map's values, in key order.

```
print(values({"b": 2, "a": 1}))   // [1, 2]
```

## has(map, key)

Reports whether the map contains a given string key.

```
let m = {"a": 1}
print(has(m, "a"))   // true
print(has(m, "z"))   // false
```

More builtins are planned; see the [roadmap](../docs/milestones.md).


# Next steps

You now know all of MiruScriptX v0.1. Here is where to go from here.

## Practice

Try writing a few small programs:

- Print the even numbers from 1 to 20.
- Write a function that reverses an array into a new array.
- Compute the greatest common divisor of two numbers with a `while` loop.

The programs in the [examples](../examples) folder are a good starting point.
Run them with `miru run examples/greet.msx`, and so on.

## Look things up

For a single searchable page covering everything in this wiki, see the
[language reference](../docs/language-reference.md).

## See how it works

Curious how the interpreter is built? The
[architecture guide](../docs/architecture.md) walks through the lexer, parser,
and evaluator.

## What is coming

The [roadmap](../docs/milestones.md) lists what is planned next: maps, more
builtins, a bytecode virtual machine, and a browser playground.


