# The MiruScriptX Language Specification

Version 1.0

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

An integer literal is a sequence of decimal digits.

There is no exponent form. There is no hexadecimal, octal, or binary form.
There is no digit separator. A literal cannot start with a minus sign: `-5` is
the negation operator and the literal `5`.

An integer literal must be in the range of a 64-bit signed integer. A literal
that is too large gives the error `integer literal '<text>' is out of range`.

> **Note.** The smallest integer, -9223372036854775808, has no literal form.
> Its magnitude is larger than the largest literal. Write
> `-9223372036854775807 - 1` instead.

### 2.7 Float literals

A float literal is a sequence of decimal digits, then `.`, then one or more
decimal digits.

A digit after the point is necessary. The text `1.` is the integer literal `1`
and then the `.` operator.

### 2.8 String literals

A string literal starts with `"` and ends with `"`.

A string literal cannot contain a newline character. A string literal that
reaches the end of a line, or the end of the source, gives the error
`unterminated string literal`.

These six escape sequences are permitted:

| Escape | Character |
| ------ | --------- |
| `\n` | Newline |
| `\t` | Tab |
| `\r` | Carriage return |
| `\\` | Backslash |
| `\"` | Quotation mark |
| `\0` | Null |

Another escape sequence gives the error `unknown escape sequence '\<c>'`. There
is no `\u` escape and no `\x` escape.

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
| Weakest | `try` | — |

`try` binds less strongly than every operator. `try a / b` applies `try` to the
division. Use parentheses to make `try` apply to less: `(try a) / b`.
