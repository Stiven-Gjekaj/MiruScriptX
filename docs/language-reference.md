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

MiruScriptX is a complete, general-purpose scripting language. If you have written
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

For a short program, use `-e` (or `--eval`) without creating a file:

```
miru -e 'print(6 * 7)'
```

Inline programs cannot use `import`, because they do not have a directory from
which an imported module can be resolved.

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

A long number is easier to read in groups, so `_` is allowed between digits:

```
let budget = 1_000_000
let rate = 1.000_5
```

The underscore is only a mark for you. `1_000` and `1000` are the same number,
and `miru fmt` writes the shorter one, in the same way it writes `1.5` for
`1.50`. It has to sit between two digits, so `1_` and `1__0` are errors. A
name can still start with one: `_1` is a variable, not a number.

## Booleans

Just `true` and `false`.

## Strings

Text in double quotes. Strings support escape sequences such as `\n` (newline),
`\t` (tab), `\"` (a quote), and `\\` (a backslash):

```
print("line one\nline two")
```

For a character you cannot type, write `\u{...}` with its value in hexadecimal:

```
print("\u{41}")       // A
print("\u{1F600}")    // an emoji
print(len("\u{1F600}"))   // 1, because len counts characters
```

One to six digits, in either case. The value has to be a real character: the
largest is `10FFFF`, and `D800` to `DFFF` are reserved and are not characters,
so an escape naming one of those is an error rather than a stray value in your
string.

Note that `miru fmt` writes the character rather than the escape you typed, in
the same way it writes `1.5` for `1.50`. What a string holds is characters, and
`\u{...}` is one way to write them down.

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

## Removing

Use `remove` to take a key out. It gives back the value that was there:

```
let scores = {"ken": 12, "mia": 9}
print(remove(scores, "ken"))   // 12
print(scores)                  // {"mia": 9}
```

Removing a key that is not there is not an error. You get `nil`:

```
print(remove(scores, "nobody"))   // nil
```

**Assigning `nil` does not remove a key.** It stores `nil` under it, and the key
stays:

```
let m = {"a": 1}
m["a"] = nil
print(len(m))        // 1, not 0
print(has(m, "a"))   // true
print(keys(m))       // ["a"]
```

This is worth knowing because assigning `nil` is the natural guess and it fails
quietly. Use `remove`.

One consequence: since removing an absent key also gives `nil`, a key holding
`nil` and a key that was never there give the same answer. Ask `has` before the
removal if you need to tell them apart.

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

## eprint(...)

The same as `print`, to the error stream instead of the output stream.

```
print("the result")             // the result
eprint("something to note")     // something to note
```

Both appear on your screen, so the two look identical when you run a program
yourself. They are different when somebody redirects one:

```
miru run report.miru > results.txt
```

`print` goes into the file. `eprint` still reaches the terminal. That is the
point of having two: a program can say what it produced and separately say what
went oddly, without the second landing in the middle of the first.

## exit(code)

Stops the program and gives the code to whoever ran it. `0` means everything
worked and any other number means it did not. The code must be from 0 to 255.

```
fn check(n) {
  if n < 0 {
    eprint("n must not be negative")
    exit(2)
  }
  return n
}

print(check(5))    // 5
print(check(-1))   // stops here with code 2
```

A program that never calls `exit` gives `0` when it finishes and `1` if an error
stopped it, which is what it always did.

`try` cannot catch an `exit`. The program has stopped. See
[Handling errors](#handling-errors).

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

## remove(map, key)

Takes the key out of the map and gives back the value it held. A key that is
not there is not an error: you get `nil`.

```
let stock = {"apple": 3, "pear": 1}
print(remove(stock, "pear"))   // 1
print(stock)                   // {"apple": 3}
print(remove(stock, "plum"))   // nil
```

Because an absent key gives `nil` too, a key holding `nil` and a key that was
never there look the same afterwards. Ask with `has` first if you need to tell
them apart.

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
- `starts_with(s, prefix)` and `ends_with(s, suffix)` report whether a string
  begins or ends with another. An empty needle gives `true`, and one longer
  than the string gives `false`.

```
print(upper("hi"), lower("HI"))     // HI hi
print(trim("  hi  "))               // hi
print(replace("a.b.c", ".", "-"))   // a-b-c
print(split("a,b,c", ","))          // ["a", "b", "c"]
print(join(["a", "b", "c"], "-"))   // a-b-c
print(contains("hello", "ell"))     // true
print(find("hello", "l"))           // 2
print(starts_with("hello.miru", "hello"))   // true
print(ends_with("hello.miru", ".miru"))     // true
```

## Array functions

- `pop(array)` removes and returns the last element.
- `index_of(array, value)` returns the index of the first match, or -1.
- `slice(seq, start, end)` returns the half-open slice of an array or string.
- `sort(array)` returns a sorted copy (all numbers or all strings).
  `sort(array, key)` sorts by something else; see below.
- `reverse(seq)` returns a reversed copy of an array or string.

```
let xs = [3, 1, 2]
print(sort(xs))                // [1, 2, 3]
print(reverse(xs))             // [2, 1, 3]
print(slice(xs, 0, 2))         // [3, 1]
print(index_of([10, 20], 20))  // 1
```

### Sorting by something other than the value

`sort(array)` puts numbers or strings in order. To sort anything else, give it a
second argument: a function that says what to sort each element **by**.

```
let people = [
  {"name": "Mai", "age": 31},
  {"name": "Aiko", "age": 24},
  {"name": "Ken", "age": 45},
]

for p in sort(people, fn(x) { return x.age }) {
  print(p.age, p.name)
}
```

The function is asked for a key, not for a comparison. It receives one element
and returns the value to order that element by. Those keys follow the same rule
the elements do: all numbers, or all strings.

Any function works, including a builtin:

```
print(sort(["bbb", "a", "cc"], len))   // ["a", "cc", "bbb"]
```

**For decreasing order, reverse it:**

```
print(reverse(sort(scores, fn(x) { return x })))
```

**The sort is stable**, which means two elements with the same key keep the
order they were already in. That is what makes sorting by two things work: sort
by the less important one first, then by the more important one.

```
let by_name = sort(people, fn(p) { return p.name })
let result = sort(by_name, fn(p) { return p.age })
// same age, and Aiko comes before Ken
```

## Math functions

- `abs(x)` is the absolute value.
- `min(...)` and `max(...)` take any number of numeric arguments.
- `floor(x)`, `ceil(x)`, and `round(x)` return integers.
- `sqrt(x)` is the square root (a float); `pow(base, exp)` raises to a power.
- `sum(array)` adds the numbers in an array and `product(array)` multiplies
  them. An empty array gives `0` and `1`, so that adding up the pieces of a
  split array still gives the total.

```
print(abs(-3), min(3, 1, 2), max(3, 1, 2))   // 3 1 3
print(floor(2.7), ceil(2.1), round(2.5))     // 2 3 3
print(sqrt(9), pow(2, 10))                    // 3.0 1024
print(sum([1, 2, 3]), product([2, 3, 4]))     // 6 24
print(sum([]), product([]))                   // 0 1
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

## Files and the command line

These four are what turn a program into a script: something you run from a
terminal, that reads a file, writes one, and takes arguments.

- `read_file(path)` gives the whole file as a string.
- `write_file(path, text)` writes the text, replacing whatever was there, and
  gives `nil`.
- `file_exists(path)` gives `true` if there is a file at the path.
- `args()` gives the arguments the program was given, as an array of strings.
  The program's own path is not one of them.

```
// upper.miru — read a file named on the command line and shout it
let names = args()
if len(names) == 0 {
  print("give me a file to read")
} else {
  let path = names[0]
  if file_exists(path) {
    print(upper(read_file(path)))
  } else {
    print("no file at", path)
  }
}
```

```
$ miru run upper.miru notes.txt
```

**A path is relative to where you are, not to where the script is.** If you run
`miru run scripts/tool.miru` and the program reads `data.txt`, it looks for
`data.txt` in the directory you ran the command from.

This is the opposite of `import`, which finds a module next to the file that
imports it. The two are different on purpose: a module is part of the program
and travels with it, while a data file belongs to whoever is running the
program.

Reading and writing fail with an error where there is no file system, such as in
the browser playground. `try` catches it:

```
let text = try read_file("data.txt")
if is_error(text) {
  print("could not read it:", text.message)
}
```

`file_exists` gives `false` there rather than failing, because the honest answer
to the question is then no.

## read_key()

`input()` waits for you to finish a line and press Enter. `read_key()` gives you
the key the moment it is pressed.

```
print("Press a key, or q to stop.")
while true {
  let key = read_key()
  if key == nil { break }
  if key == "q" { break }
  if key == "ctrl+c" { break }
  print("you pressed", key)
}
```

A key that makes a character gives you that character: `"a"`, `"A"`, `" "`.
Everything else gives a name: `"up"`, `"down"`, `"left"`, `"right"`, `"enter"`,
`"tab"`, `"escape"`, `"backspace"`, `"delete"`, `"home"`, `"end"`, `"pageup"`,
`"pagedown"`, `"insert"`, `"f1"` through `"f12"`, and `"ctrl+a"` through
`"ctrl+z"`. A key with no name gives `"unknown"`, so a loop can ignore it.

Tab gives `"tab"` and not `"ctrl+i"`, even though your terminal sends the same
thing for both. Enter and Backspace work the same way.

### Two things to know before you use it

**Control-C will not stop your program.** While you are reading keys, the
terminal hands Control-C to you as `"ctrl+c"` instead of stopping anything. That
is why the loop above checks for it. **A program that does not check for it
cannot be stopped from the keyboard**, and you will have to close the window.
Check for it in every loop you write.

**The terminal goes back to normal by itself** when your program ends, whether
it finished, failed, or called `exit`. You do not have to put it back.

Somewhere without a keyboard, such as the browser playground, `read_key()` fails
and `try` catches it:

```
let k = try read_key()
if is_error(k) {
  print("no keyboard here:", k.message)
}
```

## key_ready()

`read_key()` waits. That is usually what you want, and for anything that moves
it is exactly what you do not: **your program only gets to do something when
somebody presses a key.** A ball cannot fall while you sit still, because your
program is not running — it is waiting inside `read_key()`.

`key_ready()` tells you whether `read_key()` would answer straight away, so you
can look without committing to a wait:

```
while true {
  while key_ready() {
    let k = read_key()
    if k == nil { return }
    if k == "q" || k == "ctrl+c" { return }
    turn(k)
  }
  fall()          // happens whether or not anybody pressed anything
  draw()
  sleep(50)
}
```

The inner loop takes **everything** pressed since the last picture, rather than
one key. Somebody who presses three keys quickly gets all three handled now,
instead of one now and the others over the next two pictures.

**It answers `true` when the keys have run out**, which sounds wrong and is the
useful part. It is telling you the read will not make you wait — and a read at
the end does not, it gives `nil` at once. That is what lets the loop above
notice the end and stop. If it said `false` there instead, the loop would go
round forever waiting for a key that is never coming.

So `key_ready()` means *"will reading make me wait?"*, not *"is a key down?"*.

Where there is no keyboard at all it fails rather than saying `false`, because
"nothing is pressed" would be a lie about a keyboard that does not exist.

## now()

`now()` gives the number of milliseconds since the start of 1970, as an integer.
That date is where computers count time from, and the number itself is rarely
what you want. The difference between two of them is:

```
let started = now()
let total = 0
for n in range(1, 1000000) {
  total = total + n
}
print("took", now() - started, "milliseconds")
```

This is the first builtin whose answer is not decided by what you wrote.
`upper("hi")` is `"HI"` today and next year. `now()` is different every time you
call it, which is the whole point, and also the reason a program that has to
print the same thing twice should not use it.

The clock comes from whoever is running your program. `miru` has one, and so
does the browser playground. Somewhere without one, `now()` fails and `try`
catches it:

```
let t = try now()
if is_error(t) {
  print("no clock here:", t.message)
}
```

**Do not use it to measure short things.** The clock can be corrected while your
program runs, which makes it jump backwards, and a difference you took across
that moment comes out negative. For anything that has to be right about a
duration, say what your program does when the answer is below zero.

## sleep(ms)

`sleep(ms)` does nothing for that many milliseconds, and then your program
carries on.

```
for n in range(3, 0) {
  print(n)
  sleep(1000)
}
print("go")
```

**This is what makes a loop run at a speed you chose** rather than as fast as
the machine happens to be. A loop with nothing to slow it down runs millions of
times a second, which pins a processor at full speed and makes anything you draw
flash past. Anything that moves on a screen wants to wait between one picture
and the next:

```
while true {
  draw()
  sleep(50)      // twenty pictures a second
}
```

A negative number is an error rather than a wait of no time. Nobody means to
wait for less than nothing, so it is a mistake somewhere earlier — usually a
subtraction that came out the wrong way round — and it is better to hear about
it than to have your loop quietly run flat out. `sleep(0)` is fine, because that
same subtraction reaches zero honestly.

**The browser playground cannot do this**, and it is the one thing there that
has a clock and still refuses. A page draws between one piece of work and the
next, so a page that waited would stop drawing: your program would freeze the
tab and then show its last picture, rather than animating. `try` catches it:

```
let waited = try sleep(50)
if is_error(waited) {
  print("cannot pause here:", waited.message)
}
```

That is why anything that moves is a program for a terminal, and why you will
not find one in the playground.

## random(), random_int(low, high), and seed(n)

`random()` gives a number from 0 up to but not including 1. `random_int(low,
high)` gives a whole number, and both ends count:

```
print(random_int(1, 6))       // a die
print(random_int(0, 1))       // a coin, as 0 or 1
print(random() < 0.3)         // true about three times in ten
```

To pick from an array, ask for an index:

```
let colours = ["red", "green", "blue"]
print(colours[random_int(0, len(colours) - 1)])
```

`len(colours) - 1` is there because indexes start at 0 and both ends of
`random_int` count. Getting this wrong by one is the usual mistake, and the
symptom is an error about an index out of range on about one run in three.

### Making a run repeat

Every run of your program gives different numbers, because the generator starts
from the clock. `seed(n)` starts it from `n` instead, and the same seed always
gives the same numbers:

```
seed(1)
print(random_int(1, 100), random_int(1, 100))   // the same two numbers
seed(1)                                         // every time you run this
print(random_int(1, 100), random_int(1, 100))
```

This is how a program that uses chance is tested. Every example in this
repository that draws a number calls `seed` first, which is what lets its test
assert the exact output.

**Do not save a seed and expect it to work forever.** A later version of
MiruScriptX can change the generator, and then the same seed gives different
numbers. What is promised is the range, and that one seed repeats within one
version.

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

Two things refuse to become a value.

**The call depth limit.**

```
fn boom(n) { return boom(n + 1) }
let r = try boom(0)
```

```
error (line 1, column 21): call depth limit of 10000 exceeded
```

Runaway recursion is a bug in the program rather than a condition to recover
from, and a `try` that swallowed it would hide the only thing worth knowing.

**A call to `exit`.**

```
let r = try exit(3)
print("never reached", r)
```

The program stops with code 3 and the second line never runs. A program that
calls `exit` has finished, and a `try` that caught one would let it carry on
with a code its caller is going to be told about but that no longer describes
what happened.

A refused code is different. `exit(999)` never stops anything, because the code
is out of range, so it stays an ordinary error that `try` catches like any
other:

```
print(is_error(try exit(999)))   // true
```

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

You now know all of MiruScriptX 1.7. Here is where to go from here.

## Try it without installing anything

The [playground](https://stiven-gjekaj.github.io/MiruScriptX/) runs MiruScriptX
in your browser. It is the same lexer, compiler, and virtual machine as the
`miru` command, built to WebAssembly, so anything that works there works on your
machine and the other way round. It has the example programs ready to load, a
Format button, and a tab showing the bytecode your program compiles to.

**Share** copies a link that carries the program you wrote. It goes in the part
of the address after the `#`, which a browser never sends to a server, so
sharing a program sends it to the person you give the link to and to nobody
else.

What it cannot do is anything that needs a file, because a browser has no file
system to give it: `import`, `read_file`, and `write_file` all report there
rather than pretending. `file_exists` answers `false` and `args` gives an empty
array, since those are the honest answers when there is no file system and no
command line. `input` reads nothing and `read_key` refuses, because a page has
no keyboard to read from the way a terminal does.

`eprint` and `exit` do work there. A page has no process to end, but it can say
what a program asked for, so `eprint` output appears marked as its own stream
and a non-zero code is reported under the output. That is an honest answer,
unlike reading a file, where the honest answer is that it cannot.

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


