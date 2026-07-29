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
- [Modules](#modules)
- [When a program stops](#when-a-program-stops)
- [Handling errors](#handling-errors)
- [Next steps](#next-steps)

# Introduction

MiruScriptX is a small, general-purpose scripting language. If you have written
a little JavaScript, Python, or Lua, it will feel familiar: dynamic types, a
clean syntax, first-class functions, and arrays.

Programs live in files with a `.miru` extension. They are compiled to bytecode
and run on a stack virtual machine, all written in Rust from scratch.

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
functions and closures, arrays, every builtin, and how to split a program
across more than one file.

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

Put some code in a file, for example `hello.miru`:

```
print("Hello from a file!")
```

Then run it:

```
miru run hello.miru
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
`...` until the brackets are balanced.

Use the up and down arrow keys to recall earlier lines; the history is saved to
`~/.miru_history` and comes back the next time you start the REPL. Press Ctrl-C
to cancel the current line, and Ctrl-D to exit.

## Format your code

`miru fmt` reprints a program in one canonical style: two-space indentation,
consistent spacing, and one statement per line. By default it writes the result
to standard output:

```
miru fmt hello.miru
```

Add `-w` (or `--write`) to reformat the file in place:

```
miru fmt -w hello.miru
```

Comments and single blank lines between sections are kept.

## How your program runs

`miru run` compiles your program to bytecode and executes it on a virtual
machine. You do not have to do anything to get this; it is simply how the
language runs.

This is the only way programs run: there is no engine to choose between.

## See the bytecode

You never need this to write MiruScriptX, but it is the best way to see what
the language is actually doing. `miru disasm` prints the instructions a program
compiles to:

```
miru disasm hello.miru
```

For a one-line `print("Hello, world!")` that reads:

```
== script ==
   1  0000 GET_GLOBAL    slot 0
   |  0003 CONSTANT      0 ("Hello, world!")
   |  0005 CALL          1
   |  0007 RETURN
```

Read it as a stack machine. Push `print`, push the string, call it with one
argument, return what it gave back. The number on the left is the source line,
and a bar means the instruction continues the line above. Functions you define
are printed after the script, each under its own heading.


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
[Operators](#operators) for the details.

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

Arrays get their own page: [Arrays](#arrays).


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
`array`, `map`, `function`, `nil`, or `error`.

```
print(type(3.14))   // float
```

## is_error(value)

Whether the value is an error caught by `try`. See
[Handling errors](#handling-errors).

```
print(is_error(try 1 / 0))   // true
print(is_error(42))          // false
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

## String functions

- `upper(s)` and `lower(s)` change the case of every letter.
- `trim(s)` removes leading and trailing whitespace.
- `replace(s, from, to)` replaces every occurrence of `from` with `to`.
- `split(s, sep)` breaks a string into an array of pieces; an empty separator
  splits it into single characters.
- `join(array, sep)` joins an array's displayed elements with `sep`.
- `contains(seq, value)` reports whether a string holds a substring, or an array
  holds an element.
- `find(s, sub)` returns the character index of the first `sub`, or -1.

```
print(upper("hi"), lower("HI"))     // HI hi
print(trim("  hi  "))               // hi
print(replace("a.b.c", ".", "-"))   // a-b-c
print(split("a,b,c", ","))          // ["a", "b", "c"]
print(join(["a", "b", "c"], "-"))   // a-b-c
print(contains("hello", "ell"))     // true
print(find("hello", "l"))           // 2
```

## Array functions

- `pop(array)` removes and returns the last element.
- `index_of(array, value)` returns the index of the first match, or -1.
- `slice(seq, start, end)` returns the half-open slice of an array or string.
- `sort(array)` returns a sorted copy (all numbers or all strings).
- `reverse(seq)` returns a reversed copy of an array or string.

```
let xs = [3, 1, 2]
print(sort(xs))                // [1, 2, 3]
print(reverse(xs))             // [2, 1, 3]
print(slice(xs, 0, 2))         // [3, 1]
print(index_of([10, 20], 20))  // 1
```

## Math functions

- `abs(x)` is the absolute value.
- `min(...)` and `max(...)` take any number of numeric arguments.
- `floor(x)`, `ceil(x)`, and `round(x)` return integers.
- `sqrt(x)` is the square root (a float); `pow(base, exp)` raises to a power.

```
print(abs(-3), min(3, 1, 2), max(3, 1, 2))   // 3 1 3
print(floor(2.7), ceil(2.1), round(2.5))     // 2 3 3
print(sqrt(9), pow(2, 10))                    // 3.0 1024
```

## Conversion

- `int(x)` converts a float (truncating toward zero) or a numeric string to an
  integer.
- `float(x)` converts an integer or a numeric string to a float.

```
print(int("42"), int(2.9))      // 42 2
print(float("1.5"), float(3))   // 1.5 3.0
```

## input(prompt)

Reads one line from standard input and returns it as a string, without the
trailing newline. With a `prompt` argument, the prompt is written first (with no
newline). At end of input it returns `nil`.

```
let name = input("What is your name? ")
print("Hello,", name)
```

## Higher-order functions

These apply a function across an array. The function can be a named function, a
closure, or another builtin.

- `map(array, f)` returns a new array of `f(x)` for each element.
- `filter(array, f)` returns a new array of the elements for which `f(x)` is
  truthy.
- `reduce(array, f, init)` folds the array from the left: it starts from `init`
  and combines each element with `f(acc, x)`, returning the final accumulator.

```
print(map([1, 2, 3], fn(x) { return x * 2 }))                 // [2, 4, 6]
print(filter([1, 2, 3, 4], fn(x) { return x % 2 == 0 }))      // [2, 4]
print(reduce([1, 2, 3, 4], fn(acc, x) { return acc + x }, 0)) // 10
```

The standard library stays small on purpose; see the
[roadmap](../docs/milestones.md) for what is planned next.


# Modules

A program can be more than one file. `import` runs another file and binds
everything that file defines to a name of your choosing.

## Importing a file

```
import "./prices.miru" as prices

print(prices.tax_percent)   // 8
```

The path is relative to the file doing the importing, not to the directory you
ran `miru` from. `examples/shop.miru` imports `"./prices.miru"` and finds
`examples/prices.miru` wherever you run it from.

The alias is required. `import "./prices.miru"` on its own is a syntax error,
because there would be no name to reach the module through.

## What a module gives you

Every name defined at the top level of a file is reachable through the alias.
There is nothing to write to make a name public.

`prices.miru`:

```
let tax_percent = 8

fn with_tax(amount) {
  return amount + amount * tax_percent / 100
}
```

`shop.miru`:

```
import "./prices.miru" as prices

print(prices.with_tax(1300))   // 1404
```

The other side of that is that a module cannot keep a helper to itself. Every
top-level name is visible to whoever imports the file.

## Names belong to their file

Two files can use the same name without either one noticing:

```
import "./prices.miru" as prices

// `subtotal` is a function over in the module and a number here.
let subtotal = prices.subtotal(cart)
print(subtotal)
```

Before modules there was one table of names for the whole program, so this was
impossible to write. Now each file has its own.

## A module is a map

The alias holds an ordinary map, so anything that works on a map works here:

```
import "./prices.miru" as prices

print(keys(prices))            // ["subtotal", "tax_percent", "with_tax"]
print(prices["tax_percent"])   // 8
```

The two ways of reaching a name differ in what they do when the name is not
there. `prices.nope` is an error that points at the line, and `prices["nope"]`
is `nil`, the same as any other map lookup. Reach for the dot when a typo
should stop the program.

## Each file runs once

The first `import` of a file runs it from top to bottom. Later imports of the
same file, from anywhere in the program, get the same result without running it
again.

`shared.miru`:

```
print("loading")
let n = 7
```

If three different files import `shared.miru`, `loading` prints once. Two
spellings of one path, `./shared.miru` and `./sub/../shared.miru`, are the same
file and share the one result.

## Imports cannot form a cycle

If `aa.miru` imports `bb.miru` and `bb.miru` imports `aa.miru`, there is no
order in which either one can finish. MiruScriptX says so instead of looping:

```
error (./aa.miru, line 1, column 8): import cycle: ./bb.miru -> ./aa.miru -> ./bb.miru
```

The chain is printed in the order the files were reached, so you can see the
loop rather than guess at it.

## Where an import can appear

Only at the top level of a file. Inside an `if`, a loop, or a function it is an
error:

```
fn load() {
  import "./prices.miru" as prices
}
```

```
error (line 2, column 10): import must appear at the top level of a file
```

Imports are resolved before the file containing them starts running, so the
alias is in scope everywhere in the file, including above the `import` line.
Write them at the top regardless: it is where a reader looks to see what a file
depends on.

## Errors from inside a module

An error in an imported file names that file, so you know which one to open:

```
error (./bad.miru, line 1, column 11): undefined variable 'rate'
```

## In the playground

The browser [playground](https://stiven-gjekaj.github.io/MiruScriptX/) has no
file system, so `import` there reports that the program was not loaded from a
file. Everything else in this lesson behaves the same in both places.


# When a program stops

When something goes wrong, MiruScriptX stops and prints a single, precise error.
Every error names the line and column where it happened and underlines (`^`) the
exact piece of your source it blames, so you can find the problem quickly. The
underline covers the whole of whatever it is pointing at, so a one-character
operator gets one mark and a name gets one per character.

## Syntax errors

A syntax error means the source could not be parsed. It is caught before any code
runs.

```
print(1 +)
```

Running that reports:

```
error (line 1, column 10): expected an expression but found ')'
    print(1 +)
             ^
```

The caret points at the `)` that appeared where an expression was expected.

## Runtime errors

A runtime error happens while the program runs: dividing by zero, using a name
that was never defined, or indexing past the end of an array. These carry a caret
too, pointing at the expression that failed.

```
let xs = [1, 2, 3]
print(xs[7])
```

```
error (line 2, column 10): index 7 is out of range for an array of length 3
    print(xs[7])
             ^
```

Here the underline sits under the index `7`. A division by zero marks the
operator, which is one character wide:

```
error (line 1, column 3): division by zero
    1 / 0
      ^
```

An undefined variable marks the name, and there the width does some work: you
can see which name is the problem without counting columns.

```
let total = 0
print(subtotal)
```

```
error (line 2, column 7): undefined variable 'subtotal'
    print(subtotal)
          ^^^^^^^^
```

## Where an error came from

An error inside a function also shows the path of calls that reached it, so you
can see not just where the program broke but how it got there.

```
fn double(n) {
  return n * 2
}
fn total(xs) {
  let sum = 0
  for x in xs {
    sum = sum + double(x)
  }
  return sum
}
print(total([1, 2, nil]))
```

```
error (line 2, column 12): cannot multiply a nil and a int
      return n * 2
               ^
  in double, called from line 7
  in total, called from line 11
```

Read it from the top down. The caret says the multiplication failed because `n`
was `nil`. The trace then says `double` was called from line 7, inside `total`,
and `total` was called from line 11. The `nil` came from the array on that last
line, which is the thing to fix.

Each line names a function and the line its call was written on, innermost
first. A program that fails outside any function has no trace, because there is
no call path to report.

Very deep traces are shortened in the middle rather than printed in full, so
runaway recursion reports its error rather than burying it under ten thousand
identical lines.

## Exit codes

Run a file with `miru run` and a successful program exits with status 0. Any
error (a missing file, a syntax error, or a runtime error) exits with a non-zero
status, so scripts and continuous integration can tell success from failure.


# Handling errors

The previous lesson showed what an error looks like. Every one of them stops the
program. That is the right default, and it is not always what you want: a
program that reads input, opens files, or converts text has to survive the day
its input is not what it expected.

`try` is how a program says "this might fail, and I will deal with it".

## try

Put `try` in front of an expression. If it fails, the error becomes the value
of that expression instead of ending the program:

```
let r = try 10 / 0
print(type(r))   // error
```

If nothing fails, `try` does nothing at all:

```
let n = try 6 * 7
print(n)   // 42
```

So `try` marks the places that can fail, and everywhere without it behaves
exactly as it did before.

## Checking

`is_error` answers whether you are holding an error:

```
fn average(row) {
  let total = reduce(row, fn(acc, n) { return acc + n }, 0)
  return total / len(row)
}

let result = try average([])
if is_error(result) {
  print("could not average that:", result.message)
} else {
  print("average:", result)
}
```

`type(result) == "error"` says the same thing. `is_error` is the one to reach
for, because a misspelled string comparison quietly answers `false` forever.

## What an error knows

An error carries five things, read with a dot:

| Field | What it holds |
| ----- | ------------- |
| `message` | What went wrong, as a string |
| `line` | The line it happened on |
| `column` | The column it happened at |
| `file` | The module it came from, or `nil` for the file being run |
| `trace` | The calls it came through, as an array of strings |

`trace` is the useful one. An error remembers the path it came through, so you
can tell where it came from and not just that it happened:

```
fn half(n) { return n / 0 }
fn apply(x) { return half(x) }

let r = try apply(4)
print(r.trace)   // ["in half, called from line 2", "in apply, called from line 3"]
```

A name that is not one of the five is an error rather than `nil`, the same
bargain field access makes everywhere else, so a misspelling fails where it is
written.

## An error is not an ordinary value

You may check an error, ask its type, and read its fields. Anything else stops
the program:

```
let count = try 10 / 0
print(count + 1)
```

```
error (line 2, column 13): unhandled error: division by zero
    print(count + 1)
                ^
```

The message names the original error, and the position is where you *used* it,
because that is the mistake: the program had the error in hand and did
something else with it. The error's own position is still on the value, in
`.line` and `.column`.

This is deliberate. The usual complaint about errors as values is that they get
ignored, flow onward as data, and surface somewhere unrelated. Here they cannot:
either you check one or the program stops at the line that misused it.

For the same reason an error is *not* falsy. `if r { .. }` does not mean "if it
worked", because a successful `0`, `false`, `nil`, or `""` would be
indistinguishable from an error. Ask with `is_error`.

## What try cannot catch

One error refuses to become a value: exceeding the call depth limit.

```
fn boom(n) { return boom(n + 1) }
let r = try boom(0)
```

```
error (line 1, column 21): call depth limit of 10000 exceeded
```

Runaway recursion is a bug in the program rather than a condition to recover
from, and a `try` that swallowed it would hide the only thing worth knowing.

## How much try covers

`try` takes the whole expression after it, not just the next thing:

```
let a = try 10 / 0 + 5    // the division is covered
let b = (try 10) / 0      // it is not; this stops the program
```

Parentheses narrow it when that is what you mean.

## A worked example

[examples/recover.miru](../examples/recover.miru) averages four rows, one of
which is empty. Run it with `miru run examples/recover.miru`:

```
average: 6
skipping a row: division by zero
average: 15
average: 7
rows handled: 3 of 4
```

Without `try`, that program prints one line and stops.


# Next steps

You now know all of MiruScriptX v0.8. Here is where to go from here.

## Try it without installing anything

The [playground](https://stiven-gjekaj.github.io/MiruScriptX/) runs MiruScriptX
in your browser. It is the same lexer, compiler, and virtual machine as the
`miru` command, built to WebAssembly, so anything that works there works on your
machine and the other way round. It has the example programs ready to load, a
Format button, and a tab showing the bytecode your program compiles to. The one
thing it cannot do is `import`, because there is no file system in a browser to
resolve a path against.

## Practice

Try writing a few small programs:

- Print the even numbers from 1 to 20.
- Write a function that reverses an array into a new array.
- Compute the greatest common divisor of two numbers with a `while` loop.

The programs in the [examples](../examples) folder are a good starting point.
Run them with `miru run examples/greet.miru`, try
`miru run examples/greeter.miru` for one that reads your input, and
`miru run examples/shop.miru` for a pair of files that work together.

## Look things up

For a single searchable page covering everything in this wiki, see the
[language reference](../docs/language-reference.md).

## See how it works

Curious how the language is built? The
[architecture guide](../docs/architecture.md) walks through the lexer, parser,
compiler, and virtual machine. You can also see the bytecode for any program
yourself:

```
miru disasm hello.miru
```

## What is coming

The [roadmap](../docs/milestones.md) lists what has shipped, milestone by
milestone, and what is planned next.


