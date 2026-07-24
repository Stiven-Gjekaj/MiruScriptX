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

## Try the bytecode engine

MiruScriptX can run your program two ways. By default it walks the syntax tree
directly. It can also compile the program to bytecode and run that on a virtual
machine, which is faster:

```
miru run --vm hello.miru
```

Both engines run the same language and produce the same output, so `--vm` is
purely a speed choice. The bytecode engine is newer, so the tree-walking one
stays the default for now.

---
Previous: [Introduction](01-introduction.md) | Next: [Syntax basics](03-syntax-basics.md)
