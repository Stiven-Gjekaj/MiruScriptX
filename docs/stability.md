# The MiruScriptX Stability Guarantee

Version 1.10

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

A later 1.x can add syntax. From the release that adds it, the new syntax has
the same promise as the rest: a later 1.x cannot remove it, and cannot change
what it means. This is the rule section 2.3 states for a builtin.

1.3 is the first release to use this. It adds the `\u{...}` escape sequence in
a string literal. No 1.0 program changes meaning, because `\u` was an error
before.

1.9 uses it three times, and each is stable from 1.9 in the same way:

- **Compound assignment**, `+=` and the four like it. `x += 1` was the error
  `expected an expression but found '='`.
- **Indexing a string**, `s[0]`. It was the error `cannot index a string`.
- **The `f"..."` literal**. `f"` was a parse error.

The third of those shows why the rule is worth keeping rather than working
around. Interpolating a plain `"${n}"` would have been shorter to write and to
explain, and it was rejected because `print("n is ${n}")` is a 1.0 program that
prints the braces. Changing it needs version 2; a prefix needs nothing.

1.10 uses it twice, and each is stable from 1.10 in the same way:

- **A second loop variable**, `for key, value in map`. It was the error
  `expected 'in' after the loop variable but found ','`.
- **A pattern in a binding**, `let [x, y] = pair`, and the same in either
  position of a `for`. `let [` was the error `expected an identifier after 'let'
  but found '['`.

What 1.10 did **not** add is part of the same rule. One loop variable over a map
stays an error, because a key and a value are each a reasonable reading of
`for x in m`: choosing either would be stable from 1.10 onward, and half of all
readers would be permanently wrong. An error can be given a meaning later. A
meaning cannot be taken back.

### 2.2 Semantics

The rules in section 5 of the specification are stable. This includes:

- The order of evaluation of each construct.
- The rules for numbers, and for overflow.
- Which values are true and which are false.
- The rules for equality and for order.
- The rules for names, for loops, and for functions.

### 2.3 The builtins

All 67 builtins keep their names, their arguments, and their results. Section 8
of the specification lists them.

Four of them need something from the host: `read_file`, `write_file`,
`file_exists`, and `args`. What is stable is what they do **when the host gives
them a file system and a command line**, and that a host which does not is
refused rather than silently given nothing. `miru` gives both. A host that
embeds the language decides for itself, and the browser playground gives
neither.

`now` and `sleep` need the host's clock, `read_key` and `key_ready` need its
keyboard, and `clear`, `move_to`, `hide_cursor`, `show_cursor`, and `term_size`
need a terminal, on the same terms. These are separate capabilities rather than
more file operations, and a host can have any of them without the others: the
playground has a clock, no file system, no keyboard, and no terminal. What is
stable is what each gives **when the host has the thing it needs**, and that a
host without it is refused.

**A host can have a clock and still refuse `sleep`**, which is the one place two
builtins on one capability part company. A page can say what the time is and
cannot spend it: a browser paints between turns of its event loop, so blocking
would freeze the tab rather than pace anything. That split is stable — `now`
working somewhere is not a promise that `sleep` does.

**Where the output is not a terminal, the four drawing builtins do nothing and
`term_size` refuses.** That difference is deliberate and stable. Nothing is the
honest result of "clear the screen" when the output is a file, and it is what
keeps a program's output free of control characters when it is redirected.
`term_size` has a number to give back and no true one to give, so it refuses
rather than inventing eighty columns.

**The key names `read_key` gives are stable.** Section 8.11 of the
specification lists them. A later 1.x can add a name for a key that gives
`"unknown"` today; it cannot change one that is already there.

A later 1.x can add a builtin. A later 1.x cannot remove one, and cannot change
what one does.

### 2.4 The command line

These commands and options keep their behaviour: `run`, `-e` and its long form
`--eval`, `fmt`, `fmt -w`, `disasm`, `repl`, `--version`, and `--help`.

`0` and `1` keep their meanings. `0` means the program did all its work. `1`
means an error stopped it.

Since 1.2 a program can choose its own code with `exit`, from 0 to 255. This
does not change what `0` and `1` mean, and it does not change what a program
that never calls `exit` gives back. Before 1.2 those were the only two codes,
because there was no way to ask for another.

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

**How many reports a program gets is not stable either.** Since 1.7 a program
with several syntax mistakes gets a report for each, where it used to get one.
The shape of each is what section 2.5 promises; how many appear, and which of
them a later 1.x decides are consequences of an earlier one rather than
separate mistakes, is not promised. Section 6.2.1 of the specification gives
the rules as they are today.

Do not compare a message with a fixed string. Use `is_error` to find out that a
program holds an error. Read the `message` field to show it to a person.

### 3.2 The numbers in the limits

The limits in section 9 of the specification can change. A later 1.x can raise
one.

Two of them are recent, and neither has evidence behind the value: the nesting
limit of 256 for comparing and printing, and the mark `[...]` that printing
uses for a value that contains itself. Do not depend on either.

The two limits on the source text can also change, and are more likely to than
the others: 1000 on how deeply a program nests, and 10000 on how long one
expression is. Each is set by how much stack the tightest supported build has.
A later 1.x can raise either. No 1.x lowers either.

Both were chosen to sit above what 1.0 did, and not only above what aborts.
1.0 had no limit, so it ran until the stack ran out, and its release binary on
the 1 MiB stack the playground had reached 917 levels of nesting and a sum of
4959 terms. Every program that worked on every 1.0 build still works.

A sum longer than about 10000 terms is the one case that does not. 1.0 reached
40255 of them on an 8 MiB main thread, and nothing near that in a browser, so
such a program ran on one build and stopped the process on another. Section 2.1
is about a program that was correct with 1.0, and a program that aborted the
process wherever the stack was smaller was not.

What does not change is what happens at either limit: a program that goes past
one stops with an error that gives a line and a column. Before 1.1 there was no
limit, and such a program stopped the whole process with no message.

### 3.3 The stack the limits assume

This is a condition on the guarantee, not a limit. Read it if you use
MiruScriptX as a Rust library.

The interpreter walks its own structures by recursion, so the limits above hold
only where there is stack for them. Two builds supply that stack:

- The `miru` program runs its work on a thread of 64 MiB.
- The WebAssembly build links a shadow stack of 16 MiB.

**The `miruscriptx` crate does not.** A call to `run_source` uses the stack of
the thread that makes the call, which is 2 MiB by default and does not support
either limit. Give the call a thread with a larger stack:

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

### 3.8 The numbers a seed produces

`random` gives a float from 0 up to but not including 1. `random_int(a, b)`
gives an integer from `a` to `b`. Those ranges are stable, and so is the rule
that two runs from the same seed give the same numbers.

**Which numbers is not stable.** A later 1.x can change the generator, and then
a seed gives a different sequence. A program that stores a seed and expects to
reproduce a result later needs the same release of MiruScriptX, and not only the
same seed.

This is a decision and not an omission. A promise about the sequence would fix
the algorithm for the whole of version 1, including any defect found in it.

It is last in this section rather than beside 3.1, which it sits closest to in
subject. The changelog refers to these subsections by number, so inserting one
in the middle would make a record of an earlier release point at the wrong
thing.

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
