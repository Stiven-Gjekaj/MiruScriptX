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

---
Previous: [Introduction](01-introduction.md) | Next: [Syntax basics](03-syntax-basics.md)
