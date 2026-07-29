# The MiruScriptX Stability Guarantee

Version 1.0

This document tells you what will not change while MiruScriptX has the version
number 1. It is short on purpose. A promise that nobody can check is not a
promise.

The [specification](specification.md) says what the language does. This
document says which of those statements you can depend on.

---

## 1. The rule

A program that is correct with version 1.0 stays correct with every later
version 1.x.

"Correct" means the program does the same work and gives the same result. It
does not mean each character of each message stays the same. Section 3 gives
the difference.

---

## 2. What is stable

### 2.1 Syntax

Every program that parses with 1.0 parses with each later 1.x, and has the same
meaning. Sections 2 and 3 of the specification define this.

### 2.2 Semantics

The rules in section 5 of the specification are stable. This includes:

- The order of evaluation of each construct.
- The rules for numbers, and for overflow.
- Which values are true and which are false.
- The rules for equality and for order.
- The rules for names, for loops, and for functions.

### 2.3 The builtins

All 37 builtins keep their names, their arguments, and their results. Section 8
of the specification lists them.

A later 1.x can add a builtin. A later 1.x cannot remove one, and cannot change
what one does.

### 2.4 The command line

These commands and options keep their behaviour: `run`, `fmt`, `fmt -w`,
`disasm`, `repl`, `--version`, and `--help`.

There are two exit codes, `0` and `1`, and they keep their meanings.

### 2.5 The shape of an error

An error report keeps this shape:

- A line with the position and the message.
- The source line and a mark below it, when the error is in the file that ran.
- The name of the file, when the error is in a different file.
- One line for each call.

A caught error keeps its five fields: `message`, `line`, `column`, `file`, and
`trace`.

### 2.6 Termination

A program that makes a value that contains itself does not stop the process.
The language gives an error, or it prints a mark, but it does not fail in a way
the program cannot see.

Recursion that does not stop gives the call depth error. `try` cannot catch it.

### 2.7 Modules

An `import` resolves against the directory of the file that holds it. Each
module runs one time. A cycle is an error. A module gives the names it defines.

---

## 3. What is not stable

Do not depend on these. A later 1.x can change any of them.

### 3.1 The words in a message

The **shape** of an error report is stable. The **words** are not. A later 1.x
can make a message clearer.

Do not compare a message with a fixed string. Use `is_error` to find out that a
program holds an error. Read the `message` field to show it to a person.

### 3.2 The numbers in the limits

The limits in section 9 of the specification can change. A later 1.x can raise
one.

Two of them are recent, and neither has evidence behind the value: the nesting
limit of 256 for comparing and printing, and the mark `[...]` that printing
uses for a value that contains itself. Do not depend on either.

### 3.3 The bytecode

The bytecode, the numbers of the opcodes, and the output of `miru disasm` are
not stable. They are how the language runs, not what it means.

### 3.4 The Rust API

The `miruscriptx` crate has public Rust items. They are not stable. They exist
for the `miru` program and for the playground.

### 3.5 The WebAssembly interface

The functions the playground uses are not stable.

### 3.6 Speed

Speed is not part of the promise. A later 1.x can be faster or slower.

---

## 4. What a 1.x release can do

A later version 1.x can:

- Add a builtin.
- Add syntax, if each 1.0 program keeps its meaning.
- Make an error message clearer.
- Change a limit.
- Change the bytecode or the virtual machine.
- Change the speed.

---

## 5. What needs version 2

A change that makes a correct 1.0 program incorrect needs version 2. This
includes:

- Removal of a builtin, or a change to what one does.
- A change to the meaning of any syntax.
- A change to the order of evaluation.
- Removal of a command or an option.

---

## 6. If the implementation disagrees

If the implementation and the specification disagree, one of the two has a
defect. Tell the maintainer. Do not write a program that depends on the
disagreement.
