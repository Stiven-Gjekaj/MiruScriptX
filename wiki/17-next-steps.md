# Next steps

You now know all of MiruScriptX 1.1. Here is where to go from here.

## Try it without installing anything

The [playground](https://stiven-gjekaj.github.io/MiruScriptX/) runs MiruScriptX
in your browser. It is the same lexer, compiler, and virtual machine as the
`miru` command, built to WebAssembly, so anything that works there works on your
machine and the other way round. It has the example programs ready to load, a
Format button, and a tab showing the bytecode your program compiles to.

What it cannot do is anything that needs a file, because a browser has no file
system to give it: `import`, `read_file`, and `write_file` all report there
rather than pretending. `file_exists` answers `false` and `args` gives an empty
array, since those are the honest answers when there is no file system and no
command line. `input` reads nothing, because there is no keyboard to read from.

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

---
Previous: [Handling errors](16-handling-errors.md)
