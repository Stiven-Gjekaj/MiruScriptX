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

All 41 builtins keep their names, their arguments, and their results. Section 8
of the specification lists them.

Four of them need something from the host: `read_file`, `write_file`,
`file_exists`, and `args`. What is stable is what they do **when the host gives
them a file system and a command line**, and that a host which does not is
refused rather than silently given nothing. `miru` gives both. A host that
embeds the language decides for itself, and the browser playground gives
neither.

A later 1.x can add a builtin. A later 1.x cannot remove one, and cannot change
what one does.

### 2.4 The command line

These commands and options keep their behaviour: `run`, `-e` and its long form
`--eval`, `fmt`, `fmt -w`, `disasm`, `repl`, `--version`, and `--help`.

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

**No program makes the process fail in a way the program cannot see.** This is
the general form. Three cases show what it means:

- A value that contains itself gives an error, or prints a mark. It does not
  stop the process.
- A value nested to any depth, and a chain of values of any length, is released
  without stopping the process. A loop can build one far deeper than any
  literal.
- Source nested past the limit in section 9 of the specification gives a syntax
  error with a line and a column.

Recursion that does not stop gives the call depth error. `try` cannot catch it.

Before 1.1 the first case was the whole promise, and the other two stopped the
process outright: no message, nothing to catch, and at the end of a program the
output still in the buffer was lost as well. The wording was narrow because the
defect was seen as one value rather than as a class.

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

The limit of 1000 on nesting in the source text can also change, and is more
likely to than the others. It is set by how much stack the tightest supported
build has. A later 1.x can raise it. No 1.x lowers it.

What does not change is what happens at the limit: a program that goes past it
stops with an error that gives a line and a column. Before 1.1 there was no
limit, and such a program stopped the whole process with no message.

### 3.3 The stack the limit assumes

This is a condition on the guarantee, not a limit. Read it if you use
MiruScriptX as a Rust library.

The interpreter walks its own structures by recursion, so the limit above holds
only where there is stack for it. Two builds supply that stack:

- The `miru` program runs its work on a thread of 64 MiB.
- The WebAssembly build links a shadow stack of 16 MiB.

**The `miruscriptx` crate does not.** A call to `run_source` uses the stack of
the thread that makes the call, which is 2 MiB by default and does not support a
limit of 1000. Give the call a thread with a larger stack:

```rust
std::thread::Builder::new()
    .stack_size(64 * 1024 * 1024)
    .spawn(|| miruscriptx::run_source(source, out))
```

Everything in section 2 holds when you do. Without it, deep source can still
stop the process.

### 3.4 The bytecode

The bytecode, the numbers of the opcodes, and the output of `miru disasm` are
not stable. They are how the language runs, not what it means.

### 3.5 The Rust API

The `miruscriptx` crate has public Rust items. They are not stable. They exist
for the `miru` program and for the playground.

### 3.6 The WebAssembly interface

The functions the playground uses are not stable.

### 3.7 Speed

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
