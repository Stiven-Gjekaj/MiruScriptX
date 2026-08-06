# The MiruScriptX Language Specification

Version 1.8

This document defines the MiruScriptX language. It tells you what a program
means. The [wiki](../wiki/01-introduction.md) teaches the language, and the
[language reference](language-reference.md) summarizes it. This document is
different: it is the statement of record.

MiruScriptX is a revival of MiruScript, an earlier language by the same author,
written in C. The X shows that this is the successor.

For the promise about which parts of this document will not change, read the
[stability guarantee](stability.md).

---

## 1. Scope and notation

### 1.1 What this document defines

This document defines:

- The characters and tokens that a program can contain.
- The grammar that makes tokens into a program.
- The meaning of each construct.
- The types of value, and the operations on them.
- The errors, and the conditions that cause them.
- The limits, and the message at each limit.

### 1.2 What this document does not define

This document does not define the bytecode, the virtual machine, or the Rust
API. These are parts of the implementation. They can change in a 1.x release.
The [architecture guide](architecture.md) describes them.

### 1.3 Language of this document

This document uses ASD-STE100 Simplified Technical English. Each word has one
meaning. Each sentence is short. This makes the document less pleasant to read
than the wiki. It also makes the document harder to misread, which matters more
here.

### 1.4 One word for one thing

An **error** is a condition that stops a program. `try` can catch almost all
errors. A caught error is a value, and this document calls it an error too.
There is no second word for it.

### 1.5 Grammar notation

The grammar uses this notation:

| Symbol | Meaning |
| ------ | ------- |
| `x y` | `x`, and then `y` |
| `x \| y` | `x`, or `y` |
| `[ x ]` | `x` zero times or one time |
| `{ x }` | `x` zero or more times |
| `( x )` | Groups `x` |
| `"x"` | The characters `x` |

### 1.6 How to read a rule

Each rule in this document is a statement about all programs. If a rule and the
implementation disagree, one of the two has a defect. Tell the maintainer.

---

## 2. Lexical structure

The lexer reads the source text. The lexer makes a list of tokens. The parser
reads the tokens.

### 2.1 Source text

The source text is a sequence of Unicode characters in UTF-8. Positions in an
error message are in characters, not in bytes.

### 2.2 Comments

A comment starts with `//`. A comment continues to the end of the line. There
are no block comments.

```
// This is a comment.
let x = 1   // This is also a comment.
```

### 2.3 Statement separators

A newline ends a statement. A semicolon also ends a statement. A semicolon is
always optional.

The lexer does not make a newline token inside parentheses `( )` or brackets
`[ ]`. An expression can continue on more than one line inside these.

```
let total = (1 +
             2)
```

The lexer removes a newline at the start of the source. The lexer makes one
newline token for a group of empty lines.

### 2.4 Identifiers

An identifier starts with a letter or `_`. An identifier continues with a
letter, a digit, or `_`. Letters and digits include Unicode letters and digits.

### 2.5 Keywords

These 16 words are keywords. A program cannot use a keyword as an identifier.

```
fn      let     return  if      else    while   for     in
break   continue import  as      try     true    false   nil
```

A keyword is also not permitted as a field name after `.`. Use the bracket form
for a map key that is a keyword.

```
let m = {"if": 1}
print(m["if"])   // 1
print(m.if)      // Error: expected a field name after '.'
```

### 2.6 Integer literals

An integer literal is a sequence of decimal digits. A `_` between two digits is
permitted, and groups the digits for a reader. `1_000` and `1000` are the same
number.

A `_` must have a digit before it and a digit after it. A `_` in another
position gives the error `a digit separator must be between two digits`. This
includes `1_`, `1__0`, and `1_.5`.

A name can start with `_`. `_1` is an identifier, not a number.

There is no exponent form. There is no hexadecimal, octal, or binary form. A
literal cannot start with a minus sign: `-5` is the negation operator and the
literal `5`.

An integer literal must be in the range of a 64-bit signed integer. A literal
that is too large gives the error `integer literal '<text>' is out of range`.

> **Note.** The smallest integer, -9223372036854775808, has no literal form.
> Its magnitude is larger than the largest literal. Write
> `-9223372036854775807 - 1` instead.

### 2.7 Float literals

A float literal is a sequence of decimal digits, then `.`, then one or more
decimal digits.

A `_` between two digits is permitted in each part. Section 2.6 gives the rule.
`1_000.5` and `1.000_5` are permitted.

A digit after the point is necessary. The text `1.` is the integer literal `1`
and then the `.` operator. For the same reason, `1._5` is the integer literal
`1`, then the `.` operator, then the identifier `_5`.

### 2.8 String literals

A string literal starts with `"` and ends with `"`.

A string literal cannot contain a newline character. A string literal that
reaches the end of a line, or the end of the source, gives the error
`unterminated string literal`.

These seven escape sequences are permitted:

| Escape | Character |
| ------ | --------- |
| `\n` | Newline |
| `\t` | Tab |
| `\r` | Carriage return |
| `\\` | Backslash |
| `\"` | Quotation mark |
| `\0` | Null |
| `\u{...}` | The character with the given value |

In `\u{...}`, the `...` is one to six hexadecimal digits. The digits can be
upper case or lower case. The digits give the value of one character.

The largest character is `10FFFF`. The values from `D800` to `DFFF` are not
characters. A value that is not a character is an error.

`"\u{41}"` is the same as `"A"`. `"\u{1F600}"` is one character, and `len` of
it is 1, because `len` counts characters.

These are the errors:

| Source | Error |
| ------ | ----- |
| `"\u41"` | `escape sequence '\u' needs a '{'` |
| `"\u{}"` | `escape sequence '\u{}' needs at least one hexadecimal digit` |
| `"\u{4G}"` | `escape sequence '\u{...}' takes hexadecimal digits, found 'G'` |
| `"\u{0000041}"` | `escape sequence '\u{...}' takes at most 6 hexadecimal digits` |
| `"\u{41"` | `escape sequence '\u{...}' needs a '}'` |
| `"\u{D800}"` | `'\u{D800}' is not a character` |

An escape sequence that reaches the end of a line, or the end of the source,
gives the error `unterminated string literal`.

Another escape sequence gives the error `unknown escape sequence '\<c>'`. There
is no `\x` escape.

### 2.9 Operators and punctuation

```
+   -   *   /   %
==  !=  <   >   <=  >=
&&  ||  !
=   .   ,   :   ;
(   )   [   ]   {   }
```

---

## 3. Grammar

### 3.1 Program

```
program   = { statement }
statement = import | let | assign | function | return
          | if | while | for | break | continue | expression
```

A statement ends at a newline or a semicolon. Section 2.3 gives the rule.

### 3.2 Statements

```
import    = "import" string "as" identifier

let       = "let" identifier "=" expression

assign    = target "=" expression
target    = identifier | index | field

function  = "fn" identifier "(" [ params ] ")" block
params    = identifier { "," identifier }

return    = "return" [ expression ]

if        = "if" expression block [ "else" ( if | block ) ]

while     = "while" expression block

for       = "for" identifier "in" expression block

break     = "break"
continue  = "continue"

block     = "{" { statement } "}"
```

`else` can be on the same line as the previous `}`, or on the next line.

### 3.3 Expressions

```
expression = try | binary
try        = "try" expression
binary     = unary { operator unary }
unary      = ( "-" | "!" ) unary | postfix
postfix    = primary { call | index | field }
call       = "(" [ arguments ] ")"
arguments  = expression { "," expression }
index      = "[" expression "]"
field      = "." identifier
primary    = integer | float | string | "true" | "false" | "nil"
           | identifier | array | map | closure | "(" expression ")"
array      = "[" [ expression { "," expression } ] "]"
map        = "{" [ entry { "," entry } ] "}"
entry      = expression ":" expression
closure    = "fn" "(" [ params ] ")" block
```

An array literal and a map literal permit a trailing comma.

### 3.4 Precedence

The table gives the precedence of each operator. A larger number binds more
strongly. All binary operators are left-associative.

| Precedence | Operators | Associativity |
| ---------- | --------- | ------------- |
| 7 (strongest) | `f(x)`  `a[i]`  `a.b` | Left |
| 6 | `-x`  `!x` | Right |
| 5 | `*`  `/`  `%` | Left |
| 4 | `+`  `-` | Left |
| 3 | `<`  `>`  `<=`  `>=` | Left |
| 2 | `==`  `!=` | Left |
| 1 | `&&` | Left |
| 0 | `\|\|` | Left |
| Weakest | `try` | None |

`try` binds less strongly than every operator. `try a / b` applies `try` to the
division. Use parentheses to make `try` apply to less: `(try a) / b`.

---

## 4. Values and types

### 4.1 The eight types

The `type` builtin gives the name of the type of a value. There are eight
names.

| Name | Values |
| ---- | ------ |
| `int` | A 64-bit signed integer |
| `float` | A 64-bit IEEE 754 binary floating-point number |
| `bool` | `true` or `false` |
| `string` | A sequence of Unicode characters |
| `array` | An ordered sequence of values |
| `map` | A set of pairs of a string key and a value |
| `function` | A function, a closure, or a builtin |
| `nil` | The single value `nil` |

An error is a ninth type. The `type` builtin gives the name `error` for it.
Section 6 describes it. A program cannot make an error value without `try`.

### 4.2 Value types and reference types

An `int`, a `float`, a `bool`, a `string`, and `nil` are value types. An
operation on one of these gives a new value.

An `array` and a `map` are reference types. Two names can hold the same array.
A change through one name is visible through the other name.

```
let a = [1, 2]
let b = a
push(b, 3)
print(a)     // [1, 2, 3]
```

The `slice`, `sort`, `reverse`, `map`, and `filter` builtins give a new array.
The `push` builtin changes the array and gives the same array.

### 4.3 Map key order

A map keeps its keys in sorted order. The `keys` builtin, the `values` builtin,
a `for` loop, and printing all use this order. The order does not depend on the
order of insertion.

```
print(keys({"c": 1, "a": 2, "b": 3}))   // ["a", "b", "c"]
```

### 4.4 How a value prints

The `print` builtin writes a string without quotation marks. Inside an array or
a map, a string has quotation marks and escapes.

These are the escapes it writes: `\"`, `\\`, `\n`, `\t`, `\r`, and `\0`. Each
other character that the Unicode standard puts in the category `Cc` is written
as `\u{...}`. This category has the values `00` to `1F`, `7F`, and `80` to
`9F`. Each other character is written as itself.

`miru fmt` uses the same rule for a string literal. Section 2.8 gives the
escapes the lexer reads, and each escape here is one of them.

| Value | `print` writes | Inside an array |
| ----- | -------------- | --------------- |
| `1` | `1` | `1` |
| `1.5` | `1.5` | `1.5` |
| `1.0` | `1.0` | `1.0` |
| `"a"` | `a` | `"a"` |
| `true` | `true` | `true` |
| `nil` | `nil` | `nil` |
| A function `f` | `<fn f>` | `<fn f>` |
| A builtin `len` | `<builtin len>` | `<builtin len>` |
| An error | `<error: MESSAGE>` | `<error: MESSAGE>` |

A float always has a decimal point, an exponent, `nan`, `inf`, or `-inf`.

---

## 5. Semantics

### 5.1 Order of evaluation

| Construct | Order |
| --------- | ----- |
| `a + b` | `a`, then `b` |
| `f(x, y)` | `f`, then `x`, then `y` |
| `[x, y]` | `x`, then `y` |
| `{k: v}` | `k`, then `v`, for each pair in order |
| `a[i]` | `a`, then `i` |
| `a[i] = v` | **`v`, then `a`, then `i`** |
| `a.b = v` | **`v`, then `a`** |
| `let x = e` | `e`, then the name `x` starts to exist |

An assignment through an index evaluates the value first. This is the same
order as Python. Write the parts as separate statements if the order matters.

Because `let` evaluates the value first, `let x = x` reads the outer `x`.

### 5.2 Numbers

An operation on two integers gives an integer. An operation on two floats gives
a float. An operation on an integer and a float changes the integer to a float
first, and gives a float.

> **Note.** The change to a float loses precision for an integer with a
> magnitude of more than 2^53. The language does not give a warning.

Every integer operation tests for overflow. An operation that overflows gives
an error. An integer operation never wraps around.

```
print(9223372036854775807 + 1)   // Error: integer overflow in addition
```

Division by zero gives the error `division by zero`. This applies to an integer
and to a float. `1.0 / 0.0` does not give `inf`.

The `%` operator gives the remainder. The sign of the result is the sign of the
left operand. `-7 % 3` is `-1`. Modulo by zero gives the error `modulo by
zero`.

### 5.2.1 Infinity and not-a-number

Arithmetic never gives `inf` or `nan`. Division by zero is an error, and every
other operation that could give one of these is an error.

Three operations can give one:

- `float("inf")` and `float("-inf")` give an infinity.
- `float("nan")` gives a not-a-number.
- `pow` gives an infinity when the result is too large.

A program can hold these values. The rules for them are:

- Arithmetic on an infinity works. `float("inf") + 1.0` is `inf`.
- An order comparison with an infinity works.
- `nan` is not equal to `nan`. `==` gives `false`, and does not give an error.
- An order comparison with `nan` gives the error `cannot compare with NaN`.
- `sort` on an array with `nan` gives the error `sort cannot order NaN`.
- `min` and `max` with `nan` give the error `<name> cannot compare NaN`.
- `int`, `floor`, `ceil`, and `round` on `inf` or `nan` give the error
  `<name> of a non-finite number`.

### 5.3 Truth

Only `false` and `nil` are false. Every other value is true. `0`, `0.0`, `""`,
`[]`, and `{}` are all true.

An error is not true and not false. A test on an error stops the program.
Section 6.4 gives the rule.

The `&&` and `||` operators do not evaluate the right operand if the left
operand decides the result. Both operators give a `bool`, not the operand.

```
print(1 && 2)   // true, not 2
```

### 5.4 Equality

`==` compares two values. `!=` gives the opposite result.

- An integer and a float compare as numbers. `1 == 1.0` is true.
- A `bool` is equal only to a `bool`. `true == 1` is false.
- A string compares by its characters.
- An array compares by its length and its elements.
- A map compares by its keys and its values.
- A function is equal only to itself.
- Two values of different types are not equal.
- `nan` is not equal to `nan`.

Comparison of a value that contains itself gives an error. Section 5.7 gives
the rule.

### 5.5 Order

`<`, `>`, `<=`, and `>=` compare two integers, two floats, an integer and a
float, or two strings. A string compares by the code point of each character.

A comparison with `nan` gives the error `cannot compare with NaN`. A comparison
of two other types gives an error that names both types.

### 5.6 Strings

A string holds Unicode characters. Every builtin counts characters, not bytes.
`len("héllo")` is 5.

A string does not support the index operator. `"abc"[0]` gives the error
`cannot index a string`. Use the `slice` builtin.

### 5.6.1 Adding two arrays

The `+` operator joins two arrays. The result is a **new** array. The operator
does not change either operand.

```
let front = [1, 2]
let back = [3]
print(front + back)   // [1, 2, 3]
print(front)          // [1, 2]
```

An array added to a value that is not an array is an error. `[1] + 2` gives
`cannot add a array and a int`.

The elements are not copied. An array that contains another array gives a result
that refers to the same inner array. Section 5.4 gives the rule for what an
array is.

### 5.7 A value that contains itself

An array can contain itself. A map can contain itself.

- To print such a value, the language writes `[...]` or `{...}` at the point
  the value comes back to itself.
- To compare such a value with itself gives `true`. The language compares the
  identity first.
- To compare two different such values gives an error. There is no answer to
  give.

A value that nests very deeply, but does not contain itself, also gives these
results at a limit. Section 9 gives the limit.

```
let a = []
push(a, a)
print(a)        // [[...]]
print(a == a)   // true
```

### 5.8 Names

A name at the top level of a file is a global name. A name inside a block is a
local name.

A `let` statement makes a name. An assignment does not make a name. An
assignment to a name that no `let` made gives the error `cannot assign to
undefined variable '<name>'`.

If a name that the program can use is near to the name that was written, the
message adds `Did you mean '<name>'?`. Near means a small number of changes to
letters: an addition, a removal, a replacement, or a swap of two letters that
touch. Upper case and lower case are the same for this comparison. The message
gives one name, or no name.

The number of changes that is permitted can change in a later 1.x release, and
so can the words of the message. Section 3.1 of the
[stability guarantee](stability.md) gives the rule.

A `let` at the top level can use the name of a builtin. The file then has its
own name, and every other file keeps the builtin.

```
let print = 1
print(print)    // Error: a int is not callable
```

An inner `let` can use the name of an outer variable. The inner name hides the
outer name until the end of the block.

### 5.9 Loops

A `for` loop reads an array. A `for` loop on another type gives the error
`cannot iterate over a <type>`.

A `for` loop makes a copy of the array before the first step. A change to the
array inside the loop does not change the number of steps.

The loop variable is a new name at each step. A closure that a step makes keeps
the value of that step.

### 5.10 Functions

A function that reaches its end without a `return` gives `nil`.

A call with the wrong number of arguments gives an error. The error names the
function, the number it needs, and the number it received.

A closure keeps the variables it uses. While the enclosing function runs, the
closure and the function share each variable. After the enclosing function
returns, the closure keeps its own copy.

---

## 6. Errors

### 6.1 What an error stops

An error stops the program. The program writes the error to the standard error
stream and gives the exit code 1.

### 6.2 The shape of a report

An error report has this shape:

```
error (line LINE, column COLUMN): MESSAGE
    THE SOURCE LINE
    ^^^^
  in NAME, called from line LINE
```

- The first line gives the position and the message.
- The next two lines show the source and mark the part at fault. These two
  lines are not present when the error is in a different file.
- Each remaining line gives one call, from the innermost outward.

An error in an imported file names the file:

```
error (./math.miru, line 2, column 12): division by zero
```

The program does not show the source in this case. It holds the text of the
file it started with, not the text of the module.

### 6.2.1 More than one syntax error

**A syntax error is the only kind that can be reported more than one time in a
run.** A program stops at the first error it makes while it runs, so there is
only one of those. A program can hold several syntax mistakes at once, and each
one is reported, in the order they appear in the file. Each has the shape above.

A program with a syntax error does not run, however many errors it has. The
exit code is 1.

Three rules limit the report:

- Two errors at one position are one error.
- A run reports at most 20 errors.
- After three statements fail in a row with none between them that parsed, the
  report stops. **One mistake can make every statement after it fail.** An
  unclosed `(` or `[` is the case that does: section 2.3 says the lexer makes no
  newline token inside those, so nothing after the opening bracket can end a
  statement. Those later errors are consequences of the first one, not separate
  mistakes.

**An error in the lexer is different, and there is only ever one.** The lexer
reads the source before the parser sees it, so a program with an unterminated
string or a bad `\u{...}` escape stops there and no statement is parsed at all.
Section 2.8 gives those errors.

### 6.3 Catching an error

`try EXPRESSION` gives the value of the expression. If the expression gives an
error, `try` gives the error as a value, and the program continues.

`try` applies to the whole expression after it. Section 3.4 gives the
precedence.

### 6.4 What a program can do with an error

A program can do these things with an error value:

- Put it in a variable, an array, or a map.
- Give it to `type`.
- Give it to `is_error`.
- Read one of its five fields.

Every other operation stops the program. The message is `unhandled error:` and
then the message of the original error. The position is the position of the
operation that used the error, not the position of the original error.

An error is not true and not false. `if r { }` on an error stops the program.
This is necessary: a correct result of `0`, `false`, `nil`, or `""` would look
the same as an error if an error were false.

### 6.5 The fields of an error

| Field | Type | Contents |
| ----- | ---- | -------- |
| `message` | `string` | What went wrong |
| `line` | `int` | The line of the error |
| `column` | `int` | The column of the error |
| `file` | `string` or `nil` | The module, or `nil` for the first file |
| `trace` | `array` | The calls, as strings |

A name that is not one of these five gives the error `an error has no field
'<name>'`. If one of the five is near to the name that was written, the message
adds `Did you mean '<name>'?`. Section 5.8 gives the rule.

```
fn half(n) { return n / 0 }
let r = try half(4)
print(r.message)   // division by zero
print(r.trace)     // ["in half, called from line 2"]
```

### 6.6 What `try` cannot catch

`try` cannot catch the call depth limit. Section 9 gives the limit. Recursion
that does not stop is a defect in the program, not a condition to handle.

`try` cannot catch an `exit`. A program that calls `exit` has stopped, and a
`try` that caught one would let the program continue with a result its host
will never be told about.

---

## 7. Modules

### 7.1 The import statement

```
import "./math.miru" as math
```

The path is a string. The name after `as` is necessary.

The program reads the names of the module through the alias:

```
print(math.add(2, 3))
```

### 7.2 How the path resolves

The path resolves against the directory of the file that contains the `import`,
not against the working directory.

A program that is not in a file cannot import. The error is `cannot import:
this program was not loaded from a file`.

A path that is not a file gives the error `cannot import '<path>': no such
file`.

### 7.3 Each module runs one time

The language makes the path absolute before it looks in the cache. Two
different paths to one file are the same module. The module runs one time.

```
import "./sub/../shared.miru" as a
import "./shared.miru" as b
```

Both names give the same module. The module runs one time.

### 7.4 A cycle is an error

A module that imports itself, through any number of steps, gives an error. The
error names each step:

```
import cycle: ./y.miru -> ./x.miru -> ./y.miru
```

### 7.5 What a module gives

A module gives a map. The map holds each name that the module defines at its
top level. A name that the module uses but does not define is not in the map.

### 7.6 Names in a module

Each module has its own names. Two modules can define one name, and the two
names are separate.

The builtins are the same in every module. A module that declares the name of a
builtin changes that name for itself only.

### 7.7 When a module runs

The language resolves and runs every import before it compiles the file that
contains the import.

---

## 8. Builtins

There are 61 builtins. A program can use each of them without an import.

A builtin refuses a caught error, and stops the program. There are two
exceptions: `type` and `is_error` accept one, because a program uses them to
find out that it holds one.

### 8.1 Output and input

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `print(...)` | Any number | Writes the values, separated by a space, then a newline. Gives `nil`. |
| `eprint(...)` | Any number | The same as `print`, to the diagnostic stream. Gives `nil`. |
| `exit(n)` | 1 | Stops the program. The program gives `n` to the host. Does not give a value. |
| `input()` | 0 or 1 | Reads one line. With an argument, writes the argument first. Gives a string, or `nil` at the end of the input. |

A program has two streams of output. `print` writes to the result stream.
`eprint` writes to the diagnostic stream. A host that has only one stream can
send both to the same place. A host must not discard the diagnostic stream.

`exit` stops the program immediately. `try` cannot catch an `exit`. Section 6.6
gives the list of what `try` cannot catch.

`n` must be an integer from 0 to 255. Section 9 gives this limit. A value
outside the range, or a value that is not an integer, gives an error, and that
error is an ordinary error which `try` can catch, because the program did not
stop.

### 8.2 Files

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `read_file(path)` | 1 string | Gives the contents of the file as a string. |
| `write_file(path, text)` | 2 strings | Writes the text to the file, and replaces what was there. Gives `nil`. |
| `file_exists(path)` | 1 string | Gives `true` if there is a file at the path. |
| `args()` | 0 | Gives the arguments the program was given, as an array of strings. The program's own path is not among them. |

**A path is resolved against the working directory**, which is the directory the
program was started from. This is not the rule for `import`, which resolves
against the file that holds it.

The two rules are different because the two things are different. A module is
part of the program and stays with it. A data file belongs to the person who
runs the program, and is where that person is. A program started with `-e` can
read a file for the same reason, although it cannot `import`.

`read_file` and `write_file` give an error if the host has no file system, for
example in a browser. `file_exists` gives `false` in that situation, because the
honest answer to the question is then no. `try` catches the error.

### 8.3 General

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `len(v)` | 1 | The number of characters, elements, or pairs. Refuses another type. |
| `type(v)` | 1 | The name of the type, as a string. |
| `is_error(v)` | 1 | `true` if the value is a caught error. |
| `str(v)` | 1 | The value as a string, in the form `print` writes. |

### 8.4 Arrays

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `push(a, v)` | 2 | Adds `v` to the end of `a`. Changes `a`. Gives `a`. |
| `insert(a, i, v)` | 3 | Puts `v` at position `i`. Changes `a`. Gives `a`. |
| `pop(a)` | 1 | Removes the last element and gives it. Refuses an empty array. |
| `index_of(a, v)` | 2 | The index of the first equal element, or `-1`. |
| `slice(v, s, e)` | 3 | A new array or string, from `s` to `e`. The language limits `s` and `e` to the length. |
| `sort(a)` or `sort(a, key)` | 1 or 2 | A new array, sorted. Section 8.8 gives the two-argument form. |
| `reverse(v)` | 1 | A new array or string, reversed. |
| `range(e)` or `range(s, e)` | 1 or 2 | An array of integers, from `s` (or 0) to `e`. `e` is not in the result. |

### 8.5 Strings

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `upper(s)` | 1 | The string in upper case. |
| `lower(s)` | 1 | The string in lower case. |
| `trim(s)` | 1 | The string without space at each end. |
| `replace(s, a, b)` | 3 | The string with each `a` changed to `b`. |
| `split(s, sep)` | 2 | An array of strings. An empty `sep` gives each character. |
| `join(a, sep)` | 2 | The elements of `a` as one string, with `sep` between. |
| `contains(v, x)` | 2 | `true` if the string holds the substring, or the array holds an equal element. |
| `find(s, x)` | 2 | The character index of the first `x`, or `-1`. |
| `starts_with(s, prefix)` | 2 strings | `true` if `s` begins with `prefix`. |
| `ends_with(s, suffix)` | 2 strings | `true` if `s` ends with `suffix`. |

An empty `prefix` or `suffix` gives `true`. One that is longer than `s` gives
`false`.

### 8.6 Maps

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `keys(m)` | 1 | An array of the keys, in sorted order. |
| `values(m)` | 1 | An array of the values, in the order of the keys. |
| `has(m, k)` | 2 | `true` if the map holds the key. |
| `remove(m, k)` | 2 | Removes the key and gives the value it held, or `nil`. Changes `m`. |

`remove` is the only way to take a key out of a map. An assignment of `nil`
does not do it: `m["a"] = nil` keeps the key, `len` still counts it, and `keys`
still gives it.

A key that is not in the map is not an error. `remove` gives `nil`, the same
answer that reading an absent key gives.

> **Note.** A key that holds `nil` and a key that is not there both make
> `remove` give `nil`. Use `has` before the removal to tell one from the other.

### 8.7 Numbers

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `abs(n)` | 1 | The magnitude. |
| `min(...)` | 1 or more | The smallest. |
| `max(...)` | 1 or more | The largest. |
| `floor(n)` | 1 | The largest integer that is not larger than `n`. Gives an `int`. |
| `ceil(n)` | 1 | The smallest integer that is not smaller than `n`. Gives an `int`. |
| `round(n)` | 1 | The nearest integer. A half goes away from zero. Gives an `int`. |
| `sqrt(n)` | 1 | The square root. Refuses a negative number. |
| `pow(b, e)` | 2 | `b` to the power `e`. Gives an `int` for two integers with `e` not negative. Gives a `float` in each other condition. |
| `int(v)` | 1 | A string or a number as an `int`. A float goes towards zero. |
| `float(v)` | 1 | A string or a number as a `float`. |
| `sum(a)` | 1 array | The numbers in the array, added. An empty array gives `0`. |
| `product(a)` | 1 array | The numbers in the array, multiplied. An empty array gives `1`. |

`sum` and `product` give an `int` for an array of integers, and a `float` for
an array that has one or more floats. Section 5.2 gives the promotion rule.
Overflow of the integer form is an error, as it is for each other integer
operation. An element that is not a number is an error.

### 8.8 Higher-order

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `map(a, f)` | 2 | A new array of `f(x)` for each element. |
| `filter(a, f)` | 2 | A new array of the elements for which `f(x)` is true. |
| `reduce(a, f, init)` | 3 | One value, from `f(accumulator, x)` for each element. |
| `sort(a, key)` | 2 | A new array, ordered by `key(x)` for each element. |

`filter` asks whether the result of `f` is true. A result that is a caught
error stops the program, as `if` does. `map` and `reduce` only keep the result,
so they do not refuse one.

`sort(a, key)` calls `key` one time for each element. It then orders the
elements by the values it received. The rules for those values are the rules
section 8.4 gives for the elements of `sort(a)`: all numbers, or all strings.
A value that is not one of those, including a caught error, is an error.

`key` must be a function. `sort(a, nil)` is an error. This is different from
`map` and `filter`, which give the error at the call.

**The sort is stable.** Two elements with equal keys keep the order they had.
This makes a sort by two keys two sorts, the less important key first:

```
let by_name = sort(people, fn(p) { return p.name })
let result = sort(by_name, fn(p) { return p.age })
```

For decreasing order, use `reverse`:

```
print(reverse(sort(scores, fn(x) { return x })))
```

### 8.9 Time

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `now()` | 0 | The milliseconds since 1970-01-01T00:00:00Z, as an integer. |
| `sleep(ms)` | 1 | Does nothing for `ms` milliseconds. Gives `nil`. |

`now` gives an integer and not a float, so no whole millisecond is lost.

**The result is not monotonic.** A host whose clock is corrected while a program
runs gives a smaller number than it gave a moment before. A program that
measures how long something took by subtracting two of these can therefore get a
negative answer, and one that must not be wrong about it has to say what it does
in that case.

`now` gives an error if the host has no clock, and `try` catches it. This is the
rule `read_file` follows, and for the same reason: a program handed a wrong time
goes on to do the wrong thing with it, and 1970 is a wrong time rather than an
absent one. `miru` has a clock. So does the browser playground, which has no
file system: the two capabilities are separate and a host can have either
without the other.

**`now` is the first builtin whose result the program's own source does not
determine.** Section 8.10 gives the others. Every builtin outside these two
sections gives the same answer each time it is called with the same arguments.

`sleep` takes an integer, and **a negative one is an error** rather than a
return with nothing done. Nobody means to wait for less than no time, so a
negative duration is a mistake further up — usually a subtraction that went the
wrong way while working out how much of a frame was left — and returning at once
would hide it. Zero is not an error, because that same subtraction reaches zero
honestly on a frame whose work took exactly as long as the frame.

`sleep` gives an error where the host cannot pause, and `try` catches it. **This
is the one place where a host that has a clock still refuses.** A page can tell
the time and cannot spend it: a browser paints between turns of its event loop,
so blocking would hold that loop and freeze the tab rather than pacing anything.
`miru` can pause; the browser playground cannot.

### 8.10 Random numbers

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `random()` | 0 | A float from 0 up to but not including 1. |
| `random_int(low, high)` | 2 ints | An integer from `low` to `high`. Both are in the range. |
| `seed(n)` | 1 int | Starts the generator again from `n`. Gives `nil`. |

`random_int` gives an error if `low` is above `high`. `random_int(n, n)` gives
`n`. An argument that is not an integer is an error, including a float.

The generator is started from the host's clock the first time a program asks
for a number, so two runs of the same program give different numbers.

`seed(n)` starts it again from `n` instead. **Two runs that start from the same
seed give the same numbers.** This is how a program that uses chance is tested:
it can be asserted against exact output and still be about chance. Every integer
is a seed.

Where the host has no clock, the generator starts from a fixed value, and the
program gives the same numbers each time it runs. `random` does not refuse in
that situation. `now` refuses because a wrong time is worse than none, and a
random number that repeats is still a number in the correct range.

**Which numbers follow from a given start is not defined by this document.** A
later 1.x can change the generator. Section 3.8 of the
[stability guarantee](stability.md) states this.

### 8.11 The keyboard

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `read_key()` | 0 | The next key, as a string. `nil` when there are no more. |
| `key_ready()` | 0 | `true` if `read_key()` gives an answer without waiting. |

`input()` waits for a whole line. `read_key()` waits for one key and gives it
back immediately.

**`key_ready()` says whether a read waits. It does not say whether a key is
pressed.** The two are different at the end of input, where `read_key()` gives
`nil` immediately: `key_ready()` is `true` there.

This is the rule that makes a loop end. A program that reads only when a key is
ready must still be able to learn that no key is ever coming:

```
while key_ready() {
  let k = read_key()
  if k == nil { break }
  handle(k)
}
```

If `key_ready()` were `false` at the end of input, the loop above would stop,
but a program that asks again on each turn would ask forever.

`key_ready()` gives an error where the host has no keyboard, and `try` catches
it. It does not give `false`. `false` means "no key is waiting now", which is a
statement about a keyboard that exists.

A key that makes a character gives that character: `"a"`, `"A"`, `" "`, `"é"`.
One character, not one byte.

Another key gives its name from this table:

| Name | Key |
| ---- | --- |
| `up` `down` `left` `right` | The arrow keys |
| `enter` | Enter, or Return |
| `tab` | Tab |
| `backspace` | Backspace |
| `escape` | Escape |
| `delete` `insert` `home` `end` `pageup` `pagedown` | The editing keys |
| `f1` to `f12` | The function keys |
| `ctrl+a` to `ctrl+z` | A letter held with Control |
| `ctrl+space` | The space bar held with Control |
| `unknown` | A key this implementation does not have a name for |

`tab` and `enter` and `backspace` have their own names. A terminal sends the
same value for Tab as for Ctrl-I, for Enter as for Ctrl-M, and for Backspace as
for Ctrl-H. The name of the key wins. A program does not have to know this.

An unrecognised key gives `"unknown"` and not an error, so a program in a loop
can ignore it.

`read_key` gives an error if the host has no keyboard, and `try` catches it.
The browser playground has no keyboard, and has a clock. Section 8.9 gives the
same rule for the clock.

**Reading a key changes the terminal.** The terminal stops collecting a line
before it gives the program anything, and stops showing what is typed. The host
puts the terminal back when the program ends, for all three ways a program can
end: the last statement, an error, and `exit`.

> **Warning.** While a program reads keys, **Control-C does not stop it**. The
> terminal gives `"ctrl+c"` to the program instead. A program that reads keys in
> a loop must decide what to do with that key. A program that ignores it cannot
> be stopped from the keyboard.

### 8.12 The terminal

| Builtin | Arguments | Result |
| ------- | --------- | ------ |
| `clear()` | 0 | Clears the screen. Puts the cursor at the top left. Gives `nil`. |
| `move_to(column, row)` | 2 | Puts the cursor there. Gives `nil`. |
| `hide_cursor()` | 0 | Stops the terminal from drawing the cursor. Gives `nil`. |
| `show_cursor()` | 0 | Draws the cursor again. Gives `nil`. |
| `term_size()` | 0 | An array of two integers: the columns, then the rows. |

The terminal is a capability. Section 8.2 gives the rule for the file system,
8.9 for the clock, and 8.11 for the keyboard. The terminal is the fourth. A host
can have any of these without the others.

**`move_to` counts from zero.** `move_to(0, 0)` is the top left corner. This
matches how the language indexes an array. A negative column or row is an error.

**The cursor returns when the program ends.** This happens for all three ways a
program can end: the last statement, an error, and `exit`. A program does not
have to call `show_cursor`. Section 8.11 gives the same rule for the terminal
settings that `read_key` changes.

#### Where the output is not a terminal

`clear`, `move_to`, `hide_cursor`, and `show_cursor` **do nothing** if the
program's output is not a terminal. This happens when the output goes to a file
or to another program. There is no screen, so there is nothing to do, and the
program continues. The output of the program does not contain the control
characters that a terminal needs.

`term_size` is different. It gives an **error**, and `try` catches it. There is
no correct number of columns for a file. A program that receives a wrong size
draws a picture of the wrong size. Section 8.9 gives the same rule for the
clock: a wrong answer is worse than a refusal.

A program that must run with its output in a file therefore must not call
`term_size`, or must catch the error.

All five give an error where the host has no terminal at all, such as the
browser playground.

---

## 9. Limits

A program that goes past a limit stops with the message in the table. Each
limit below was reached by a test program.

| Limit | Value | Message |
| ----- | ----- | ------- |
| Call depth | 10000 | `call depth limit of 10000 exceeded` |
| Call arguments | 255 | `too many call arguments` |
| Captured variables in one function | 255 | `too many captured variables in one function` |
| Local variables in scope | 65536 | `too many local variables in scope` |
| Constants in one chunk | 65536 | `too many constants in one chunk` |
| Functions in one chunk | 65536 | `too many functions in one chunk` |
| Global names in one program | 65536 | `too many global variables in one program` |
| Array literal elements | 65535 | `array literal has too many elements` |
| Map literal pairs | 65535 | `map literal has too many entries` |
| Jump distance in bytes | 65535 | `the compiled body is too large to jump over` |
| Loop body in bytes | 65535 | `the loop body is too large to compile` |
| Nesting for comparing and printing | 256 | `value is nested too deeply to compare` |
| Nesting in the source text | 1000 | `the program is nested too deeply` |
| Length of one expression | 10000 | `the expression is too long` |
| Hexadecimal digits in `\u{...}` | 6 | `escape sequence '\u{...}' takes at most 6 hexadecimal digits` |
| Exit code | 0 to 255 | `exit code must be from 0 to 255 but got <n>` |

The value stack has no limit. Only the call depth stops recursion.

The three nesting limits count different things, and none of them follows from
another.

The limit of 1000 counts how deeply a *program* nests: brackets inside
brackets, as in `[[[ ... ]]]`.

The limit of 10000 counts how *long* one expression is: the number of operators
in one chain, as in `a + b + c`, and equally a chain of index operations such as
`a[0][0][0]` or of field accesses such as `a.b.b.b`. Nothing is nested inside
anything in these, which is why the message is different.

The two figures differ because the constructs cost different amounts of memory
to read. Nesting is read by a procedure that calls itself once for each level.
A chain is read by a loop, which costs nothing per term, and shows its cost only
later, when the program is compiled or formatted.

The limit of 256 counts how deeply a *value* nests, which a loop can make much
deeper than the program that builds it.

The call depth limit is the one limit `try` cannot catch. Section 6.6 gives the
reason.

> **Note.** The limits on constants, functions, and global names are the number
> of different items, not the number of times a program uses one.

---

## 10. The command line

| Command | Purpose |
| ------- | ------- |
| `miru` | Starts the REPL. |
| `miru repl` | Starts the REPL. |
| `miru run FILE` | Runs a program. |
| `miru -e PROGRAM` | Runs a program supplied on the command line. |
| `miru --eval PROGRAM` | Runs a program supplied on the command line. |
| `miru fmt FILE` | Writes the program in the standard form. |
| `miru fmt -w FILE` | Writes the standard form into the file. |
| `miru disasm FILE` | Writes the bytecode of a program. |
| `miru --version` | Writes the version. |
| `miru --help` | Writes the usage. |

There are two exit codes. `0` means the program did all its work. `1` means an
error stopped it, or the command line was not correct.

> **Note.** `miru disasm` shows the bytecode. The bytecode is not part of this
> specification, and it can change in a 1.x release.
